//! Coach vocal en direct : comme un ingénieur de course à la radio.
//!
//! Chaque tour terminé est découpé par le même code que les fichiers .ibt, comparé à la
//! référence (même moteur que l'analyse d'après-session), puis les 3 derniers tours propres sont
//! synthétisés : le virage où tu perds le plus, de façon répétée, devient le « focus » annoncé au
//! début du tour suivant, avec une consigne simple (freiner, tourner ou accélérer plus tôt / tard).
//!
//! Volontairement sobre : un seul message par tour, au passage de la ligne, jamais pendant un
//! freinage. Aucune consigne n'est inventée : sans écart mesurable, le coach le dit.

use crate::analysis::compare::{compare_laps_named, Comparison, CornerName, LapInput};
use crate::analysis::resample::Resampled;
use crate::ingest::ibt::{LiveLapBuilder, SessionInfo};
use crate::model::*;
use serde::Serialize;
use std::collections::VecDeque;

/// Tour de référence du coach.
#[derive(Debug, Clone)]
pub struct ReferenceLap {
    /// « Luke Fleming (référence externe) », « ton meilleur tour »…
    pub label: String,
    pub meta: LapMeta,
    pub trace: LapTrace,
    /// Noms des virages du circuit (distance de l'apex, nom).
    pub names: Vec<CornerName>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoachMessageKind {
    /// Mise en route, changement de référence.
    Info,
    /// Synthèse de fin de tour avec un virage à travailler.
    Focus,
    /// Tour non compté (sortie, incident, stand).
    Invalid,
    /// Rappel court juste avant le virage du focus.
    Callout,
    /// Réponse à une commande vocale.
    Reply,
}

/// Message radio, prononcé au début du tour suivant.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CoachMessage {
    pub kind: CoachMessageKind,
    /// Tour qui vient de se terminer (0 pour un message de mise en route).
    pub lap: i32,
    pub text: String,
    pub focus: Option<String>,
    /// Temps perdu en moyenne au virage focus sur les derniers tours (s).
    pub focus_loss_s: Option<f32>,
    /// Écart du tour à la référence (s, positif = plus lent).
    pub lap_delta_s: Option<f64>,
    pub lap_time_s: Option<f64>,
}

/// Écarts mesurés à un virage sur un tour (convention : positif = défavorable au tour).
#[derive(Debug, Clone)]
struct CornerStat {
    id: u32,
    name: String,
    apex_m: f32,
    loss_s: f32,
    /// > 0 : freine plus tôt que la référence (m).
    brake_early_m: Option<f32>,
    /// La référence ne freine pas, ce tour freine.
    needless_brake: bool,
    /// > 0 : commence à tourner plus tard que la référence (m).
    turn_in_late_m: Option<f32>,
    /// > 0 : plus lent au point de corde (km/h).
    vmin_deficit_kmh: f32,
    /// > 0 : remet les gaz plus tard (m).
    throttle_late_m: Option<f32>,
    exit_deficit_kmh: f32,
    /// Distance où annoncer le virage : ~3,5 s avant son entrée, à la vitesse de la référence (m).
    trigger_m: f32,
}

#[derive(Debug, Clone)]
struct LapAnalysis {
    corners: Vec<CornerStat>,
}

/// Consigne retenue pour un virage, à partir des écarts moyens.
#[derive(Debug, Clone, PartialEq)]
enum Cue {
    BrakeLater(f32),
    BrakeEarlier,
    NoBrake,
    TurnEarlier,
    TurnLater,
    CarrySpeed(f32),
    ThrottleEarlier,
    BetterExit,
    Line,
}

impl Cue {
    /// Version très courte, pour l'annonce juste avant le virage.
    fn short(&self) -> &'static str {
        match self {
            Cue::BrakeLater(_) => "freine plus tard",
            Cue::BrakeEarlier => "freine plus tôt",
            Cue::NoBrake => "sans freiner",
            Cue::TurnEarlier => "tourne plus tôt",
            Cue::TurnLater => "tourne plus tard",
            Cue::CarrySpeed(_) => "relâche le frein plus tôt",
            Cue::ThrottleEarlier => "gaz plus tôt",
            Cue::BetterExit => "soigne la sortie",
            Cue::Line => "vise la corde",
        }
    }

    fn say(&self) -> String {
        match self {
            Cue::BrakeLater(m) => format!("Freine {} mètres plus tard.", round_to(*m, 5.0)),
            Cue::BrakeEarlier => "Freine un peu plus tôt, tu sors trop lentement.".into(),
            Cue::NoBrake => "Ne freine pas, lève juste le pied.".into(),
            Cue::TurnEarlier => "Commence à tourner plus tôt.".into(),
            Cue::TurnLater => "Tourne plus tard, attends la corde.".into(),
            Cue::CarrySpeed(k) => format!("Relâche le frein plus tôt, garde {} kilomètres heure de plus à la corde.", k.round() as i32),
            Cue::ThrottleEarlier => "Accélère plus tôt.".into(),
            Cue::BetterExit => "Ouvre la sortie et accélère plus franchement.".into(),
            Cue::Line => "Vise la corde de la référence.".into(),
        }
    }
}

fn round_to(x: f32, step: f32) -> i32 {
    (((x / step).round() * step) as i32).max(step as i32)
}

/// « 3 dixièmes », « 1 seconde 2 », « moins d'un dixième ».
pub fn say_time(s: f64) -> String {
    let a = s.abs();
    if a < 0.05 {
        "moins d'un dixième".into()
    } else if a < 0.95 {
        let d = (a * 10.0).round() as i32;
        format!("{d} dixième{}", if d > 1 { "s" } else { "" })
    } else {
        let whole = a.floor() as i32;
        let tenth = ((a - a.floor()) * 10.0).round() as i32;
        let (whole, tenth) = if tenth == 10 { (whole + 1, 0) } else { (whole, tenth) };
        format!("{whole} seconde{}{}", if whole > 1 { "s" } else { "" }, if tenth > 0 { format!(" {tenth}") } else { String::new() })
    }
}

/// Nom prononçable : « V7 » → « virage 7 ».
fn spoken(name: &str) -> String {
    match name.strip_prefix('V').filter(|r| r.chars().all(|c| c.is_ascii_digit()) && !r.is_empty()) {
        Some(n) => format!("virage {n}"),
        None => name.to_string(),
    }
}

/// Distance (m) où le pilote commence à tourner dans la fenêtre du virage : volant au-delà de 35 % de son maximum.
fn turn_in_m(r: &Resampled, from_m: f32, to_m: f32) -> Option<f32> {
    let steer = r.steer_deg.as_ref()?;
    let (a, b) = (r.idx(from_m), r.idx(to_m));
    if b <= a + 2 {
        return None;
    }
    let peak = steer[a..=b].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    if peak < 5.0 {
        return None;
    }
    (a..=b).find(|&i| steer[i].abs() >= 0.35 * peak).map(|i| r.dist[i])
}

fn analyse(c: &Comparison) -> LapAnalysis {
    let corners = c
        .corners
        .iter()
        .map(|k| {
            let (rf, cm) = (&k.reference, &k.compared);
            let ti = match (turn_in_m(&c.ref_trace, k.corner.search_start_m, k.corner.apex_m), turn_in_m(&c.cmp_trace, k.corner.search_start_m, k.corner.apex_m)) {
                (Some(a), Some(b)) => Some(b - a),
                _ => None,
            };
            // Point de freinage comparable seulement si les deux tours freinent vraiment : un simple
            // effleurement de la pédale déplace le « début du freinage » de 100 m et fausserait la consigne.
            let (rb, cb) = (rf.brake_peak >= 0.2, cm.brake_peak >= 0.2);
            CornerStat {
                id: k.corner.id,
                name: k.corner.name.clone(),
                apex_m: k.corner.apex_m,
                loss_s: k.time_delta_s,
                brake_early_m: match (rf.brake_on_m, cm.brake_on_m) {
                    (Some(a), Some(b)) if rb && cb && (a - b).abs() <= 60.0 => Some(a - b),
                    _ => None,
                },
                needless_brake: !rb && cb,
                turn_in_late_m: ti,
                vmin_deficit_kmh: rf.min_speed_kmh - cm.min_speed_kmh,
                throttle_late_m: match (rf.throttle_on_m, cm.throttle_on_m) {
                    (Some(a), Some(b)) => Some(b - a),
                    _ => None,
                },
                exit_deficit_kmh: rf.exit_speed_kmh - cm.exit_speed_kmh,
                trigger_m: {
                    let v = c.ref_trace.speed_kmh.get(c.ref_trace.idx(k.corner.entry_m)).copied().unwrap_or(150.0);
                    k.corner.entry_m - v / 3.6 * 3.5
                },
            }
        })
        .collect();
    LapAnalysis { corners }
}

/// Consigne pour un virage à partir des tours où il coûte du temps (moyennes).
fn cue_for(stats: &[&CornerStat]) -> Cue {
    let mean = |f: &dyn Fn(&CornerStat) -> Option<f32>| {
        let v: Vec<f32> = stats.iter().filter_map(|s| f(s)).collect();
        (!v.is_empty()).then(|| v.iter().sum::<f32>() / v.len() as f32)
    };
    let brake = mean(&|s| s.brake_early_m);
    let vmin = mean(&|s| Some(s.vmin_deficit_kmh)).unwrap_or(0.0);
    let exit = mean(&|s| Some(s.exit_deficit_kmh)).unwrap_or(0.0);
    let turn = mean(&|s| s.turn_in_late_m);
    let throttle = mean(&|s| s.throttle_late_m);
    let needless = stats.iter().filter(|s| s.needless_brake).count() * 2 > stats.len();
    if let Some(b) = brake.filter(|b| *b >= 8.0) {
        Cue::BrakeLater(b * 0.7)
    } else if needless && vmin >= 3.0 {
        Cue::NoBrake
    } else if brake.is_some_and(|b| b <= -8.0) && (vmin >= 3.0 || exit >= 3.0) {
        Cue::BrakeEarlier
    } else if let Some(t) = turn.filter(|t| t.abs() >= 8.0) {
        if t > 0.0 {
            Cue::TurnEarlier
        } else {
            Cue::TurnLater
        }
    } else if vmin >= 3.0 {
        Cue::CarrySpeed(vmin)
    } else if throttle.is_some_and(|t| t >= 10.0) {
        Cue::ThrottleEarlier
    } else if exit >= 3.0 {
        Cue::BetterExit
    } else {
        Cue::Line
    }
}

/// Coach d'une session en direct (un circuit, une voiture).
pub struct LiveCoach {
    builder: LiveLapBuilder,
    meta: SessionMeta,
    reference: Option<ReferenceLap>,
    /// La référence est ton meilleur tour de cette session (aucune référence enregistrée).
    self_reference: bool,
    history: VecDeque<LapAnalysis>,
    last_focus: Option<(u32, String, f32)>,
    last_session_num: f64,
    /// Noms donnés par le pilote à ce circuit (prioritaires sur les tables intégrées).
    user_names: Vec<CornerName>,
    /// Sorties de piste de la séance par virage.
    offs: std::collections::HashMap<String, u32>,
    /// Annonce avant le virage du focus : (distance de déclenchement, texte), et si elle a déjà été faite ce tour-ci.
    callout: Option<(f32, String)>,
    callout_done: bool,
    last_dist: f64,
    /// Meilleur tour propre de la séance, écart du dernier tour, dernier conseil (pour « répète » / « point »).
    best_lap: Option<f64>,
    last_delta: Option<f64>,
    last_advice: Option<String>,
    last_cue: Option<String>,
}

/// Tours propres pris en compte dans la synthèse.
const WINDOW: usize = 3;
/// Perte minimale pour qu'un virage compte comme « perdu » sur un tour (s).
const LOSS_MIN_S: f32 = 0.03;
/// Perte moyenne minimale pour devenir le focus (s).
const FOCUS_MIN_S: f32 = 0.05;

fn meta_from(si: &SessionInfo) -> SessionMeta {
    SessionMeta {
        sim: SimId::Iracing,
        track: si.track_display_name.clone().unwrap_or_else(|| "Circuit inconnu".into()),
        track_config: si.track_config.clone(),
        track_length_m: si.track_length_m,
        car: si.car_screen_name.clone().unwrap_or_else(|| "Voiture inconnue".into()),
        kind: SessionKind::Unknown,
        started_at_unix: 0,
        weather: Weather::default(),
        source_fingerprint: "live".into(),
        source_path: None,
    }
}

impl LiveCoach {
    /// `reference` : tour de référence du même circuit et de la même voiture (perso ou externe),
    /// `None` si aucun : le coach prendra ton meilleur tour propre de la session.
    pub fn new(session_yaml: &str, reference: Option<ReferenceLap>, user_names: Vec<CornerName>) -> (Self, CoachMessage) {
        let builder = LiveLapBuilder::new(session_yaml);
        let meta = meta_from(builder.session_info());
        let text = match &reference {
            Some(r) => format!("Coach prêt. Référence : {}.", r.label),
            None => "Coach prêt. Pas de référence enregistrée ici : je prends ton meilleur tour propre de la session.".into(),
        };
        let self_reference = reference.is_none();
        (
            Self { builder, meta, reference, self_reference, history: VecDeque::new(), last_focus: None, last_session_num: -1.0, user_names, offs: Default::default(), callout: None, callout_done: false, last_dist: 0.0, best_lap: None, last_delta: None, last_advice: None, last_cue: None },
            CoachMessage { kind: CoachMessageKind::Info, lap: 0, text, focus: None, focus_loss_s: None, lap_delta_s: None, lap_time_s: None },
        )
    }

    pub fn session(&self) -> &SessionMeta {
        &self.meta
    }

    /// Type de la séance en cours (essais, qualif, course), d'après le numéro de session.
    pub fn current_kind(&self) -> SessionKind {
        let si = self.builder.session_info();
        si.session_types.iter().find(|(n, _)| *n as f64 == self.last_session_num).map(|(_, l)| SessionKind::from_label(l)).unwrap_or(SessionKind::Unknown)
    }

    /// Un échantillon de télémétrie (ordre de `LIVE_CHANNELS`). Renvoie le message radio quand un tour se termine.
    pub fn push(&mut self, row: [f64; 19]) -> Option<CoachMessage> {
        if row[17] != self.last_session_num {
            // Nouvelle séance (essais → qualif → course) : on repart d'une synthèse vierge.
            self.last_session_num = row[17];
            self.history.clear();
            self.last_focus = None;
            self.offs.clear();
            self.callout = None;
            self.best_lap = None;
        }
        // Annonce du virage du focus, une fois par tour, quand la voiture passe le point de déclenchement.
        let dist = row[1];
        let mut callout = None;
        if let Some((trig, text)) = &self.callout {
            let t = *trig as f64;
            if !self.callout_done && self.last_dist < t && dist >= t && dist - self.last_dist < 100.0 {
                self.callout_done = true;
                callout = Some(CoachMessage { kind: CoachMessageKind::Callout, lap: row[6] as i32, text: text.clone(), focus: None, focus_loss_s: None, lap_delta_s: None, lap_time_s: None });
            }
        }
        self.last_dist = dist;
        match self.builder.push(row) {
            Some(lap) => {
                self.callout_done = false;
                Some(self.on_lap(lap))
            }
            None => callout,
        }
    }

    /// Dernier conseil (commande « répète »).
    pub fn repeat_text(&self) -> String {
        self.last_advice.clone().unwrap_or_else(|| "Pas encore de conseil : il faut au moins un tour propre.".into())
    }

    /// Point de situation (commande « point ») : dernier tour, meilleur tour, focus.
    pub fn status_text(&self) -> String {
        let mut parts = vec![];
        match self.last_delta {
            Some(d) if d < -0.05 => parts.push(format!("Dernier tour : moins {} sur la référence.", say_time(d))),
            Some(d) if d.abs() <= 0.05 => parts.push("Dernier tour : à égalité avec la référence.".into()),
            Some(d) => parts.push(format!("Dernier tour : plus {}.", say_time(d))),
            None => {}
        }
        if let Some(b) = self.best_lap {
            parts.push(format!("Meilleur tour, {}.", say_lap(Some(b))));
        }
        match (&self.last_focus, &self.callout) {
            (Some((_, name, loss)), Some((_, short))) => parts.push(format!("Focus {name}, {} à prendre : {}", say_time(*loss as f64), short.split_once(", ").map(|x| x.1).unwrap_or(short))),
            (Some((_, name, _)), None) => parts.push(format!("Focus {name}.")),
            _ => parts.push("Pas de focus pour l'instant.".into()),
        }
        parts.join(" ")
    }

    fn on_lap(&mut self, lap: ImportedLap) -> CoachMessage {
        let n = lap.meta.number;
        let t = lap.meta.lap_time_s;
        let msg = |kind, text: String, focus: Option<String>, loss: Option<f32>, delta: Option<f64>| CoachMessage { kind, lap: n, text, focus, focus_loss_s: loss, lap_delta_s: delta, lap_time_s: t };
        if lap.meta.kind != LapKind::Flying {
            return msg(CoachMessageKind::Invalid, "Tour de stand, non compté.".into(), None, None, None);
        }
        // Pas encore de référence : le premier tour propre le devient.
        if self.reference.is_none() {
            if lap.meta.is_clean() {
                let names = crate::tracks::corner_names_for(&self.meta, Some(&lap.trace), self.user_names.clone());
                self.reference = Some(ReferenceLap { label: "ton meilleur tour de la session".into(), meta: lap.meta.clone(), trace: lap.trace.clone(), names });
                return msg(CoachMessageKind::Info, format!("Tour de référence enregistré, {}. On compare à partir du prochain.", say_lap(t)), None, None, None);
            }
            return msg(CoachMessageKind::Invalid, "Tour non propre, je ne peux pas encore en faire une référence.".into(), None, None, None);
        }
        let rf = self.reference.as_ref().unwrap();
        let ref_time = rf.meta.lap_time_s;
        let delta = match (t, ref_time) {
            (Some(a), Some(b)) => Some(a - b),
            _ => None,
        };
        let comparison = compare_laps_named(
            &LapInput { lap_id: 0, session: &self.meta, lap: &rf.meta, trace: &rf.trace },
            &LapInput { lap_id: 1, session: &self.meta, lap: &lap.meta, trace: &lap.trace },
            1.0,
            &rf.names,
        );
        let Ok(c) = comparison else {
            return msg(CoachMessageKind::Invalid, "Tour incomplet, non analysé.".into(), None, None, delta);
        };
        if !lap.meta.is_clean() {
            let where_ = c
                .cmp_off_track_m
                .first()
                .and_then(|(a, _)| c.corners.iter().min_by(|x, y| (x.corner.apex_m - a).abs().total_cmp(&(y.corner.apex_m - a).abs())))
                .map(|k| spoken(&k.corner.name));
            let reminder = self.last_focus.as_ref().map(|(_, name, _)| format!(" Focus toujours {name}.")).unwrap_or_default();
            let text = match (lap.meta.flags.off_track, where_) {
                (true, Some(w)) => {
                    let n = self.offs.entry(w.clone()).or_insert(0);
                    *n += 1;
                    let at = at_corner(&w);
                    if *n >= 2 {
                        format!("Tour non compté, {} sortie {at}. Garde de la marge ici.{reminder}", ordinal(*n))
                    } else {
                        format!("Tour non compté, sortie de piste {at}.{reminder}")
                    }
                }
                (true, None) => format!("Tour non compté, sortie de piste.{reminder}"),
                _ => format!("Tour non compté, incident.{reminder}"),
            };
            return msg(CoachMessageKind::Invalid, text, None, None, delta);
        }
        // Référence de session : un tour plus rapide la remplace.
        if self.self_reference && delta.is_some_and(|d| d < 0.0) {
            let names = crate::tracks::corner_names_for(&self.meta, Some(&lap.trace), self.user_names.clone());
            self.reference = Some(ReferenceLap { label: "ton meilleur tour de la session".into(), meta: lap.meta.clone(), trace: lap.trace.clone(), names });
            self.history.clear();
            self.last_focus = None;
            self.callout = None;
            self.best_lap = t;
            return msg(CoachMessageKind::Info, format!("Meilleur tour, {}. Nouvelle référence.", say_lap(t)), None, None, delta);
        }
        self.last_delta = delta;
        let session_best = t.is_some_and(|x| self.best_lap.is_none_or(|b| x < b));
        if session_best {
            self.best_lap = t;
        }
        self.history.push_back(analyse(&c));
        while self.history.len() > WINDOW {
            self.history.pop_front();
        }
        let mut head = match delta {
            Some(d) if d < -0.05 => format!("Moins {} sur la référence, excellent.", say_time(d)),
            Some(d) if d.abs() <= 0.05 => "À égalité avec la référence.".to_string(),
            Some(d) => format!("Plus {}.", say_time(d)),
            None => String::new(),
        };
        if session_best && self.history.len() > 1 {
            head = format!("Meilleur tour, {}. {head}", say_lap(t));
        }
        let laps = self.history.len();
        let length = self.meta.track_length_m.unwrap_or(c.length_m);
        // Synthèse : perte moyenne par virage sur les derniers tours propres, perte répétée exigée.
        let mut best: Option<(f32, Vec<&CornerStat>)> = None;
        let ids: Vec<u32> = self.history.back().map(|h| h.corners.iter().map(|s| s.id).collect()).unwrap_or_default();
        for id in ids {
            let stats: Vec<&CornerStat> = self.history.iter().filter_map(|h| h.corners.iter().find(|s| s.id == id)).collect();
            let mean = stats.iter().map(|s| s.loss_s).sum::<f32>() / stats.len() as f32;
            let repeated = stats.iter().filter(|s| s.loss_s >= LOSS_MIN_S).count() >= laps.min(2);
            if repeated && mean >= FOCUS_MIN_S && best.as_ref().is_none_or(|(m, _)| mean > *m) {
                best = Some((mean, stats));
            }
        }
        // Un ingénieur garde le pilote sur un seul point : le focus précédent reste tant qu'il coûte
        // encore du temps et qu'aucun autre virage ne coûte nettement plus.
        let mut solved = String::new();
        if let Some((pid, pname, ploss)) = self.last_focus.clone() {
            let prev: Vec<&CornerStat> = self.history.iter().filter_map(|h| h.corners.iter().find(|s| s.id == pid)).collect();
            let prev_mean = if prev.is_empty() { 0.0 } else { prev.iter().map(|s| s.loss_s).sum::<f32>() / prev.len() as f32 };
            let last = prev.last().map(|s| s.loss_s).unwrap_or(0.0);
            let still = prev_mean >= FOCUS_MIN_S && last >= LOSS_MIN_S;
            if still && best.as_ref().is_some_and(|(m, s)| s[0].id != pid && *m - prev_mean < 0.08) {
                best = Some((prev_mean, prev));
            } else if (!still || last < ploss * 0.5) && best.as_ref().is_none_or(|(_, s)| s[0].id != pid) {
                solved = format!("{} : c'est mieux. ", capitalize(&pname));
            }
        }
        let lead = if laps >= 2 { "Focus" } else { "Premier relevé, focus" };
        let prev_focus = self.last_focus.as_ref().map(|f| f.0);
        match best {
            Some((mean, stats)) => {
                let s0 = stats[0];
                let name = spoken(&s0.name);
                let sector = ((s0.apex_m / length.max(1.0)) * 3.0).floor().clamp(0.0, 2.0) as i32 + 1;
                let losing: Vec<&CornerStat> = stats.iter().copied().filter(|s| s.loss_s >= LOSS_MIN_S).collect();
                let cue = cue_for(if losing.is_empty() { &stats } else { &losing });
                // Même focus, même consigne qu'au tour précédent : version courte, comme à la radio.
                let same = prev_focus == Some(s0.id) && self.last_cue.as_deref() == Some(cue.short());
                let text = if same {
                    format!("{head} {solved}Toujours {name} : {}", cue.say().to_lowercase())
                } else {
                    format!("{head} {solved}{lead} {name}, secteur {sector} : {} perdus. {}", say_time(mean as f64), cue.say())
                };
                self.last_cue = Some(cue.short().to_string());
                self.last_advice = Some(format!("Focus {name}, secteur {sector} : {} perdus. {}", say_time(mean as f64), cue.say()));
                self.last_focus = Some((s0.id, name.clone(), mean));
                // Annonce juste avant le virage, sauf s'il suit de près la ligne (le message du tour suffit).
                let trig = s0.trigger_m;
                self.callout = (trig >= 400.0).then(|| (trig, format!("{}, {}.", capitalize(&name), cue.short())));
                msg(CoachMessageKind::Focus, text.trim().to_string(), Some(name), Some(mean), delta)
            }
            None => {
                self.last_focus = None;
                self.callout = None;
                self.last_advice = Some("Rien de significatif à corriger, garde ce rythme.".into());
                msg(CoachMessageKind::Focus, format!("{head} {solved}Rien de significatif à corriger, garde ce rythme.").trim().to_string(), None, None, delta)
            }
        }
    }
}

/// « au virage 7 », « à Descida do Lago ».
fn at_corner(name: &str) -> String {
    if name.starts_with("virage") {
        format!("au {name}")
    } else {
        format!("à {name}")
    }
}

fn ordinal(n: u32) -> String {
    match n {
        2 => "deuxième".into(),
        3 => "troisième".into(),
        4 => "quatrième".into(),
        5 => "cinquième".into(),
        n => format!("{n}e"),
    }
}

/// Circuit, configuration et voiture d'une session en direct, d'après son YAML.
pub fn session_meta_from_yaml(yaml: &str) -> SessionMeta {
    meta_from(&crate::ingest::ibt::parse_session_info(yaml))
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Temps au tour prononçable : « 1 minute 51 point 2 ».
fn say_lap(t: Option<f64>) -> String {
    match t {
        Some(t) => {
            let m = (t / 60.0).floor() as i32;
            let s = t - m as f64 * 60.0;
            let (sec, tenth) = (s.floor() as i32, ((s - s.floor()) * 10.0).floor() as i32);
            if m > 0 {
                format!("{m} minute {sec} point {tenth}")
            } else {
                format!("{sec} secondes point {tenth}")
            }
        }
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::ibt::{ibt_live_rows, writer};

    #[test]
    fn spoken_numbers() {
        assert_eq!(say_time(0.31), "3 dixièmes");
        assert_eq!(say_time(-0.1), "1 dixième");
        assert_eq!(say_time(0.02), "moins d'un dixième");
        assert_eq!(say_time(1.24), "1 seconde 2");
        assert_eq!(say_time(2.0), "2 secondes");
        assert_eq!(say_time(1.97), "2 secondes");
        assert_eq!(spoken("V7"), "virage 7");
        assert_eq!(spoken("Descida do Lago"), "Descida do Lago");
        assert_eq!(say_lap(Some(111.08)), "1 minute 51 point 0");
        assert_eq!(round_to(13.0, 5.0), 15);
        assert_eq!(round_to(2.0, 5.0), 5);
    }

    fn stat(loss: f32, brake: Option<f32>, vmin: f32) -> CornerStat {
        CornerStat { id: 1, name: "V1".into(), apex_m: 100.0, loss_s: loss, brake_early_m: brake, needless_brake: false, turn_in_late_m: None, vmin_deficit_kmh: vmin, throttle_late_m: None, exit_deficit_kmh: 0.0, trigger_m: 0.0 }
    }

    #[test]
    fn cues_follow_the_measured_cause() {
        let a = stat(0.2, Some(14.0), 2.0);
        assert_eq!(cue_for(&[&a, &a]), Cue::BrakeLater(14.0 * 0.7));
        let b = stat(0.2, Some(1.0), 6.0);
        assert_eq!(cue_for(&[&b]), Cue::CarrySpeed(6.0));
        let mut c = stat(0.2, Some(-12.0), 5.0);
        assert_eq!(cue_for(&[&c]), Cue::BrakeEarlier);
        c.brake_early_m = Some(0.0);
        c.vmin_deficit_kmh = 0.0;
        c.turn_in_late_m = Some(11.0);
        assert_eq!(cue_for(&[&c]), Cue::TurnEarlier);
        c.turn_in_late_m = None;
        c.throttle_late_m = Some(15.0);
        assert_eq!(cue_for(&[&c]), Cue::ThrottleEarlier);
        c.throttle_late_m = None;
        assert_eq!(cue_for(&[&c]), Cue::Line, "aucune cause mesurée : pas de consigne inventée");
        assert_eq!(Cue::BrakeLater(9.8).say(), "Freine 10 mètres plus tard.");
    }

    /// Rejoue un fichier comme une session en direct : un message par tour terminé, sans référence au départ.
    #[test]
    fn replayed_session_talks_once_per_lap() {
        let b = writer::synthetic();
        let (yaml, rows) = ibt_live_rows(&b).unwrap();
        let file = crate::ingest::ibt::import_ibt_bytes(&b).unwrap();
        let (mut coach, hello) = LiveCoach::new(&yaml, None, vec![]);
        assert!(hello.text.contains("meilleur tour propre"));
        let msgs: Vec<CoachMessage> = rows.into_iter().filter_map(|r| coach.push(r)).collect();
        assert_eq!(msgs.len(), file.laps.iter().filter(|l| l.meta.lap_time_s.is_some()).count());
        assert!(msgs.iter().all(|m| !m.text.is_empty()));
    }
}
