//! Architecture par connecteurs : un connecteur par simulateur.
//!
//! Un connecteur **déclare honnêtement** ce qu'il sait faire via `Capabilities`.
//! `Support::Unverified` = code ou piste technique identifiée mais non validée
//! sur une vraie installation ; l'UI l'affiche comme tel.

pub mod ac;
pub mod iracing;
#[cfg(windows)]
pub mod iracing_live;
pub mod lmu;
pub mod lmu_shm;
#[cfg(windows)]
pub mod lmu_live;

use crate::model::{ImportedSession, SimId};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    /// Implémenté et testé.
    Implemented,
    /// Implémenté, non validé sur une installation réelle.
    Unverified,
    /// Source publique identifiée, pas encore implémenté.
    Planned,
    /// Pas de source publique fiable connue.
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
pub struct Capabilities {
    pub live_telemetry: Support,
    pub post_session_import: Support,
    pub position_xy: Support,
    pub tyres: Support,
    pub damage: Support,
    pub weather: Support,
    pub replay: Support,
    /// Source de données retenue, en clair (affichée dans Réglages).
    pub data_source: &'static str,
    pub notes: Vec<&'static str>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConnectorConfig {
    pub install_path: Option<PathBuf>,
    pub telemetry_dir: Option<PathBuf>,
    pub udp_port: Option<u16>,
    pub live_enabled: bool,
    /// Fréquence d'enregistrement live (Hz) : 60 = précis, 20 = économe.
    pub sample_rate_hz: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallStatus {
    pub installed: Option<bool>,
    pub detail: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("fonction non disponible pour ce simulateur : {0}")]
    NotSupported(&'static str),
    #[error("import : {0}")]
    Import(String),
    #[error("E/S : {0}")]
    Io(#[from] std::io::Error),
}

/// Import de sessions terminées depuis des fichiers.
pub trait SessionImporter: Send + Sync {
    /// Extensions de fichiers acceptées (sans point).
    fn extensions(&self) -> &'static [&'static str];
    /// Dossier par défaut où chercher les fichiers, s'il est connu.
    fn default_dir(&self) -> Option<PathBuf>;
    fn import_file(&self, path: &Path) -> Result<ImportedSession, ConnectorError>;

    fn scan(&self, dir: &Path) -> Result<Vec<PathBuf>, ConnectorError> {
        let mut out = vec![];
        for e in std::fs::read_dir(dir)? {
            let p = e?.path();
            let ok = p
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| self.extensions().iter().any(|e| e.eq_ignore_ascii_case(x)));
            if ok {
                out.push(p);
            }
        }
        out.sort();
        Ok(out)
    }
}

/// Instantané minimal d'une session live (statut + contexte).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LiveSnapshot {
    pub connected: bool,
    pub on_track: bool,
    pub session_time_s: f64,
    pub lap: i32,
    pub lap_dist_pct: f32,
    pub speed_kmh: f32,
    pub gear: i32,
    pub throttle: f32,
    pub brake: f32,
    pub tick: i64,
    /// iRacing : la télémétrie est-elle en cours d'enregistrement sur disque (.ibt) ?
    pub disk_logging: Option<bool>,
    /// Échantillon complet pour le coach en direct (ordre de `ingest::ibt::LIVE_CHANNELS`).
    #[serde(skip)]
    pub channels: Option<[f64; crate::ingest::ibt::LIVE_N]>,
    /// YAML de session, seulement quand iRacing l'a mis à jour depuis la lecture précédente.
    #[serde(skip)]
    pub session_info: Option<String>,
}

pub trait LiveSource: Send {
    /// Lecture non bloquante (timeout en ms) du dernier état ; `Ok(None)` = pas de donnée neuve.
    fn poll(&mut self, timeout_ms: u32) -> Result<Option<LiveSnapshot>, ConnectorError>;
}

pub trait SimulatorConnector: Send + Sync {
    fn id(&self) -> SimId;
    fn display_name(&self) -> &'static str {
        self.id().display_name()
    }
    /// Noms d'exécutables (Windows) servant à la détection de processus.
    fn process_names(&self) -> &'static [&'static str];
    fn capabilities(&self) -> Capabilities;
    fn detect_installation(&self, cfg: &ConnectorConfig) -> InstallStatus;
    fn importer(&self) -> Option<&dyn SessionImporter> {
        None
    }
    /// Ouvre la source live si le jeu la publie (mémoire partagée, UDP…).
    fn open_live(&self, _cfg: &ConnectorConfig) -> Result<Box<dyn LiveSource>, ConnectorError> {
        Err(ConnectorError::NotSupported("télémétrie live"))
    }
}

/// Registre de tous les connecteurs connus.
pub fn registry() -> Vec<Box<dyn SimulatorConnector>> {
    vec![
        Box::new(iracing::IracingConnector),
        Box::new(lmu::LmuConnector { recordings_dir: None }),
        Box::new(ac::AcConnector::assetto_corsa()),
        Box::new(ac::AcConnector::ac_evo()),
        Box::new(ac::AcConnector::acc()),
    ]
}

pub fn install_from_path(cfg: &ConnectorConfig, marker: &str) -> InstallStatus {
    match &cfg.install_path {
        Some(p) if p.join(marker).exists() => InstallStatus { installed: Some(true), detail: format!("trouvé : {}", p.display()) },
        Some(p) => InstallStatus { installed: Some(false), detail: format!("{marker} absent de {}", p.display()) },
        None => InstallStatus { installed: None, detail: "chemin d'installation non configuré".into() },
    }
}
