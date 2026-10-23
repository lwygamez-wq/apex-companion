//! Thread de surveillance : processus des simulateurs, source live, import auto.
//! Cadence 2 s (≈0 % CPU) : la détection de processus n'a pas besoin de plus.

use apex_core::connectors::{LiveSnapshot, LiveSource};
use apex_core::detect::{MonitorState, Observation, ProcessScanner, SessionMonitor};
use apex_core::live_coach::{session_meta_from_yaml, CoachMessage, LiveCoach};
use apex_core::model::SessionKind;
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
    /// Derniers messages radio du coach en direct (le plus récent en dernier).
    pub coach: Vec<CoachMessage>,
}

/// Message radio envoyé à l'interface, qui le prononce si `speak`.
#[derive(Debug, Clone, Serialize)]
pub struct CoachEvent {
    pub message: CoachMessage,
    pub speak: bool,
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
            coach: vec![],
        }
    }
}

/// Diffuse un message du coach (journal, état, voix).
fn radio(app: &AppHandle, status: &Arc<Mutex<StatusPayload>>, message: CoachMessage, speak: bool) {
    log::info!("coach : {}", message.text);
    if let Ok(mut s) = status.lock() {
        s.coach.push(message.clone());
        let n = s.coach.len();
        if n > 12 {
            s.coach.drain(..n - 12);
        }
    }
    let _ = app.emit("apex://coach", CoachEvent { message, speak });
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
        // Coach en direct : (circuit|configuration|voiture, coach).
        let mut coach: Option<(String, LiveCoach)> = None;
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
                coach = None;
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
            // Lecture continue (~60 Hz) pendant ~0,9 s : le coach a besoin de chaque échantillon pour
            // reconstituer le tour ; le reste de la boucle (processus, synchro) garde sa cadence.
            let polled = live.as_mut().map(|(sim, src)| {
                let until = Instant::now() + Duration::from_millis(900);
                let mut latest: Result<Option<LiveSnapshot>, apex_core::connectors::ConnectorError> = Ok(None);
                while Instant::now() < until {
                    match src.poll(20) {
                        Ok(Some(snap)) => {
                            if let Some(yaml) = &snap.session_info {
                                let meta = session_meta_from_yaml(yaml);
                                let key = format!("{}|{}|{}", meta.track, meta.track_config.clone().unwrap_or_default(), meta.car);
                                if coach.as_ref().is_none_or(|(k, _)| *k != key) && meta.track != "Circuit inconnu" {
                                    let reference = svc.live_reference(&meta).unwrap_or_else(|e| {
                                        log::warn!("référence du coach : {e}");
                                        None
                                    });
                                    let (c, hello) = LiveCoach::new(yaml, reference, svc.user_corner_names(&meta));
                                    coach = Some((key, c));
                                    radio(&app, &status, hello, settings.coach_audio);
                                }
                            }
                            if let (Some(row), Some((_, c))) = (snap.channels, coach.as_mut()) {
                                if let Some(m) = c.push(row) {
                                    let mute = c.current_kind() == SessionKind::Race && !settings.coach_in_race;
                                    radio(&app, &status, m, settings.coach_audio && !mute);
                                }
                            }
                            latest = Ok(Some(snap));
                        }
                        // Pas de donnée neuve : courte pause (évite une boucle active si l'événement iRacing est absent).
                        Ok(None) => std::thread::sleep(Duration::from_millis(4)),
                        Err(e) => {
                            latest = Err(e);
                            break;
                        }
                    }
                }
                (*sim, latest)
            });
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
                    coach = None;
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
            // Avec une source live, la lecture continue ci-dessus occupe déjà ~0,9 s par tour de boucle.
            std::thread::sleep(Duration::from_millis(if live.is_some() { 20 } else { 2000 }));
        }
    });
    if let Err(e) = r {
        log::error!("impossible de lancer le thread de surveillance : {e}");
    }
}
