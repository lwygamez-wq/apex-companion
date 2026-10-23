//! Bouton radio du volant : tant qu'il est maintenu, le micro enregistre ; au relâchement, le
//! mot-clé est reconnu (hors ligne) et le coach répond : « répète », « silence », « reprends »,
//! « point », « plan ».

use crate::monitor::{radio, StatusPayload};
use apex_core::live_coach::{CoachMessage, CoachMessageKind};
use apex_core::mic::{Mic, SAMPLE_RATE};
use apex_core::ptt::binding_down;
use apex_core::service::AppService;
use apex_core::voice_cmd::{parse_command, VoiceCommand};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// État affiché dans Live Coach pendant une commande vocale.
#[derive(Debug, Clone, Serialize)]
pub struct PttEvent {
    /// « listening », « done », « error »
    pub state: &'static str,
    pub heard: Option<String>,
    pub command: Option<VoiceCommand>,
    pub error: Option<String>,
}

fn reply(app: &AppHandle, status: &Arc<Mutex<StatusPayload>>, text: String, speak: bool) {
    let m = CoachMessage { kind: CoachMessageKind::Reply, lap: 0, text, focus: None, focus_loss_s: None, lap_delta_s: None, lap_time_s: None };
    radio(app, status, m, speak);
}

pub fn spawn(app: AppHandle, svc: Arc<AppService>, status: Arc<Mutex<StatusPayload>>) {
    let r = std::thread::Builder::new().name("apex-ptt".into()).spawn(move || loop {
        let settings = svc.settings();
        let Some(binding) = settings.ptt_binding.clone().filter(|_| settings.voice_commands) else {
            std::thread::sleep(Duration::from_millis(500));
            continue;
        };
        if !binding_down(&binding) {
            std::thread::sleep(Duration::from_millis(20));
            continue;
        }
        // Bouton enfoncé : on coupe la voix du coach (le micro ne doit pas l'entendre) et on écoute.
        let _ = app.emit("apex://coach-stop", ());
        let _ = app.emit("apex://ptt", PttEvent { state: "listening", heard: None, command: None, error: None });
        let mut mic = match Mic::start() {
            Ok(m) => m,
            Err(e) => {
                let _ = app.emit("apex://ptt", PttEvent { state: "error", heard: None, command: None, error: Some(e.clone()) });
                reply(&app, &status, "Micro indisponible.".into(), settings.coach_audio);
                while binding_down(&binding) {
                    std::thread::sleep(Duration::from_millis(30));
                }
                continue;
            }
        };
        let t0 = Instant::now();
        while binding_down(&binding) && t0.elapsed() < Duration::from_secs(6) {
            mic.poll();
            std::thread::sleep(Duration::from_millis(30));
        }
        // Le mot-clé finit souvent au moment du relâchement : on garde 250 ms de plus.
        let tail = Instant::now();
        while tail.elapsed() < Duration::from_millis(250) {
            mic.poll();
            std::thread::sleep(Duration::from_millis(25));
        }
        let pcm = mic.stop();
        if pcm.len() < SAMPLE_RATE as usize / 5 {
            continue;
        }
        let heard = svc.voice_recognize(&pcm);
        let (text, cmd) = match &heard {
            Ok(t) => (Some(t.clone()), parse_command(t)),
            Err(_) => (None, None),
        };
        let answer = match (cmd, &heard) {
            (_, Err(e)) => format!("Commande vocale indisponible : {e}"),
            (Some(VoiceCommand::Repeat), _) => status.lock().ok().and_then(|s| s.coach_repeat.clone()).unwrap_or_else(|| "Pas encore de conseil.".into()),
            (Some(VoiceCommand::Status), _) => status.lock().ok().and_then(|s| s.coach_point.clone()).unwrap_or_else(|| "Pas encore de tour analysé.".into()),
            (Some(VoiceCommand::Mute), _) => {
                if let Ok(mut s) = status.lock() {
                    s.muted = true;
                }
                "Radio coupée. Dis reprends pour la remettre.".into()
            }
            (Some(VoiceCommand::Resume), _) => {
                if let Ok(mut s) = status.lock() {
                    s.muted = false;
                }
                "Radio reprise.".into()
            }
            (Some(VoiceCommand::Plan), _) => status.lock().ok().and_then(|s| s.coach_plan.clone()).unwrap_or_else(|| "Pas de plan pour cette séance.".into()),
            (None, Ok(_)) => "Pas compris. Répète, silence, reprends, point ou plan.".into(),
        };
        let _ = app.emit("apex://ptt", PttEvent { state: "done", heard: text, command: cmd, error: heard.as_ref().err().map(|e| e.to_string()) });
        // Une réponse demandée est toujours prononcée, même radio coupée.
        reply(&app, &status, answer, settings.coach_audio);
        while binding_down(&binding) {
            std::thread::sleep(Duration::from_millis(30));
        }
    });
    if let Err(e) = r {
        log::error!("bouton radio : démarrage impossible : {e}");
    }
}
