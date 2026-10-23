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
        // Sessions décodées par une ancienne version : recalculées depuis leurs fichiers .ibt.
        match svc.reprocess_outdated() {
            Ok(n) if n > 0 => {
                log::info!("{n} session(s) recalculée(s) avec le nouveau décodeur");
                let _ = app.emit("apex://sessions-changed", ());
            }
            Ok(_) => {}
            Err(e) => log::warn!("recalcul des sessions : {e}"),
        }
        // Recherche Garage 61 pour toutes les sessions déjà roulées : au plus une fois par jour.
        let mut next_g61_check = Instant::now() + Duration::from_secs(20);
        loop {
            let settings = svc.settings();
            if Instant::now() >= next_g61_check {
                next_g61_check = Instant::now() + Duration::from_secs(3600);
                // Garage 61, dans un seul fil pour ne pas dépasser la limite de débit de l'API gratuite :
                // 1) tes sessions envoyées sur Garage 61 mais absentes d'Apex (fichier .ibt effacé avant import),
                //    récupérées au plus une fois par jour et dès le premier lancement ;
                // 2) recherche de références pour toutes les sessions déjà roulées, au plus une fois par jour.
                let enabled = settings.garage61_auto && settings.garage61_token.as_deref().is_some_and(|t| !t.trim().is_empty());
                let (recover, refs) = (enabled && svc.garage61_recover_due(24), enabled && svc.garage61_full_scan_due(24));
                if recover || refs {
                    let (svc2, app2, status2) = (svc.clone(), app.clone(), status.clone());
                    std::thread::spawn(move || {
                        let say = |msg: String| {
                            log::info!("{msg}");
                            if let Ok(mut s) = status2.lock() {
                                s.last_auto_import = Some(msg);
                            }
                            let _ = app2.emit("apex://sessions-changed", ());
                        };
                        if recover {
                            say(match svc2.recover_garage61_sessions(30) {
                                Ok(r) if r.sessions > 0 => format!("Garage 61 : {} session(s) récupérée(s) ({} tours)", r.sessions, r.laps),
                                Ok(r) => format!("Garage 61 : aucune session à récupérer ({})", r.messages.join(" ; ")),
                                Err(e) => format!("Garage 61, récupération : {e}"),
                            });
                        }
                        if refs {
                            say(match svc2.find_garage61_references_all() {
                                Ok(v) => format!("Garage 61 : {}", v.join(" ; ")),
                                Err(e) => format!("Garage 61 : {e}"),
                            });
                        }
                    });
                }
            }
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
            // Copie de garde des .ibt dès leur apparition : un autre logiciel (agent Garage 61 réglé pour
            // supprimer après envoi) peut les effacer quelques secondes après la fin de la session.
            if settings.auto_import {
                match svc.guard_iracing_files() {
                    Ok(n) if n > 0 => log::info!("{n} fichier(s) de télémétrie mis sous garde"),
                    Ok(_) => {}
                    Err(e) => log::debug!("garde : {e}"),
                }
            }
            if settings.auto_import && Instant::now() >= next_sync {
                next_sync = Instant::now() + Duration::from_secs(15);
                // Tours Garage 61 téléchargés depuis le site : importés comme références.
                if settings.watch_downloads {
                    match svc.sync_downloads() {
                        Ok(v) if !v.is_empty() => {
                            let msgs: Vec<String> = v
                                .iter()
                                .map(|r| match &r.result {
                                    Ok(x) => format!("référence {} ({}) importée", x.driver, apex_core::service::fmt_lap(x.lap_time_s)),
                                    Err(e) => format!("{} : {e}", r.file),
                                })
                                .collect();
                            auto_msg = Some(msgs.join(" ; "));
                            let _ = app.emit("apex://sessions-changed", ());
                        }
                        Ok(_) => {}
                        Err(e) => log::debug!("téléchargements : {e}"),
                    }
                }
                match svc.sync_iracing_folder(Duration::from_secs(20)) {
                    Ok(r) if r.imported > 0 || !r.errors.is_empty() => {
                        // Nouvelle session : recherche d'une référence plus rapide sur Garage 61, en arrière-plan.
                        if settings.garage61_auto && settings.garage61_token.as_deref().is_some_and(|t| !t.trim().is_empty()) {
                            let (svc2, app2, status2, ids) = (svc.clone(), app.clone(), status.clone(), r.new_sessions.clone());
                            std::thread::spawn(move || {
                                for id in ids {
                                    let msg = match svc2.find_garage61_reference(id) {
                                        Ok(m) => m,
                                        Err(e) => format!("Garage 61 : {e}"),
                                    };
                                    log::info!("{msg}");
                                    if let Ok(mut s) = status2.lock() {
                                        s.last_auto_import = Some(msg);
                                    }
                                    let _ = app2.emit("apex://sessions-changed", ());
                                }
                            });
                        }
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
