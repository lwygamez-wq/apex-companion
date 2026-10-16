//! Thread de surveillance : processus des simulateurs, source live, import auto.
//! Cadence 2 s (≈0 % CPU) : la détection de processus n'a pas besoin de plus.

use apex_core::connectors::{LiveSnapshot, LiveSource};
use apex_core::detect::{MonitorState, Observation, ProcessScanner, SessionMonitor};
use apex_core::model::SimId;
use apex_core::service::AppService;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
pub struct StatusPayload {
    pub state: MonitorState,
    pub label: String,
    pub running: Vec<SimId>,
    pub live: Option<LiveSnapshot>,
    pub importing: Option<String>,
    pub last_auto_import: Option<String>,
}

impl Default for StatusPayload {
    fn default() -> Self {
        Self {
            label: MonitorState::NoSimulator.label_fr().into(),
            state: MonitorState::NoSimulator,
            running: vec![],
            live: None,
            importing: None,
            last_auto_import: None,
        }
    }
}

pub fn set_importing(status: &Arc<Mutex<StatusPayload>>, v: Option<String>) {
    if let Ok(mut s) = status.lock() {
        s.importing = v;
    }
}

pub fn spawn(app: AppHandle, svc: Arc<AppService>, status: Arc<Mutex<StatusPayload>>) {
    let r = std::thread::Builder::new().name("apex-monitor".into()).spawn(move || {
        let mut scanner = ProcessScanner::default();
        let mut monitor = SessionMonitor::default();
        let mut live: Option<(SimId, Box<dyn LiveSource>)> = None;
        let mut last_snap: Option<LiveSnapshot> = None;
        // Synchro du dossier : immédiate au démarrage (rattrapage), puis toutes les 15 s.
        let mut next_sync = Instant::now();
        loop {
            let settings = svc.settings();
            let running = scanner.running(svc.connectors());

            // Source live : ouverte à la demande, fermée si le jeu disparaît.
            if live.as_ref().is_some_and(|(sim, _)| !running.contains(sim) || !settings.live_enabled) {
                live = None;
                last_snap = None;
            }
            if live.is_none() && settings.live_enabled {
                for sim in &running {
                    if let Some(c) = svc.connectors().iter().find(|c| c.id() == *sim) {
                        if let Ok(src) = c.open_live(&svc.connector_config(*sim)) {
                            log::info!("source live ouverte : {}", sim.display_name());
                            live = Some((*sim, src));
                            break;
                        }
                    }
                }
            }
            let mut live_obs = vec![];
            let polled = live.as_mut().map(|(sim, src)| (*sim, src.poll(50)));
            match polled {
                Some((sim, Ok(snap))) => {
                    if snap.is_some() {
                        last_snap = snap;
                    }
                    if let Some(s) = &last_snap {
                        live_obs.push((sim, s.connected && s.on_track));
                    }
                }
                Some((_, Err(e))) => {
                    log::warn!("source live perdue : {e}");
                    live = None;
                    last_snap = None;
                }
                None => {}
            }

            let importing = status.lock().ok().and_then(|s| s.importing.clone());

            let state = monitor.step(&Observation { running: running.clone(), live: live_obs, importing }).clone();

            // Synchro automatique du dossier de télémétrie iRacing, indépendante de la lecture live :
            // un .ibt est importé dès qu'il n'a plus bougé depuis 20 s (iRacing a fini de l'écrire).
            let mut auto_msg = None;
            if settings.auto_import && Instant::now() >= next_sync {
                next_sync = Instant::now() + Duration::from_secs(15);
                match svc.sync_iracing_folder(Duration::from_secs(20)) {
                    Ok(r) if r.imported > 0 || !r.errors.is_empty() => {
                        let mut m = format!("{} session(s) importée(s) automatiquement", r.imported);
                        if !r.errors.is_empty() {
                            m += &format!(", {} fichier(s) illisible(s) : {}", r.errors.len(), r.errors.join(" ; "));
                            log::warn!("synchro : {}", r.errors.join(" ; "));
                        }
                        auto_msg = Some(m);
                        let _ = app.emit("apex://sessions-changed", ());
                    }
                    Ok(_) => {}
                    // Dossier absent (iRacing pas installé ou chemin non réglé) : on réessaie plus tard, sans bruit.
                    Err(e) => {
                        log::debug!("synchro : {e}");
                        next_sync = Instant::now() + Duration::from_secs(60);
                    }
                }
            }

            let payload = {
                let mut s = status.lock().unwrap_or_else(|p| p.into_inner());
                s.label = state.label_fr().into();
                s.state = state;
                s.running = running;
                s.live = last_snap.clone();
                if auto_msg.is_some() {
                    s.last_auto_import = auto_msg;
                }
                s.clone()
            };
            let _ = app.emit("apex://status", payload);
            std::thread::sleep(Duration::from_millis(if live.is_some() { 1000 } else { 2000 }));
        }
    });
    if let Err(e) = r {
        log::error!("impossible de lancer le thread de surveillance : {e}");
    }
}
