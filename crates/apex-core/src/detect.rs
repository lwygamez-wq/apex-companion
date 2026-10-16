//! Détection des simulateurs et machine à états de session.
//!
//! La logique de transition est pure (`SessionMonitor::step`) et testée ;
//! l'observation (processus, source live) est injectée par l'appelant.

use crate::connectors::SimulatorConnector;
use crate::model::SimId;
use serde::Serialize;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum MonitorState {
    NoSimulator,
    GameDetected { sim: SimId },
    WaitingForSession { sim: SimId },
    SessionActive { sim: SimId },
    SessionEnded { sim: SimId },
    Importing { detail: String },
}

impl MonitorState {
    pub fn label_fr(&self) -> &'static str {
        match self {
            MonitorState::NoSimulator => "Aucun simulateur détecté",
            MonitorState::GameDetected { .. } => "Jeu détecté",
            MonitorState::WaitingForSession { .. } => "En attente de session",
            MonitorState::SessionActive { .. } => "Session active",
            MonitorState::SessionEnded { .. } => "Session terminée",
            MonitorState::Importing { .. } => "Import/analyse en cours",
        }
    }
}

/// Observation à un instant t.
#[derive(Debug, Clone, Default)]
pub struct Observation {
    /// Simulateurs dont le processus tourne (ordre de priorité du registre).
    pub running: Vec<SimId>,
    /// Pour chaque sim avec source live : Some(true) = en piste, Some(false) = connecté hors piste.
    pub live: Vec<(SimId, bool)>,
    pub importing: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionMonitor {
    pub state: MonitorState,
}

impl Default for SessionMonitor {
    fn default() -> Self {
        Self { state: MonitorState::NoSimulator }
    }
}

impl SessionMonitor {
    /// Calcule le nouvel état. Plusieurs jeux ouverts : la session active prime,
    /// sinon le premier jeu détecté.
    pub fn step(&mut self, o: &Observation) -> &MonitorState {
        let prev_active = match &self.state {
            MonitorState::SessionActive { sim } => Some(*sim),
            _ => None,
        };
        let next = if let Some(d) = &o.importing {
            MonitorState::Importing { detail: d.clone() }
        } else if let Some((sim, _)) = o.live.iter().find(|(s, on)| *on && o.running.contains(s)) {
            MonitorState::SessionActive { sim: *sim }
        } else if let Some(sim) = prev_active {
            // Était en piste et ne l'est plus (sortie de piste, menu ou jeu fermé).
            MonitorState::SessionEnded { sim }
        } else if let Some((sim, _)) = o.live.iter().find(|(s, _)| o.running.contains(s)) {
            match &self.state {
                MonitorState::SessionEnded { sim: s } if s == sim => MonitorState::SessionEnded { sim: *sim },
                _ => MonitorState::WaitingForSession { sim: *sim },
            }
        } else if let Some(sim) = o.running.first() {
            match &self.state {
                MonitorState::SessionEnded { sim: s } if s == sim => MonitorState::SessionEnded { sim: *sim },
                _ => MonitorState::GameDetected { sim: *sim },
            }
        } else {
            MonitorState::NoSimulator
        };
        self.state = next;
        &self.state
    }
}

/// Scanner de processus (réutilise l'objet `System` : coût CPU minimal).
pub struct ProcessScanner {
    sys: System,
}

impl Default for ProcessScanner {
    fn default() -> Self {
        Self { sys: System::new_with_specifics(RefreshKind::nothing()) }
    }
}

impl ProcessScanner {
    pub fn running(&mut self, connectors: &[Box<dyn SimulatorConnector>]) -> Vec<SimId> {
        self.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
        let names: Vec<String> = self.sys.processes().values().map(|p| p.name().to_string_lossy().to_ascii_lowercase()).collect();
        connectors
            .iter()
            .filter(|c| c.process_names().iter().any(|n| names.iter().any(|p| p == &n.to_ascii_lowercase())))
            .map(|c| c.id())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(running: &[SimId], live: &[(SimId, bool)]) -> Observation {
        Observation { running: running.to_vec(), live: live.to_vec(), importing: None }
    }

    #[test]
    fn full_lifecycle() {
        let mut m = SessionMonitor::default();
        assert_eq!(m.step(&obs(&[], &[])), &MonitorState::NoSimulator);
        assert_eq!(m.step(&obs(&[SimId::Iracing], &[])), &MonitorState::GameDetected { sim: SimId::Iracing });
        assert_eq!(m.step(&obs(&[SimId::Iracing], &[(SimId::Iracing, false)])), &MonitorState::WaitingForSession { sim: SimId::Iracing });
        assert_eq!(m.step(&obs(&[SimId::Iracing], &[(SimId::Iracing, true)])), &MonitorState::SessionActive { sim: SimId::Iracing });
        assert_eq!(m.step(&obs(&[SimId::Iracing], &[(SimId::Iracing, false)])), &MonitorState::SessionEnded { sim: SimId::Iracing });
        // Reste « terminée » tant qu'on ne repart pas en piste.
        assert_eq!(m.step(&obs(&[SimId::Iracing], &[(SimId::Iracing, false)])), &MonitorState::SessionEnded { sim: SimId::Iracing });
        assert_eq!(m.step(&obs(&[SimId::Iracing], &[(SimId::Iracing, true)])), &MonitorState::SessionActive { sim: SimId::Iracing });
        assert_eq!(m.step(&obs(&[], &[])), &MonitorState::SessionEnded { sim: SimId::Iracing });
        assert_eq!(m.step(&obs(&[], &[])), &MonitorState::NoSimulator);
    }

    #[test]
    fn active_session_wins_when_several_games_run() {
        let mut m = SessionMonitor::default();
        let o = obs(&[SimId::Lmu, SimId::Iracing], &[(SimId::Iracing, true)]);
        assert_eq!(m.step(&o), &MonitorState::SessionActive { sim: SimId::Iracing });
    }

    #[test]
    fn importing_overrides() {
        let mut m = SessionMonitor::default();
        let o = Observation { importing: Some("x.ibt".into()), ..Default::default() };
        assert!(matches!(m.step(&o), MonitorState::Importing { .. }));
    }
}
