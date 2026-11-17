//! Export « retour testeur » : un fichier .zip à envoyer au développeur, fait à la demande du pilote.
//!
//! Contenu : la note du pilote, la version, le journal d'Apex (valeurs sensibles masquées), les derniers
//! enregistrements Le Mans Ultimate et ce que le jeu a annoncé (sonde). Jamais la base (`apex.db`), les
//! réglages, les clés ou les jetons. Rien n'est envoyé : le pilote choisit où écrire le fichier.

use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Ce que le pilote exporte.
pub struct FeedbackInput<'a> {
    pub version: &'a str,
    /// Texte libre du pilote (ce qui s'est passé, ce qu'il attendait).
    pub note: &'a str,
    /// Dossier du journal d'Apex.
    pub logs_dir: Option<&'a Path>,
    /// Dossier des données (contient `enregistrements-lmu` et `sonde-lmu`).
    pub data_dir: Option<&'a Path>,
    /// Derniers enregistrements LMU à joindre.
    pub recordings: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FeedbackSummary {
    pub path: PathBuf,
    /// Fichiers mis dans l'archive (chemins dans l'archive).
    pub files: Vec<String>,
    pub bytes: u64,
}

/// Fichiers d'un dossier dont le nom finit par `ext`, du plus récent au plus ancien.
fn newest(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut v: Vec<(SystemTime, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(ext)))
        .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    v.into_iter().map(|(_, p)| p).collect()
}

/// Masque la valeur qui suit un mot sensible suivi de « : » ou « = » (`token`, `key`, `secret`,
/// `password`, `authorization`) : le reste de la ligne devient « [masqué] ».
pub fn redact_line(line: &str) -> String {
    const WORDS: [&str; 5] = ["token", "key", "secret", "password", "authorization"];
    let low = line.to_lowercase();
    let mut cut: Option<usize> = None;
    for w in WORDS {
        for (i, _) in low.match_indices(w) {
            let after = &low[i + w.len()..];
            let sep = after.len() - after.trim_start_matches([' ', '"']).len();
            if after[sep..].starts_with([':', '=']) {
                let end = i + w.len() + sep + 1;
                let end = end + (low[end..].len() - low[end..].trim_start_matches([' ', '"']).len());
                cut = Some(cut.map_or(end, |c| c.min(end)));
            }
        }
    }
    match cut {
        // Minuscules de longueur différente (rare hors ASCII) : toute la ligne est masquée.
        Some(c) if low.len() == line.len() && line.is_char_boundary(c) => format!("{}[masqué]", &line[..c]),
        Some(_) => "[ligne masquée]".to_string(),
        None => line.to_string(),
    }
}

pub fn export_feedback(out: &Path, i: &FeedbackInput) -> Result<FeedbackSummary, String> {
    let file = std::fs::File::create(out).map_err(|e| format!("écriture impossible ({}) : {e}", out.display()))?;
    let mut z = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true);
    let mut files = vec![];
    let mut put = |z: &mut zip::ZipWriter<std::fs::File>, name: String, data: &[u8]| -> Result<(), String> {
        z.start_file(name.as_str(), opts).map_err(|e| e.to_string())?;
        z.write_all(data).map_err(|e| e.to_string())?;
        files.push(name);
        Ok(())
    };
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M");
    let readme = format!(
        "Retour testeur Apex Companion\nVersion : {}\nExporté le : {now}\nSystème : {}\n\nNote du pilote :\n{}\n\nContenu : journal d'Apex (valeurs sensibles masquées), derniers enregistrements Le Mans Ultimate, ce que le jeu a annoncé. Ni base de données, ni réglages, ni clés.\n",
        i.version,
        std::env::consts::OS,
        if i.note.trim().is_empty() { "(aucune)" } else { i.note.trim() }
    );
    put(&mut z, "LISEZ-MOI.txt".into(), readme.as_bytes())?;
    if let Some(dir) = i.logs_dir {
        for p in newest(dir, ".log").into_iter().take(3) {
            let Ok(b) = std::fs::read(&p) else { continue };
            let text: String = String::from_utf8_lossy(&b).lines().map(redact_line).collect::<Vec<_>>().join("\n");
            put(&mut z, format!("journal/{}", p.file_name().unwrap_or_default().to_string_lossy()), text.as_bytes())?;
        }
    }
    if let Some(data) = i.data_dir {
        for p in newest(&data.join("enregistrements-lmu"), ".apexlmu").into_iter().take(i.recordings) {
            let Ok(b) = std::fs::read(&p) else { continue };
            put(&mut z, format!("enregistrements-lmu/{}", p.file_name().unwrap_or_default().to_string_lossy()), &b)?;
        }
        let probe = data.join("sonde-lmu");
        if let Ok(b) = std::fs::read(probe.join("jeu.json")) {
            put(&mut z, "sonde-lmu/jeu.json".into(), &b)?;
        }
        // Réponses brutes du jeu des 3 dernières séances (sans le schéma de l'API, volumineux et inutile ici).
        let mut sessions: Vec<(SystemTime, PathBuf)> = std::fs::read_dir(&probe).into_iter().flatten().flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).collect();
        sessions.sort_by_key(|s| std::cmp::Reverse(s.0));
        for (_, d) in sessions.into_iter().take(3) {
            let name = d.file_name().unwrap_or_default().to_string_lossy().to_string();
            for p in newest(&d, ".json").into_iter().filter(|p| !p.ends_with("swagger.json")) {
                let Ok(b) = std::fs::read(&p) else { continue };
                put(&mut z, format!("sonde-lmu/{name}/{}", p.file_name().unwrap_or_default().to_string_lossy()), &b)?;
            }
        }
    }
    z.finish().map_err(|e| e.to_string())?;
    let bytes = std::fs::metadata(out).map(|m| m.len()).unwrap_or(0);
    Ok(FeedbackSummary { path: out.to_path_buf(), files, bytes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_secrets_in_log_lines() {
        assert_eq!(redact_line("[info] garage61 token: abc123 ok"), "[info] garage61 token: [masqué]");
        assert_eq!(redact_line("api_key=SECRET"), "api_key=[masqué]");
        assert_eq!(redact_line("coach : Box, box, ce tour."), "coach : Box, box, ce tour.");
    }

    /// Fichiers synthétiques : l'archive contient la note, le journal masqué, les derniers enregistrements
    /// et la sonde ; jamais la base ni le schéma de l'API.
    #[test]
    fn exports_logs_recordings_and_probe_without_the_database() {
        let d = tempfile::tempdir().unwrap();
        let logs = d.path().join("logs");
        let data = d.path().join("data");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::create_dir_all(data.join("enregistrements-lmu")).unwrap();
        std::fs::create_dir_all(data.join("sonde-lmu/course1")).unwrap();
        std::fs::write(logs.join("Apex Companion.log"), "coach : Box, box\nAzure key: zzz\n").unwrap();
        std::fs::write(data.join("apex.db"), "base").unwrap();
        for n in ["a", "b", "c", "d"] {
            std::fs::write(data.join("enregistrements-lmu").join(format!("{n}.apexlmu")), n).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        std::fs::write(data.join("enregistrements-lmu/x.apexlmu.part"), "provisoire").unwrap();
        std::fs::write(data.join("sonde-lmu/jeu.json"), "{}").unwrap();
        std::fs::write(data.join("sonde-lmu/course1/pit-menu.json"), "[]").unwrap();
        std::fs::write(data.join("sonde-lmu/course1/swagger.json"), "{}").unwrap();
        let out = d.path().join("retour.zip");
        let s = export_feedback(&out, &FeedbackInput { version: "0.39.0", note: "Box trop tôt à Imola", logs_dir: Some(&logs), data_dir: Some(&data), recordings: 3 }).unwrap();
        let mut names = s.files.clone();
        names.sort();
        assert_eq!(names, ["LISEZ-MOI.txt", "enregistrements-lmu/b.apexlmu", "enregistrements-lmu/c.apexlmu", "enregistrements-lmu/d.apexlmu", "journal/Apex Companion.log", "sonde-lmu/course1/pit-menu.json", "sonde-lmu/jeu.json"]);
        let mut z = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        let mut log = String::new();
        std::io::Read::read_to_string(&mut z.by_name("journal/Apex Companion.log").unwrap(), &mut log).unwrap();
        assert_eq!(log, "coach : Box, box\nAzure key: [masqué]");
        let mut readme = String::new();
        std::io::Read::read_to_string(&mut z.by_name("LISEZ-MOI.txt").unwrap(), &mut readme).unwrap();
        assert!(readme.contains("Version : 0.39.0") && readme.contains("Box trop tôt à Imola"));
    }
}
