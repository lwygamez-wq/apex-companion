//! Mises à jour de l'application.
//!
//! Fonctionnement : on lit `latest.json` (publié par le workflow GitHub avec chaque
//! release), on compare la version, puis on télécharge l'installeur, on **vérifie sa
//! signature minisign** avec la clé publique ci-dessous, et seulement alors on le lance.
//! Un installeur modifié ou non signé par notre clé privée est refusé.
//!
//! Même format que le plugin officiel `tauri-plugin-updater` (indisponible pour
//! l'instant : sa dépendance `zip` a été retirée de crates.io). On pourra y revenir
//! sans rien changer côté publication.

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

const MANIFEST_URL: &str = "https://github.com/lwygamez-wq/apex-companion/releases/latest/download/latest.json";
/// Seule origine autorisée pour les installeurs.
const ALLOWED_PREFIX: &str = "https://github.com/lwygamez-wq/apex-companion/releases/download/";
const PLATFORM: &str = "windows-x86_64";
/// Clé publique minisign (la clé privée n'existe que dans les secrets GitHub).
const PUBKEY_B64: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDkyRDUyMzkwQzI5MTZBQzIKUldUQ2FwSENrQ1BWa3EyQ2l3eUZCOUkwQndZOXZaQzFvd2VPZGJaWENVczF0dEdpM0psRlhyYlUK";
const MAX_INSTALLER_BYTES: u64 = 500 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct Manifest {
    version: String,
    notes: Option<String>,
    platforms: HashMap<String, Platform>,
}

#[derive(Debug, Deserialize)]
struct Platform {
    signature: String,
    url: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
}

fn parse_version(v: &str) -> Vec<u64> {
    v.trim().trim_start_matches('v').split(['-', '+']).next().unwrap_or("").split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// `true` si `remote` est strictement plus récente que `current` (comparaison semver simple).
pub fn is_newer(remote: &str, current: &str) -> bool {
    let (mut r, mut c) = (parse_version(remote), parse_version(current));
    let n = r.len().max(c.len()).max(3);
    r.resize(n, 0);
    c.resize(n, 0);
    r > c
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(300))).build().into()
}

fn fetch_manifest() -> Result<Manifest, String> {
    let mut resp = agent()
        .get(MANIFEST_URL)
        .header("User-Agent", "apex-companion-updater")
        .call()
        .map_err(|e| match e {
            ureq::Error::StatusCode(404) => "aucune version publiée pour l'instant".to_string(),
            e => format!("connexion à GitHub impossible ({e})"),
        })?;
    let text = resp.body_mut().read_to_string().map_err(|e| format!("lecture de latest.json : {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("latest.json invalide : {e}"))
}

/// Vérifie la signature minisign (format Tauri : texte de signature encodé en base64).
pub fn verify_signature(data: &[u8], signature_b64: &str) -> Result<(), String> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let pk_text = String::from_utf8(b64.decode(PUBKEY_B64).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let pk = minisign_verify::PublicKey::decode(&pk_text).map_err(|e| format!("clé publique : {e}"))?;
    let sig_text = String::from_utf8(b64.decode(signature_b64.trim()).map_err(|e| format!("signature illisible : {e}"))?)
        .map_err(|e| format!("signature illisible : {e}"))?;
    let sig = minisign_verify::Signature::decode(&sig_text).map_err(|e| format!("signature illisible : {e}"))?;
    pk.verify(data, &sig, false).map_err(|_| "signature invalide : installeur refusé".to_string())
}

pub fn check(current: &str) -> Result<Option<UpdateInfo>, String> {
    let m = fetch_manifest()?;
    Ok(is_newer(&m.version, current).then(|| UpdateInfo { version: m.version, notes: m.notes.filter(|n| !n.trim().is_empty()) }))
}

/// Télécharge, vérifie et lance l'installeur. L'appelant ferme ensuite l'application.
pub fn download_and_launch(current: &str) -> Result<String, String> {
    let m = fetch_manifest()?;
    if !is_newer(&m.version, current) {
        return Err("tu as déjà la dernière version".into());
    }
    let p = m.platforms.get(PLATFORM).ok_or("cette version ne contient pas d'installeur Windows")?;
    if !p.url.starts_with(ALLOWED_PREFIX) {
        return Err("adresse de téléchargement non reconnue : installeur refusé".into());
    }
    let mut resp = agent()
        .get(&p.url)
        .header("User-Agent", "apex-companion-updater")
        .call()
        .map_err(|e| format!("téléchargement impossible ({e})"))?;
    let bytes = resp
        .body_mut()
        .with_config()
        .limit(MAX_INSTALLER_BYTES)
        .read_to_vec()
        .map_err(|e| format!("téléchargement interrompu : {e}"))?;
    verify_signature(&bytes, &p.signature)?;
    let path = std::env::temp_dir().join(format!("ApexCompanion-{}-setup.exe", m.version));
    std::fs::write(&path, &bytes).map_err(|e| format!("écriture de l'installeur : {e}"))?;
    std::process::Command::new(&path).spawn().map_err(|e| format!("lancement de l'installeur : {e}"))?;
    log::info!("mise à jour {} lancée ({} octets, signature vérifiée)", m.version, bytes.len());
    Ok(m.version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("0.3.1", "0.3.0"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(!is_newer("0.2.9", "0.3.0"));
        assert!(!is_newer("0.3.0-beta", "0.3.0"));
    }

    // Signature produite par `tauri signer sign` avec la clé privée du projet, sur le contenu ci-dessous.
    const SAMPLE: &[u8] = include_bytes!("../tests/updater-sample.txt");
    const SAMPLE_SIG: &str = include_str!("../tests/updater-sample.txt.sig");

    #[test]
    fn accepts_genuine_signature() {
        verify_signature(SAMPLE, SAMPLE_SIG).unwrap();
    }

    #[test]
    fn rejects_tampered_installer() {
        let mut bad = SAMPLE.to_vec();
        bad[0] ^= 1;
        assert!(verify_signature(&bad, SAMPLE_SIG).is_err());
        assert!(verify_signature(SAMPLE, "pas une signature").is_err());
    }

    /// Vérifie une release réellement publiée : `APEX_E2E_DIR` contient latest.json et setup.exe téléchargés.
    #[test]
    fn published_release_signature() {
        let Some(dir) = std::env::var_os("APEX_E2E_DIR") else { return };
        let dir = std::path::PathBuf::from(dir);
        let m: Manifest = serde_json::from_str(&std::fs::read_to_string(dir.join("latest.json")).unwrap()).unwrap();
        let p = &m.platforms[PLATFORM];
        assert!(p.url.starts_with(ALLOWED_PREFIX));
        verify_signature(&std::fs::read(dir.join("setup.exe")).unwrap(), &p.signature).unwrap();
        println!("release {} : signature valide", m.version);
    }
}
