//! Connecteurs de la famille Assetto Corsa (AC1, AC EVO, ACC).
//!
//! - AC1 : mémoire partagée documentée par Kunos (`Local\acpmf_physics`,
//!   `acpmf_graphics`, `acpmf_static`). Piste solide, non implémentée à ce stade.
//! - AC EVO : mémoire partagée native annoncée (utilisée par des outils tiers),
//!   format encore susceptible d'évoluer (accès anticipé) → `Planned`, à vérifier.
//! - ACC : même famille de mémoire partagée + API broadcasting UDP. Non prioritaire.

use super::*;

pub struct AcConnector {
    id: SimId,
    processes: &'static [&'static str],
    source: &'static str,
    notes: &'static [&'static str],
    live: Support,
}

impl AcConnector {
    pub fn assetto_corsa() -> Self {
        Self {
            id: SimId::AssettoCorsa,
            processes: &["acs.exe", "acs_x86.exe"],
            source: "Mémoire partagée Kunos acpmf_* (documentée)",
            notes: &["Interface documentée publiquement par Kunos ; implémentation prévue en phase 3."],
            live: Support::Planned,
        }
    }
    pub fn ac_evo() -> Self {
        Self {
            id: SimId::AcEvo,
            processes: &["AssettoCorsaEVO.exe"],
            source: "Mémoire partagée native AC EVO (format à confirmer)",
            notes: &[
                "Jeu en accès anticipé : format de télémétrie susceptible de changer.",
                "Nom de processus à confirmer sur ta machine (configurable).",
            ],
            live: Support::Planned,
        }
    }
    pub fn acc() -> Self {
        Self {
            id: SimId::Acc,
            processes: &["AC2-Win64-Shipping.exe"],
            source: "Mémoire partagée acpmf_* + broadcasting UDP",
            notes: &["Non prioritaire."],
            live: Support::Planned,
        }
    }
}

impl SimulatorConnector for AcConnector {
    fn id(&self) -> SimId {
        self.id
    }
    fn process_names(&self) -> &'static [&'static str] {
        self.processes
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            live_telemetry: self.live,
            post_session_import: Support::Unavailable,
            position_xy: Support::Planned,
            tyres: Support::Planned,
            damage: Support::Planned,
            weather: Support::Planned,
            replay: Support::Unavailable,
            data_source: self.source,
            notes: self.notes.to_vec(),
        }
    }
    fn detect_installation(&self, cfg: &ConnectorConfig) -> InstallStatus {
        install_from_path(cfg, self.processes[0])
    }
}
