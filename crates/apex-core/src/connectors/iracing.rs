//! Connecteur iRacing.
//!
//! - Import : fichiers `.ibt` (Documents\iRacing\telemetry), format irsdk public. ✅
//! - Live : mémoire partagée `Local\IRSDKMemMapFileName` + événement
//!   `Local\IRSDKDataValidEvent` (Windows). Code écrit, **non validé** en conditions réelles.

use super::*;
use crate::ingest::ibt;

pub struct IracingConnector;

pub struct IbtImporter;

impl SessionImporter for IbtImporter {
    fn extensions(&self) -> &'static [&'static str] {
        &["ibt"]
    }
    fn default_dir(&self) -> Option<PathBuf> {
        // %USERPROFILE%\Documents\iRacing\telemetry (OneDrive peut déplacer « Documents » : configurable)
        std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("Documents").join("iRacing").join("telemetry"))
    }
    fn import_file(&self, path: &Path) -> Result<ImportedSession, ConnectorError> {
        ibt::import_ibt_file(path).map_err(|e| ConnectorError::Import(e.to_string()))
    }
}

static IBT: IbtImporter = IbtImporter;

impl SimulatorConnector for IracingConnector {
    fn id(&self) -> SimId {
        SimId::Iracing
    }
    fn process_names(&self) -> &'static [&'static str] {
        &["iRacingSim64DX11.exe", "iRacingSim64.exe"]
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            live_telemetry: if cfg!(windows) { Support::Unverified } else { Support::Unavailable },
            post_session_import: Support::Implemented,
            position_xy: Support::Implemented,
            tyres: Support::Planned,
            damage: Support::Unavailable,
            weather: Support::Implemented,
            replay: Support::Unavailable,
            data_source: "Fichiers .ibt + mémoire partagée irsdk",
            notes: vec![
                "Activer l'enregistrement télémétrie en jeu (Alt+L) pour produire les .ibt.",
                "Position X/Y dérivée de Lat/Lon, présents uniquement dans les .ibt.",
                "Validité du tour déduite (hors-piste / incidents) : iRacing ne l'expose pas directement.",
                "Températures/usure pneus : mesurées uniquement aux stands dans iRacing.",
            ],
        }
    }
    fn detect_installation(&self, cfg: &ConnectorConfig) -> InstallStatus {
        if cfg.install_path.is_some() {
            return install_from_path(cfg, "iRacingSim64DX11.exe");
        }
        match IBT.default_dir() {
            Some(d) if d.exists() => InstallStatus { installed: Some(true), detail: format!("dossier télémétrie trouvé : {}", d.display()) },
            _ => InstallStatus { installed: None, detail: "dossier télémétrie par défaut introuvable".into() },
        }
    }
    fn importer(&self) -> Option<&dyn SessionImporter> {
        Some(&IBT)
    }
    #[cfg(windows)]
    fn open_live(&self, _cfg: &ConnectorConfig) -> Result<Box<dyn LiveSource>, ConnectorError> {
        Ok(Box::new(super::iracing_live::IrsdkLive::open()?))
    }
}

// ---------------------------------------------------------------------------
// Enregistrement automatique de la télémétrie (app.ini → irsdkAutoLogDisk).
// Clé documentée par la communauté : à 1, iRacing enregistre chaque session en .ibt
// sans avoir à appuyer sur Alt+L. iRacing réécrit app.ini à sa fermeture : ne
// modifier ce fichier que jeu fermé.
// ---------------------------------------------------------------------------

pub const AUTOLOG_KEY: &[u8] = b"irsdkAutoLogDisk";

/// Emplacements possibles de `app.ini` (Documents classique ou redirigé par OneDrive).
pub fn app_ini_candidates(telemetry_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut v = vec![];
    if let Some(d) = telemetry_dir.and_then(|d| d.parent()) {
        v.push(d.join("app.ini"));
    }
    if let Some(h) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        v.push(h.join("Documents").join("iRacing").join("app.ini"));
        v.push(h.join("OneDrive").join("Documents").join("iRacing").join("app.ini"));
    }
    v
}

/// Position (début de ligne, index du chiffre) de la valeur `irsdkAutoLogDisk=<n>`.
fn find_autolog(ini: &[u8]) -> Option<usize> {
    let mut start = 0;
    for line in ini.split(|&b| b == b'\n') {
        let trimmed = line.iter().position(|b| !b.is_ascii_whitespace()).map(|p| &line[p..]).unwrap_or(&[]);
        if trimmed.len() > AUTOLOG_KEY.len() && trimmed[..AUTOLOG_KEY.len()].eq_ignore_ascii_case(AUTOLOG_KEY) {
            let rest = &trimmed[AUTOLOG_KEY.len()..];
            let eq = rest.iter().position(|b| !b.is_ascii_whitespace())?;
            if rest[eq] == b'=' {
                let after = &rest[eq + 1..];
                let digit = after.iter().position(|b| !b.is_ascii_whitespace())?;
                if after[digit].is_ascii_digit() {
                    let offset = start + (line.len() - trimmed.len()) + AUTOLOG_KEY.len() + eq + 1 + digit;
                    return Some(offset);
                }
            }
        }
        start += line.len() + 1;
    }
    None
}

/// `Some(true)` si l'enregistrement automatique est actif, `None` si la clé est absente.
pub fn read_autolog(ini: &[u8]) -> Option<bool> {
    find_autolog(ini).map(|i| ini[i] != b'0')
}

/// Renvoie le contenu modifié ; le reste du fichier est conservé octet pour octet.
pub fn write_autolog(ini: &[u8], enable: bool) -> Result<Vec<u8>, String> {
    let i = find_autolog(ini).ok_or("réglage irsdkAutoLogDisk introuvable dans app.ini (version d'iRacing différente ?)")?;
    let mut out = ini.to_vec();
    out[i] = if enable { b'1' } else { b'0' };
    Ok(out)
}

#[cfg(test)]
mod autolog_tests {
    use super::*;

    const INI: &[u8] = b"[Graphics]\r\nfoo=1\r\n[Telemetry]\r\nirsdkAutoLogDisk=0                 ; record telemetry to disk automatically\r\nirsdkLog=0\r\n";

    #[test]
    fn toggles_only_the_digit() {
        assert_eq!(read_autolog(INI), Some(false));
        let on = write_autolog(INI, true).unwrap();
        assert_eq!(read_autolog(&on), Some(true));
        assert_eq!(on.len(), INI.len());
        let diff: Vec<_> = on.iter().zip(INI).enumerate().filter(|(_, (a, b))| a != b).collect();
        assert_eq!(diff.len(), 1, "un seul octet modifié");
        assert_eq!(write_autolog(&on, false).unwrap(), INI);
    }

    #[test]
    fn missing_key_is_an_error_not_a_guess() {
        assert_eq!(read_autolog(b"[Telemetry]\nirsdkLog=0\n"), None);
        assert!(write_autolog(b"[Telemetry]\nirsdkLog=0\n", true).is_err());
        // Une clé au nom proche ne doit pas être confondue.
        assert_eq!(read_autolog(b"irsdkAutoLogDiskX=1\n"), None);
    }
}
