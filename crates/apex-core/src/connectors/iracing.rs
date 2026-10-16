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
