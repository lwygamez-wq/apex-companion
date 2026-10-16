//! Connecteur Le Mans Ultimate.
//!
//! État des sources publiques (vérifié sept. 2026, à revalider à chaque mise à jour du jeu) :
//! - Mémoire partagée native `LMU_Data`, publiée si *Réglages → Gameplay → Enable Plugins*
//!   est activé (redémarrage du jeu requis). Utilisée par plusieurs outils open source.
//! - Ancienne voie : plugin rFactor 2 Shared Memory Map (`$rFactor2SMMP_Telemetry$`…).
//! - API REST locale (port 6397) : météo prévue, classement, dégâts.
//! - Exports MoTeC `.ld` pour l'analyse après session.
//!
//! Aucune de ces structures n'est implémentée ici : la disposition mémoire de `LMU_Data`
//! n'est pas documentée officiellement par Studio 397. Plutôt que de deviner des offsets,
//! le connecteur se déclare `Planned` (piste : évaluer la crate Rust `kerb`, qui l'implémente).

use super::*;

pub struct LmuConnector;

impl SimulatorConnector for LmuConnector {
    fn id(&self) -> SimId {
        SimId::Lmu
    }
    fn process_names(&self) -> &'static [&'static str] {
        &["Le Mans Ultimate.exe", "LeMansUltimate.exe"]
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            live_telemetry: Support::Planned,
            post_session_import: Support::Planned,
            position_xy: Support::Planned,
            tyres: Support::Planned,
            damage: Support::Planned,
            weather: Support::Planned,
            replay: Support::Unavailable,
            data_source: "Mémoire partagée LMU_Data (à implémenter) / exports MoTeC .ld",
            notes: vec![
                "Activer « Enable Plugins » dans Réglages → Gameplay, puis redémarrer le jeu.",
                "Disposition mémoire non documentée officiellement : à valider sur chaque version du jeu.",
            ],
        }
    }
    fn detect_installation(&self, cfg: &ConnectorConfig) -> InstallStatus {
        install_from_path(cfg, "Le Mans Ultimate.exe")
    }
}
