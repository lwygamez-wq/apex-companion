//! Sonde de la télémétrie écrite par Le Mans Ultimate lui-même (lecture seule).
//!
//! D'après des outils communautaires (non vérifié sur le PC d'Alex), LMU écrit depuis sa version 1.2 un
//! fichier DuckDB par séance dans `UserData\Telemetry` quand l'enregistrement est activé (option
//! « Automatically Record Telemetry » ou touche dédiée), avec une table par canal et le setup en JSON
//! dans les métadonnées. Cette sonde ne lit que la liste des fichiers, leur en-tête (16 octets, pour
//! reconnaître le format) et les réglages du jeu qui parlent de télémétrie. Elle n'ouvre ni ne copie
//! aucun contenu : le verdict décide s'il vaut la peine d'ajouter un lecteur DuckDB à Apex.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// Un fichier du dossier de télémétrie du jeu.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GameTelemetryFile {
    pub name: String,
    pub size_bytes: u64,
    pub modified_unix: i64,
    /// Format reconnu à l'en-tête : « duckdb », « motec-ld » ou `None`.
    pub format: Option<String>,
}

/// Rapport de la sonde (`sonde-lmu/telemetrie-jeu.json`).
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct GameTelemetryReport {
    pub userdata_dir: Option<String>,
    pub telemetry_dir: Option<String>,
    pub telemetry_dir_exists: bool,
    pub files: Vec<GameTelemetryFile>,
    /// Réglages du jeu dont le nom contient « telemetry » (fichier, clé, valeur brute).
    pub settings: Vec<(String, String, String)>,
    pub verdict: String,
}

/// Reconnaît le format à l'en-tête : DuckDB porte « DUCK » à l'octet 8 ; MoTeC .ld commence par 0x40.
pub fn sniff(head: &[u8], name: &str) -> Option<String> {
    if head.len() >= 12 && &head[8..12] == b"DUCK" {
        return Some("duckdb".into());
    }
    if name.to_ascii_lowercase().ends_with(".ld") && head.first() == Some(&0x40) {
        return Some("motec-ld".into());
    }
    None
}

/// Clés de réglage qui parlent de télémétrie, dans un JSON du jeu (parcours récursif).
pub fn telemetry_settings(file: &str, json: &serde_json::Value, out: &mut Vec<(String, String, String)>) {
    match json {
        serde_json::Value::Object(m) => {
            for (k, v) in m {
                if k.to_ascii_lowercase().contains("telemetry") && !v.is_object() && !v.is_array() {
                    out.push((file.to_string(), k.clone(), v.to_string()));
                }
                telemetry_settings(file, v, out);
            }
        }
        serde_json::Value::Array(a) => a.iter().for_each(|v| telemetry_settings(file, v, out)),
        _ => {}
    }
}

/// Inventaire du dossier `UserData` de LMU (lecture seule).
pub fn probe(userdata: &Path) -> GameTelemetryReport {
    let mut r = GameTelemetryReport { userdata_dir: Some(userdata.display().to_string()), ..Default::default() };
    let tdir = userdata.join("Telemetry");
    r.telemetry_dir = Some(tdir.display().to_string());
    r.telemetry_dir_exists = tdir.is_dir();
    if let Ok(rd) = std::fs::read_dir(&tdir) {
        for e in rd.flatten().filter(|e| e.path().is_file()) {
            let name = e.file_name().to_string_lossy().into_owned();
            let md = e.metadata().ok();
            let mut head = [0u8; 16];
            let n = std::fs::File::open(e.path()).and_then(|mut f| std::io::Read::read(&mut f, &mut head)).unwrap_or(0);
            r.files.push(GameTelemetryFile {
                format: sniff(&head[..n], &name),
                size_bytes: md.as_ref().map(|m| m.len()).unwrap_or(0),
                modified_unix: md.and_then(|m| m.modified().ok()).and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0),
                name,
            });
        }
    }
    r.files.sort_by(|a, b| b.modified_unix.cmp(&a.modified_unix).then(a.name.cmp(&b.name)));
    if let Ok(rd) = std::fs::read_dir(userdata.join("player")) {
        let mut jsons: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("json"))).collect();
        jsons.sort();
        for p in jsons {
            let Ok(txt) = std::fs::read_to_string(&p) else { continue };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) else { continue };
            telemetry_settings(&p.file_name().unwrap_or_default().to_string_lossy(), &v, &mut r.settings);
        }
    }
    let duck = r.files.iter().filter(|f| f.format.as_deref() == Some("duckdb")).count();
    r.verdict = if duck > 0 {
        format!("{duck} fichier(s) DuckDB écrits par le jeu : format présent. Étape suivante : lire les tables (canaux, fréquence) sur une copie.")
    } else if r.telemetry_dir_exists {
        format!("dossier présent, {} fichier(s), aucun DuckDB : enregistrement du jeu probablement désactivé.", r.files.len())
    } else {
        "pas de dossier Telemetry : le jeu n'enregistre pas sa télémétrie sur ce PC.".into()
    };
    r
}

/// Dossiers `UserData` de LMU (à côté du dossier des setups trouvé dans Steam).
pub fn find_userdata_dirs() -> Vec<PathBuf> {
    crate::setup::find_settings_dirs().into_iter().filter_map(|s| Some(s.parent()?.parent()?.to_path_buf())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_lists_files_formats_and_settings_without_reading_content() {
        let d = tempfile::tempdir().unwrap();
        let ud = d.path();
        std::fs::create_dir_all(ud.join("Telemetry")).unwrap();
        std::fs::create_dir_all(ud.join("player")).unwrap();
        let mut duck = vec![0u8; 8];
        duck.extend_from_slice(b"DUCK");
        duck.extend_from_slice(&[0u8; 100]);
        std::fs::write(ud.join("Telemetry").join("Monza_P_2026-11-21.duckdb"), &duck).unwrap();
        std::fs::write(ud.join("Telemetry").join("notes.txt"), b"bonjour").unwrap();
        std::fs::write(ud.join("player").join("Settings.JSON"), r#"{"Misc":{"Automatically Record Telemetry":true,"Other":1}}"#).unwrap();
        let r = probe(ud);
        assert!(r.telemetry_dir_exists);
        assert_eq!(r.files.len(), 2);
        let f = r.files.iter().find(|f| f.name.ends_with(".duckdb")).unwrap();
        assert_eq!((f.format.as_deref(), f.size_bytes), (Some("duckdb"), 112));
        assert_eq!(r.files.iter().find(|f| f.name == "notes.txt").unwrap().format, None);
        assert_eq!(r.settings, vec![("Settings.JSON".into(), "Automatically Record Telemetry".into(), "true".into())]);
        assert!(r.verdict.starts_with("1 fichier(s) DuckDB"), "{}", r.verdict);
    }

    #[test]
    fn missing_folder_gives_a_clear_verdict() {
        let d = tempfile::tempdir().unwrap();
        let r = probe(d.path());
        assert!(!r.telemetry_dir_exists && r.files.is_empty());
        assert!(r.verdict.contains("pas de dossier Telemetry"));
    }
}
