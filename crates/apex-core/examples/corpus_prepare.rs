//! Prépare un fichier réel pour le corpus de non-régression (dépôt privé `apex-corpus`).
//!
//! `cargo run -p apex-core --example corpus_prepare -- <sortie/> fichier.ibt [fichier.apexlmu …]`
//!
//! - `.ibt` : les noms et identifiants des pilotes (le tien compris) sont remplacés dans les infos de
//!   séance par des valeurs de même longueur (les décalages du fichier restent valides), puis le
//!   fichier est compressé (`.ibt.gz`). La télémétrie n'est pas touchée.
//! - `.apexlmu` : format Apex, sans nom de pilote ; seulement compressé.
//!
//! Le fichier préparé est relu et comparé à l'original (mêmes tours, mêmes temps) avant d'être écrit.

use flate2::{write::GzEncoder, Compression};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

/// Champs des infos de séance iRacing qui identifient une personne (ou mènent à elle).
const PERSONAL: &[&str] = &[
    "UserName", "AbbrevName", "Initials", "TeamName", "UserID", "TeamID", "DriverUserID", "ClubName", "ClubID", "FlairName", "FlairID",
    "SubSessionID", "SessionID", "LeagueID", "LeagueSeasonID",
];

/// Remplace la valeur par une valeur de même longueur en octets : chiffres → chiffres, texte → lettres.
/// Une même valeur donne toujours le même remplacement (le pilote reste reconnaissable d'un champ à l'autre).
fn mask(v: &[u8], map: &mut HashMap<Vec<u8>, Vec<u8>>) -> Vec<u8> {
    let n = map.len();
    map.entry(v.to_vec())
        .or_insert_with(|| {
            // Identifiant : numéro d'ordre aligné à droite, même nombre de chiffres (« 1238469 » → « 0000007 »).
            let numeric = v.iter().all(|c| c.is_ascii_digit());
            let seed = format!("{:0>w$}", n + 1, w = v.len()).into_bytes();
            let skip = seed.len() - v.len();
            v.iter()
                .enumerate()
                .map(|(i, c)| match () {
                    _ if numeric => seed[skip + i],
                    _ if *c == b' ' || *c == b',' || *c == b'"' => *c,
                    _ => b"PILOTEX"[(i + n) % 7],
                })
                .collect()
        })
        .clone()
}

fn anonymize_ibt(b: &mut [u8]) -> usize {
    let len = i32::from_le_bytes(b[16..20].try_into().unwrap()) as usize;
    let off = i32::from_le_bytes(b[20..24].try_into().unwrap()) as usize;
    let mut map = HashMap::new();
    let mut changed = 0;
    let yaml = &mut b[off..off + len];
    let mut start = 0;
    while start < yaml.len() {
        let end = yaml[start..].iter().position(|c| *c == b'\n').map(|p| start + p).unwrap_or(yaml.len());
        let line = &yaml[start..end];
        let key_start = line.iter().position(|c| !matches!(c, b' ' | b'-')).unwrap_or(line.len());
        if let Some(colon) = line[key_start..].iter().position(|c| *c == b':').map(|p| key_start + p) {
            let key = std::str::from_utf8(&line[key_start..colon]).unwrap_or("");
            let vstart = colon + 2;
            if PERSONAL.contains(&key) && vstart < line.len() {
                let mut vend = line.len();
                while vend > vstart && matches!(line[vend - 1], b'\r' | b' ') {
                    vend -= 1;
                }
                let masked = mask(&line[vstart..vend], &mut map);
                yaml[start + vstart..start + vend].copy_from_slice(&masked);
                changed += 1;
            }
        }
        start = end + 1;
    }
    changed
}

fn fingerprint(p: &Path) -> Vec<(i32, Option<f64>)> {
    let s = if p.extension().is_some_and(|e| e == "ibt") {
        apex_core::ingest::ibt::import_ibt_file(p).unwrap()
    } else {
        let r = apex_core::ingest::lmu::decode(&std::fs::read(p).unwrap()).unwrap();
        apex_core::ingest::lmu::import_recording(&r).unwrap()
    };
    s.laps.iter().map(|l| (l.meta.number, l.meta.lap_time_s)).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = Path::new(&args[0]);
    std::fs::create_dir_all(out).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    for f in &args[1..] {
        let src = Path::new(f);
        let name = src.file_name().unwrap().to_string_lossy().replace(' ', "_");
        let mut b = std::fs::read(src).unwrap();
        let changed = if name.ends_with(".ibt") { anonymize_ibt(&mut b) } else { 0 };
        let check = tmp.path().join(&name);
        std::fs::write(&check, &b).unwrap();
        assert_eq!(fingerprint(src), fingerprint(&check), "{name} : l'anonymisation a changé les tours");
        let dest = out.join(format!("{name}.gz"));
        let mut gz = GzEncoder::new(std::fs::File::create(&dest).unwrap(), Compression::best());
        gz.write_all(&b).unwrap();
        gz.finish().unwrap();
        println!("{} : {} champs anonymisés, {} Mo → {} Mo", name, changed, b.len() / 1_000_000, std::fs::metadata(&dest).unwrap().len() / 1_000_000);
    }
}
