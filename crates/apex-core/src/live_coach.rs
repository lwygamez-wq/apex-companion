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
    /// Bip de freinage (texte : « bip » ou « long » pour le dernier, au point de freinage de la référence).
    Beep,
    /// Ingénieur de course : stratégie carburant / énergie, appel aux stands (en course).
    Strategy,
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
    /// Point de freinage de la référence et sa vitesse à cet endroit (m, m/s), si elle freine vraiment.
    ref_brake: Option<(f32, f32)>,
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
                ref_brake: rf.brake_on_m.filter(|_| rb).map(|b| (b, c.ref_trace.speed_kmh.get(c.ref_trace.idx(b)).copied().unwrap_or(150.0) / 3.6)),
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
    /// Points de freinage de la référence (LEDs du volant), recalculés quand elle change.
    brake_pts: (Option<(u64, usize)>, Vec<crate::led::BrakePoint>),
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
    /// Programme du jour : texte radio et virages à privilégier comme focus.
    plan_text: Option<String>,
    plan_apexes: Vec<f32>,
    /// Bips de freinage du virage focus : (point de freinage, vitesse) et bips déjà joués ce tour-ci.
    beeps: Option<(f32, f32)>,
    beep_stage: u8,
    /// Carburant : niveau actuel (l) et consommation des derniers tours complets (l).
    fuel_now: f64,
    fuel_laps: VecDeque<f64>,
    /// Plein utilisable (l), d'après la fiche voiture de la session.
    tank_l: Option<f64>,
    /// Temps (s) et tours restants de la séance, position dans le tour (0..1).
    time_remain: f64,
    laps_remain: f64,
    lap_pct: f64,
    /// Énergie virtuelle (Le Mans Ultimate) : niveau (%) et consommation des derniers tours (%).
    ve_now: f64,
    ve_laps: VecDeque<f64>,
    /// Limites de piste (Le Mans Ultimate) : avertissements en cours et nombre par pénalité.
    tl_steps: f64,
    tl_per_penalty: f64,
    /// Ingénieur de course (stratégie en course) et son message en attente.
    engineer: crate::race_engineer::Engineer,
    pending: Option<CoachMessage>,
    /// Temps des derniers tours lancés (s), pour estimer les tours restants d'une course au temps.
    lap_times: VecDeque<f64>,
    /// Carburant (l) et énergie (%) d'un même tour : ratio carburant mesuré en piste.
    pairs: VecDeque<(f32, f32)>,
    /// Avant le premier tour mesuré : consommation et ratio mesurés sur tes séances précédentes.
    hint_per: Option<(crate::analysis::strategy::Unit, f32)>,
    hint_ratio: Option<f32>,
    top_class: bool,
    pit_loss_s: Option<f64>,
    /// Retard sur le leader (tours, secondes) : maintenant, et à chacun des derniers passages de ligne.
    gap_now: (f64, f64),
    gaps: VecDeque<(f64, f64)>,
}

/// Relais en cours, pour l'écran « Au volant » et la commande « carburant ».
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LiveFuel {
    /// Ce qui limite le relais : l'énergie virtuelle si la voiture en a, sinon le carburant.
    pub unit: crate::analysis::strategy::Unit,
    /// Niveau actuel (% d'énergie, ou litres).
    pub level: f32,
    /// Contenance (100 % d'énergie, ou plein utilisable en litres).
    pub capacity: Option<f32>,
    /// Carburant à bord (l), affiché en plus quand l'énergie limite.
    pub fuel_l: Option<f32>,
    /// Consommation moyenne des 3 derniers tours lancés.
    pub per_lap: Option<f32>,
    /// Tours possibles avec le niveau actuel.
    pub laps_in_tank: Option<f32>,
    /// Course : tours restant à couvrir (tour en cours compris).
    pub laps_left: Option<f32>,
    /// Course : niveau qu'il faudrait pour finir, et arrêts nécessaires.
    pub need_to_finish: Option<f32>,
    pub stops_needed: Option<u32>,
    /// Course : économie par tour qui éviterait l'arrêt, si elle est raisonnable.
    pub save_per_lap: Option<f32>,
    /// Énergie virtuelle : ratio carburant conseillé (l par 1 %), et tours possibles avec le carburant à bord.
    pub fuel_ratio: Option<f32>,
    pub fuel_laps: Option<f32>,
    /// Course : dernière consigne de l'ingénieur (cible, lift ou attaque, arrêt, réglages au stand).
    pub engineer: Option<crate::race_engineer::EngineerCall>,
}

/// Tours propres pris en compte dans la synthèse.
const WINDOW: usize = 3;
/// Perte minimale pour qu'un virage compte comme « perdu » sur un tour (s).
const LOSS_MIN_S: f32 = 0.03;
/// Perte moyenne minimale pour devenir le focus (s).
const FOCUS_MIN_S: f32 = 0.05;

fn meta_from(si: &SessionInfo) -> SessionMeta {
    SessionMeta {
        sim: si.sim.unwrap_or(SimId::Iracing),
        track: si.track_display_name.clone().unwrap_or_else(|| "Circuit inconnu".into()),
        track_config: si.track_config.clone(),
        track_length_m: si.track_length_m,
        car: si.car_screen_name.clone().unwrap_or_else(|| "Voiture inconnue".into()),
        kind: SessionKind::Unknown,
        started_at_unix: 0,
        weather: Weather::default(),
        source_fingerprint: "live".into(),
        source_path: None,
        setup: si.setup_type(),
    }
}

/// Ce que le coach dit en course à la fin d'un tour. Le conseil virage par virage se tait : il compare
/// à un tour d'essais (autre charge, pas de trafic) et pousse à freiner plus tard un pilote qui gère
/// une course (Imola, 12 nov. : un conseil de freinage par tour pendant 90 min, 7 tours hors piste).
/// Restent : les sorties répétées au même endroit, les changements de référence, et l'ingénieur.
pub fn race_voice(mut m: CoachMessage) -> Option<CoachMessage> {
    match m.kind {
        CoachMessageKind::Focus => None,
        CoachMessageKind::Invalid if m.text.contains("Garde de la marge") => {
            if let Some(cut) = m.text.find(" Focus toujours") {
                m.text.truncate(cut);
            }
            Some(m)
        }
        CoachMessageKind::Invalid => None,
        _ => Some(m),
    }
}

/// Tours restants d'une course au temps, tour en cours compté pour la part qui reste (`rem`).
/// Être doublé ne retire pas de tour : tu finis toujours au premier passage après la fin du chrono.
/// Le leader peut en ajouter un : s'il passe la ligne juste avant la fin, il repart pour un tour.
/// Le drapeau tombe quand le leader passe la ligne après la fin du chrono (`t` secondes) ; on finit
/// le tour en cours à cet instant. Le leader a `d` secondes d'avance à ton rythme `lt` et te reprend
/// `gain` secondes par tour. Leader toi-même (`d` = 0) : la règle « tour en cours + tours entamés ».
pub fn timed_laps_left(rem: f64, lt: f64, t: f64, d: f64, gain: f64) -> f64 {
    let lt_l = (lt - gain).max(lt * 0.9);
    let c1 = rem * lt;
    let l1 = if d <= 0.0 { c1 } else { (c1 - d).rem_euclid(lt_l) };
    let flag = if l1 >= t { l1 } else { l1 + ((t - l1) / lt_l).ceil() * lt_l };
    rem + ((flag - c1) / lt).ceil().max(0.0)
}

impl LiveCoach {
    /// `reference` : tour de référence du même circuit et de la même voiture (perso ou externe),
    /// `None` si aucun : le coach prendra ton meilleur tour propre de la session.
    /// `plan` : programme du jour (texte radio, virages à privilégier), s'il y en a un pour ce circuit.
    pub fn new(session_yaml: &str, reference: Option<ReferenceLap>, user_names: Vec<CornerName>, plan: Option<(String, Vec<f32>)>) -> (Self, CoachMessage) {
        let builder = LiveLapBuilder::new(session_yaml);
        let meta = meta_from(builder.session_info());
        let tank_l = builder.session_info().profile.usable_tank_l().map(f64::from);
        let mut text = match &reference {
            Some(r) => format!("Coach prêt. Référence : {}.", r.label),
            None => "Coach prêt. Pas de référence enregistrée ici : je prends ton meilleur tour propre de la session.".into(),
        };
        if let Some((p, _)) = &plan {
            text = format!("{text} {p}");
        }
        let (plan_text, plan_apexes) = match plan {
            Some((t, a)) => (Some(t), a),
            None => (None, vec![]),
        };
        let self_reference = reference.is_none();
        // Catégorie lue dans la séance elle-même : valable même sans historique sur ce circuit.
        let top_class = crate::analysis::strategy::is_top_class(meta.sim, builder.session_info().profile.car.class.as_deref());
        (
            Self { builder, meta, reference, self_reference, history: VecDeque::new(), last_focus: None, last_session_num: -1.0, user_names, offs: Default::default(), callout: None, callout_done: false, last_dist: 0.0, best_lap: None, last_delta: None, last_advice: None, last_cue: None, plan_text, plan_apexes, beeps: None, beep_stage: 0, fuel_now: 0.0, fuel_laps: VecDeque::new(), tank_l, time_remain: -1.0, laps_remain: -1.0, lap_pct: 0.0, brake_pts: (None, vec![]), ve_now: 0.0, ve_laps: VecDeque::new(), tl_steps: 0.0, tl_per_penalty: 0.0, engineer: crate::race_engineer::Engineer::new(Default::default()), pending: None, lap_times: VecDeque::new(), pairs: VecDeque::new(), hint_per: None, hint_ratio: None, top_class, pit_loss_s: None, gap_now: (0.0, 0.0), gaps: VecDeque::new() },
            CoachMessage { kind: CoachMessageKind::Info, lap: 0, text, focus: None, focus_loss_s: None, lap_delta_s: None, lap_time_s: None },
        )
    }

    pub fn session(&self) -> &SessionMeta {
        &self.meta
    }

    /// Plan de l'ingénieur (choisi dans le calculateur) et mesures de tes séances précédentes, pour
    /// annoncer la stratégie dès le premier tour de course.
    pub fn set_engineer(&mut self, plan: crate::race_engineer::EngineerPlan, per_lap: Option<(crate::analysis::strategy::Unit, f32)>, fuel_ratio: Option<f32>) {
        self.engineer.set_plan(plan);
        self.hint_per = per_lap.filter(|(_, p)| *p > 0.0);
        self.hint_ratio = fuel_ratio.filter(|r| *r > 0.0);
    }

    /// Fin de course au temps : catégorie la plus rapide (pas de tour en plus) et temps perdu par
    /// arrêt mesuré sur tes courses (les arrêts restants mangent du chrono, donc des tours).
    pub fn set_race_context(&mut self, top_class: bool, pit_loss_s: Option<f32>) {
        // La catégorie de la séance prime : l'historique peut manquer (nouvelle voiture).
        self.top_class |= top_class;
        self.pit_loss_s = pit_loss_s.filter(|p| *p > 0.0).map(f64::from);
    }

    /// Nouveau plan choisi pendant la séance.
    pub fn set_engineer_plan(&mut self, plan: crate::race_engineer::EngineerPlan) {
        self.engineer.set_plan(plan);
    }

    /// Message de l'ingénieur en attente (émis au passage de la ligne, en course).
    pub fn take_engineer(&mut self) -> Option<CoachMessage> {
        self.pending.take()
    }

    /// Type de la séance en cours (essais, qualif, course), d'après le numéro de session.
    pub fn current_kind(&self) -> SessionKind {
        let si = self.builder.session_info();
        si.session_types.iter().find(|(n, _)| *n as f64 == self.last_session_num).map(|(_, l)| SessionKind::from_label(l)).unwrap_or(SessionKind::Unknown)
    }

    /// Un échantillon de télémétrie (ordre de `LIVE_CHANNELS`). Renvoie le message radio quand un tour se termine.
    pub fn push(&mut self, row: [f64; crate::ingest::ibt::LIVE_N]) -> Option<CoachMessage> {
        if row[17] != self.last_session_num {
            // Nouvelle séance (essais → qualif → course) : on repart d'une synthèse vierge.
            self.last_session_num = row[17];
            self.history.clear();
            self.last_focus = None;
            self.offs.clear();
            self.callout = None;
            self.beeps = None;
            self.best_lap = None;
            self.tl_steps = row[23];
            self.engineer.reset();
            self.pending = None;
            self.lap_times.clear();
            self.gaps.clear();
        }
        // Annonce du virage du focus, une fois par tour, quand la voiture passe le point de déclenchement.
        let dist = row[1];
        let mut callout = None;
        // En course : pas de rappel de virage (voir `race_voice`).
        let racing = self.current_kind() == SessionKind::Race;
        if let Some((trig, text)) = self.callout.as_ref().filter(|_| !racing) {
            let t = *trig as f64;
            if !self.callout_done && self.last_dist < t && dist >= t && dist - self.last_dist < 100.0 {
                self.callout_done = true;
                callout = Some(CoachMessage { kind: CoachMessageKind::Callout, lap: row[6] as i32, text: text.clone(), focus: None, focus_loss_s: None, lap_delta_s: None, lap_time_s: None });
            }
        }
        // Bips de freinage : à 1 s, à 0,5 s, puis long au point de freinage de la référence.
        if callout.is_none() {
            if let Some((brake, v)) = self.beeps {
                let marks = [brake - v * 1.0, brake - v * 0.5, brake];
                // Un échantillon peut sauter plusieurs repères (boucle ralentie) : on les passe tous et on
                // joue le dernier, pour que le bip long du point de freinage ne soit jamais perdu.
                let mut fired = None;
                while (self.beep_stage as usize) < 3 && dist - self.last_dist < 100.0 {
                    let m = marks[self.beep_stage as usize] as f64;
                    if !(self.last_dist < m && dist >= m) {
                        break;
                    }
                    fired = Some(self.beep_stage);
                    self.beep_stage += 1;
                }
                if let Some(stage) = fired {
                    callout = Some(CoachMessage { kind: CoachMessageKind::Beep, lap: row[6] as i32, text: if stage == 2 { "long".into() } else { "bip".into() }, focus: None, focus_loss_s: None, lap_delta_s: None, lap_time_s: None });
                }
            }
        }
        self.last_dist = dist;
        if row[12] > 0.0 {
            self.fuel_now = row[12];
        }
        if row[22] > 0.0 {
            self.ve_now = row[22];
        }
        // Ravitaillement : le niveau qui limite le relais remonte.
        self.engineer.observe(if self.ve_now > 0.0 { self.ve_now } else { self.fuel_now });
        // Limites de piste : annonce à chaque nouvel avertissement, avec le compte avant pénalité.
        if row[23] > self.tl_steps && row[24] > 0.0 && callout.is_none() {
            let (n, per) = (row[23] as i64, row[24] as i64);
            let left = (per - n.rem_euclid(per.max(1))).rem_euclid(per.max(1));
            let text = if left == 0 {
                format!("Limites de piste : {n} avertissement{}, pénalité.", if n > 1 { "s" } else { "" })
            } else {
                format!("Limites de piste : avertissement {} sur {per}. Encore {left} avant pénalité.", n.rem_euclid(per.max(1)))
            };
            // Message d'information (pas un rappel de virage) : jamais coupé par l'option « rappels ».
            callout = Some(CoachMessage { kind: CoachMessageKind::Info, lap: row[6] as i32, text, focus: None, focus_loss_s: None, lap_delta_s: None, lap_time_s: None });
        }
        self.tl_steps = row[23];
        self.tl_per_penalty = row[24];
        self.time_remain = row[20];
        self.laps_remain = row[21];
        self.lap_pct = row[2].clamp(0.0, 1.0);
        self.gap_now = (row[25], row[26]);
        match self.builder.push(row) {
            Some(lap) => {
                // Écart au leader relevé à chaque passage de ligne : son évolution donne le rythme du leader.
                self.gaps.push_back(self.gap_now);
                while self.gaps.len() > 4 {
                    self.gaps.pop_front();
                }
                self.callout_done = false;
                self.beep_stage = 0;
                let flying = lap.meta.kind == LapKind::Flying;
                let fuel = lap.meta.fuel_used_l.filter(|u| *u > 0.05 && flying);
                let energy = lap.meta.energy_used_pct.filter(|u| *u > 0.1 && flying);
                if let Some(u) = fuel {
                    push_consumption(&mut self.fuel_laps, u as f64);
                }
                if let Some(u) = energy {
                    push_consumption(&mut self.ve_laps, u as f64);
                }
                if let (Some(f), Some(e)) = (fuel, energy) {
                    self.pairs.push_back((f, e));
                    while self.pairs.len() > 5 {
                        self.pairs.pop_front();
                    }
                }
                if let Some(t) = lap.meta.lap_time_s.filter(|_| flying) {
                    self.lap_times.push_back(t);
                    while self.lap_times.len() > 3 {
                        self.lap_times.pop_front();
                    }
                }
                let number = lap.meta.number;
                let msg = self.on_lap(lap);
                let msg = if self.current_kind() == SessionKind::Race { race_voice(msg) } else { Some(msg) };
                // Ingénieur de course : consigne recalculée à chaque passage de la ligne, en course.
                if let Some(st) = self.race_state(number) {
                    if let Some(text) = self.engineer.on_lap(&st) {
                        let strategy = CoachMessage { kind: CoachMessageKind::Strategy, lap: number, text, focus: None, focus_loss_s: None, lap_delta_s: None, lap_time_s: None };
                        // Rien d'autre à dire sur ce tour : la consigne part tout de suite.
                        if msg.is_none() {
                            return Some(strategy);
                        }
                        self.pending = Some(strategy);
                    }
                }
                msg
            }
            None => callout,
        }
    }

    /// Repère de freinage pour les LEDs du volant, à l'échantillon `row` (ordre de `LIVE_CHANNELS`).
    pub fn led_cue(&mut self, row: &[f64; crate::ingest::ibt::LIVE_N]) -> crate::led::LedCue {
        let key = self.reference.as_ref().map(|r| (r.meta.lap_time_s.unwrap_or(0.0).to_bits(), r.trace.len()));
        if key != self.brake_pts.0 {
            self.brake_pts = (key, self.reference.as_ref().map(|r| crate::led::brake_points(&r.trace)).unwrap_or_default());
        }
        let len = self.meta.track_length_m.unwrap_or(0.0);
        // Aux stands ou hors piste : rien à afficher.
        if row[13] > 0.5 {
            return crate::led::LedCue { seconds: 99.0, ..Default::default() };
        }
        crate::led::cue(&self.brake_pts.1, len, row[1] as f32, row[3] as f32)
    }

    /// Programme du jour (commande « plan »).
    pub fn plan_text(&self) -> String {
        self.plan_text.clone().unwrap_or_else(|| "Pas de plan pour cette séance.".into())
    }

    /// Tours restant à couvrir en course (tour en cours compris) : donnés par le jeu, sinon estimés sur le chrono.
    /// Course au temps : fin du tour en cours, tours commencés avant la fin du chrono, plus un tour
    /// (en multiclasse, le drapeau tombe quand le leader de la catégorie la plus rapide passe la ligne :
    /// une catégorie plus lente peut boucler un tour de plus ; même règle que le calculateur).
    fn race_laps_left(&self) -> Option<f64> {
        if self.current_kind() != SessionKind::Race {
            return None;
        }
        let rem = 1.0 - self.lap_pct;
        if self.laps_remain > 0.0 && self.laps_remain < 30_000.0 {
            Some(self.laps_remain + rem)
        } else if self.time_remain > 0.0 {
            let mut t: Vec<f64> = self.lap_times.iter().copied().collect();
            t.sort_by(|a, b| a.total_cmp(b));
            let lt = t.get(t.len() / 2).copied().or(self.best_lap.map(|b| b * 1.01)).filter(|l| *l > 1.0)?;
            // Arrêts encore prévus : leur temps ne fait pas de tours.
            let pits = self.engineer.stops_left().unwrap_or(0) as f64 * self.pit_loss_s.unwrap_or(0.0);
            let t = self.time_remain - pits;
            if !self.top_class {
                // Hors catégorie la plus rapide : le leader est dans une autre catégorie, un tour de plus possible.
                return Some(rem + ((t - rem * lt) / lt).ceil().max(0.0) + 1.0);
            }
            // Le drapeau tombe quand le leader passe la ligne après la fin du chrono ; on finit le tour en
            // cours à ce moment-là. Le leader a `d` secondes d'avance (à ton rythme) et tourne en `lt_l`.
            Some(timed_laps_left(rem, lt, t, self.leader_gap(lt), self.leader_gain(lt)))
        } else {
            None
        }
    }

    /// Avance du leader en secondes, à ton rythme : tours de retard × temps au tour + écart dans le tour.
    /// (Que le jeu donne l'écart dans le tour ou au total, le modulo le ramène dans le tour.)
    fn leader_gap(&self, lt: f64) -> f64 {
        let (laps, secs) = self.gap_now;
        laps.max(0.0).floor() * lt + secs.max(0.0).rem_euclid(lt)
    }

    /// Ce que le leader te reprend par tour (s), d'après l'écart aux derniers passages de ligne.
    fn leader_gain(&self, lt: f64) -> f64 {
        let d: Vec<f64> = self.gaps.iter().map(|&(l, s)| l.max(0.0).floor() * lt + s.max(0.0).rem_euclid(lt)).collect();
        let mut g: Vec<f64> = d.windows(2).map(|w| w[1] - w[0]).filter(|x| x.abs() < lt * 0.1).collect();
        g.sort_by(|a, b| a.total_cmp(b));
        g.get(g.len() / 2).copied().unwrap_or(0.0).clamp(0.0, lt * 0.1)
    }

    /// La course vue par l'ingénieur au passage de la ligne : `None` hors course ou sans mesure.
    fn race_state(&self, lap: i32) -> Option<crate::race_engineer::RaceState> {
        use crate::analysis::strategy::Unit;
        if self.current_kind() != SessionKind::Race {
            return None;
        }
        let mean = |q: &VecDeque<f64>| (!q.is_empty()).then(|| (q.iter().sum::<f64>() / q.len() as f64) as f32);
        let pairs: Vec<(f32, f32)> = self.pairs.iter().copied().collect();
        let ratio = crate::analysis::strategy::fuel_ratio(&pairs);
        let fuel_per = mean(&self.fuel_laps);
        let hint = |u: Unit| self.hint_per.filter(|(hu, _)| *hu == u).map(|(_, p)| p);
        let laps_left = (self.race_laps_left()? - 0.05).ceil().max(0.0) as u32;
        if self.ve_now > 0.0 {
            let per = mean(&self.ve_laps).or(hint(Unit::Energy))?;
            let ratio_set = ratio.map(|r| r.1).or(self.hint_ratio);
            // Le carburant à bord, ramené en énergie : s'il lâche avant, c'est lui qui limite.
            let mut level = self.ve_now as f32;
            if let Some((m, _)) = ratio.filter(|_| self.fuel_now > 0.0) {
                level = level.min(self.fuel_now as f32 / m);
            }
            let cap = match (self.tank_l, ratio_set) {
                (Some(t), Some(r)) if r > 0.0 => (t as f32 / r).min(100.0),
                _ => 100.0,
            };
            Some(crate::race_engineer::RaceState {
                unit: Unit::Energy,
                level,
                cap,
                per_lap: per,
                laps_left,
                lap,
                fuel_ratio: ratio_set,
                fuel_laps: fuel_per.filter(|f| *f > 0.0 && self.fuel_now > 0.0).map(|f| self.fuel_now as f32 / f),
                energy_laps: Some(self.ve_now as f32 / per),
            })
        } else if self.fuel_now > 0.0 {
            let per = fuel_per.or(hint(Unit::Litres))?;
            Some(crate::race_engineer::RaceState {
                unit: Unit::Litres,
                level: self.fuel_now as f32,
                cap: self.tank_l? as f32,
                per_lap: per,
                laps_left,
                lap,
                fuel_ratio: None,
                fuel_laps: None,
                energy_laps: None,
            })
        } else {
            None
        }
    }

    /// Rappel de la stratégie (commande « stratégie »).
    pub fn strategy_text(&self) -> String {
        use crate::analysis::strategy::Unit;
        if self.current_kind() != SessionKind::Race {
            let mut t = "Pas en course : l'ingénieur annonce la stratégie au premier passage de la ligne.".to_string();
            if let Some(f) = self.fuel_snapshot() {
                if let (Unit::Energy, Some(p)) = (f.unit, f.per_lap) {
                    t += &format!(" 100 % d'énergie font {} tours", ((100.0 / p) + 1e-3).floor() as i64);
                    if let Some(r) = f.fuel_ratio {
                        t += &format!(", ratio carburant conseillé {}", format!("{r:.2}").replace('.', ","));
                    }
                    t += ".";
                }
            }
            return t;
        }
        self.engineer.summary(if self.ve_now > 0.0 { Unit::Energy } else { Unit::Litres })
    }

    /// Relais en cours : énergie virtuelle si la voiture en a (Le Mans Ultimate, Hypercar et LMGT3),
    /// sinon carburant. `None` sans mesure.
    pub fn fuel_snapshot(&self) -> Option<LiveFuel> {
        use crate::analysis::strategy::Unit;
        let energy = self.ve_now > 0.0;
        let (unit, level, laps, capacity) = if energy {
            (Unit::Energy, self.ve_now, &self.ve_laps, Some(100.0))
        } else if self.fuel_now > 0.0 {
            (Unit::Litres, self.fuel_now, &self.fuel_laps, self.tank_l)
        } else {
            return None;
        };
        let per = (!laps.is_empty()).then(|| laps.iter().sum::<f64>() / laps.len() as f64);
        let left = self.race_laps_left();
        let need = per.zip(left).map(|(p, l)| p * l);
        let stops = need.map(|n| match capacity.filter(|c| *c > 0.0) {
            Some(c) if n > level => ((n - level) / c).ceil() as u32,
            _ => 0,
        });
        let save = per.zip(need).zip(left).and_then(|((p, n), l)| {
            let s = (n - level) / l;
            (s > 0.0 && s <= p * 0.08).then_some(s as f32)
        });
        let pairs: Vec<(f32, f32)> = self.pairs.iter().copied().collect();
        let fuel_per = (!self.fuel_laps.is_empty()).then(|| self.fuel_laps.iter().sum::<f64>() / self.fuel_laps.len() as f64);
        Some(LiveFuel {
            unit,
            level: level as f32,
            capacity: capacity.map(|c| c as f32),
            fuel_l: (energy && self.fuel_now > 0.0).then_some(self.fuel_now as f32),
            per_lap: per.map(|p| p as f32),
            laps_in_tank: per.filter(|p| *p > 0.0).map(|p| (level / p) as f32),
            laps_left: left.map(|l| l as f32),
            need_to_finish: need.map(|n| n as f32),
            stops_needed: stops,
            save_per_lap: save,
            fuel_ratio: if energy { crate::analysis::strategy::fuel_ratio(&pairs).map(|r| r.1).or(self.hint_ratio) } else { None },
            fuel_laps: fuel_per.filter(|f| energy && *f > 0.0 && self.fuel_now > 0.0).map(|f| (self.fuel_now / f) as f32),
            engineer: (self.current_kind() == SessionKind::Race).then(|| self.engineer.last().cloned()).flatten(),
        })
    }

    /// Carburant ou énergie restante (commande « carburant ») : niveau, consommation moyenne, tours possibles.
    pub fn fuel_text(&self) -> String {
        use crate::analysis::strategy::{finish_check, Unit};
        let Some(f) = self.fuel_snapshot() else {
            return "Pas de mesure de carburant.".into();
        };
        let n1 = |v: f32| format!("{v:.1}").replace('.', ",");
        let level = match f.unit {
            Unit::Energy => format!("Énergie : {} %", n1(f.level)),
            Unit::Litres => format!("Carburant : {} litres", n1(f.level)),
        };
        let Some(per) = f.per_lap else {
            return format!("{level}. Consommation au prochain tour complet.");
        };
        let laps = f.laps_in_tank.unwrap_or(0.0).floor() as i64;
        let per_txt = match f.unit {
            Unit::Energy => format!("{} % par tour", n1(per)),
            Unit::Litres => format!("{} litre par tour", n1(per)),
        };
        let mut t = format!("{level}, environ {laps} tour{}. {per_txt}.", if laps > 1 { "s" } else { "" });
        if let Some(left) = f.laps_left {
            let check = finish_check(f.unit, f.level, per, left);
            if !check.is_empty() {
                t = format!("{t} {check}");
            }
            if let Some(n) = f.stops_needed.filter(|n| *n > 1) {
                t += &format!(" {n} arrêts encore.");
            }
            return t;
        }
        // Hors course : autonomie d'un plein, pour préparer l'arrêt ou le départ.
        match f.unit {
            Unit::Energy => t += &format!(" 100 % d'énergie font environ {} tours.", (100.0 / per).floor() as i64),
            Unit::Litres => {
                if let Some(tank) = f.capacity.filter(|t| *t > 0.0 && per > 0.0) {
                    t += &format!(" Un plein, {} litres, fait environ {} tours.", tank.round() as i64, (tank / per).floor() as i64);
                }
            }
        }
        t
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
            self.beeps = None;
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
        let mut best_score = 0.0f32;
        let ids: Vec<u32> = self.history.back().map(|h| h.corners.iter().map(|s| s.id).collect()).unwrap_or_default();
        for id in ids {
            let stats: Vec<&CornerStat> = self.history.iter().filter_map(|h| h.corners.iter().find(|s| s.id == id)).collect();
            let mean = stats.iter().map(|s| s.loss_s).sum::<f32>() / stats.len() as f32;
            let repeated = stats.iter().filter(|s| s.loss_s >= LOSS_MIN_S).count() >= laps.min(2);
            // Virage du programme du jour : privilégié comme focus (à perte comparable).
            let in_plan = stats.first().is_some_and(|s| self.plan_apexes.iter().any(|a| (a - s.apex_m).abs() < 60.0));
            let score = if in_plan { mean * 1.3 } else { mean };
            if repeated && mean >= FOCUS_MIN_S && best_score < score {
                best_score = score;
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
                self.beeps = s0.ref_brake.filter(|(b, v)| *b - v * 1.2 > 150.0);
                msg(CoachMessageKind::Focus, text.trim().to_string(), Some(name), Some(mean), delta)
            }
            None => {
                self.last_focus = None;
                self.callout = None;
                self.beeps = None;
                self.last_advice = Some("Rien de significatif à corriger, garde ce rythme.".into());
                msg(CoachMessageKind::Focus, format!("{head} {solved}Rien de significatif à corriger, garde ce rythme.").trim().to_string(), None, None, delta)
            }
        }
    }
}

/// Consommation d'un tour lancé, gardée sur les 3 derniers tours. Un tour très en dessous des
/// précédents (neutralisation, drapeau jaune) ne compte pas : il fausserait la cible de l'ingénieur.
fn push_consumption(q: &mut VecDeque<f64>, v: f64) {
    if q.len() >= 2 {
        let mut m: Vec<f64> = q.iter().copied().collect();
        m.sort_by(|a, b| a.total_cmp(b));
        if v < 0.7 * m[m.len() / 2] {
            return;
        }
    }
    q.push_back(v);
    while q.len() > 3 {
        q.pop_front();
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
        CornerStat { id: 1, name: "V1".into(), apex_m: 100.0, loss_s: loss, brake_early_m: brake, needless_brake: false, turn_in_late_m: None, vmin_deficit_kmh: vmin, throttle_late_m: None, exit_deficit_kmh: 0.0, trigger_m: 0.0, ref_brake: None }
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
        let (mut coach, hello) = LiveCoach::new(&yaml, None, vec![], None);
        assert!(hello.text.contains("meilleur tour propre"));
        let msgs: Vec<CoachMessage> = rows.into_iter().filter_map(|r| coach.push(r)).collect();
        assert_eq!(msgs.len(), file.laps.iter().filter(|l| l.meta.lap_time_s.is_some()).count());
        assert!(msgs.iter().all(|m| !m.text.is_empty()));
    }

    #[test]
    fn lmu_energy_and_track_limits_in_the_coach() {
        use crate::analysis::strategy::Unit;
        use crate::ingest::lmu::{live_yaml, testkit::*, LiveState};
        // Course LMU (séance 10) : 5 % d'énergie par tour, un avertissement de limites de piste au tour 2.
        let y = live_yaml(&frame(0.0, 0.0, 0.0, 0.0, 10)).unwrap();
        let (mut coach, _) = LiveCoach::new(&y, None, vec![], None);
        let mut live = LiveState::default();
        let mut warnings = vec![];
        let mut et = 0.0f64;
        while et < 200.0 {
            let d = 1500.0 + et * 50.0;
            let sc_et = (et * 5.0).floor() / 5.0;
            let mut f = frame(100.0 + et, d, 100.0 + sc_et, 1500.0 + sc_et * 50.0, 10);
            f.info.mEndET = 100.0 + 40.0 * 12.0;
            if let Some(m) = coach.push(live.row(&f).unwrap()) {
                if m.text.starts_with("Limites de piste") {
                    warnings.push(m.text);
                }
            }
            et += 0.02;
        }
        assert_eq!(warnings, vec!["Limites de piste : avertissement 1 sur 4. Encore 3 avant pénalité.".to_string()]);
        let f = coach.fuel_snapshot().unwrap();
        assert_eq!(f.unit, Unit::Energy);
        assert!((f.per_lap.unwrap() - 5.0).abs() < 0.05, "{f:?}");
        assert!(f.fuel_l.is_some());
        let t = coach.fuel_text();
        assert!(t.starts_with("Énergie : "), "{t}");
        assert!(t.contains("5,0 % par tour"), "{t}");
    }

    /// Course LMU au temps (20 min, tours de 40 s, 5 % d'énergie et 2 l par tour) : l'ingénieur annonce
    /// le relais au premier passage de la ligne, avec la cible et le tour de l'arrêt.
    #[test]
    fn race_engineer_talks_in_an_lmu_race() {
        use crate::ingest::lmu::{live_yaml, testkit::*, LiveState};
        let y = live_yaml(&frame(0.0, 0.0, 0.0, 0.0, 10)).unwrap();
        let (mut coach, _) = LiveCoach::new(&y, None, vec![], None);
        let mut live = LiveState::default();
        let mut said = vec![];
        let mut et = 0.0f64;
        while et < 330.0 {
            let d = 1500.0 + et * 50.0;
            let sc_et = (et * 5.0).floor() / 5.0;
            let mut f = frame(100.0 + et, d, 100.0 + sc_et, 1500.0 + sc_et * 50.0, 10);
            f.info.mEndET = 100.0 + 40.0 * 30.0;
            if let Some(r) = live.row(&f) {
                coach.push(r);
            }
            if let Some(m) = coach.take_engineer() {
                assert_eq!(m.kind, CoachMessageKind::Strategy);
                said.push(m.text);
            }
            et += 0.02;
        }
        assert!(said.first().is_some_and(|m| m.starts_with("Relais 1 sur 2. Box à la fin du tour")), "{said:?}");
        let f = coach.fuel_snapshot().unwrap();
        let e = f.engineer.clone().unwrap();
        assert_eq!(e.stops_left, 1);
        assert!(e.target_per_lap >= 5.0, "{e:?}");
        // Ratio carburant mesuré en piste : 2 l pour 5 % = 0,40, conseillé 0,41 (+2 %).
        assert!((f.fuel_ratio.unwrap() - 0.41).abs() < 1e-3, "{f:?}");
        assert!(!e.fuel_limits);
        let t = coach.strategy_text();
        assert!(t.starts_with("Plan auto"), "{t}");
        assert!(t.contains("ratio 0,41"), "{t}");
    }

    /// Même course : en Hypercar (catégorie la plus rapide), pas de tour en plus ; l'arrêt prévu
    /// (80 s mesurées, soit 2 tours de 40 s) ne fait pas de tours. 3 tours de moins à couvrir.
    #[test]
    fn timed_race_laps_left_drop_the_extra_lap_and_the_pit_time() {
        use crate::ingest::lmu::{live_yaml, testkit::*, LiveState};
        let run = |ctx: Option<(bool, Option<f32>)>| {
            let y = live_yaml(&frame(0.0, 0.0, 0.0, 0.0, 10)).unwrap();
            let (mut coach, _) = LiveCoach::new(&y, None, vec![], None);
            if let Some((top, pit)) = ctx {
                coach.set_race_context(top, pit);
            }
            let mut live = LiveState::default();
            let mut et = 0.0f64;
            while et < 330.0 {
                let d = 1500.0 + et * 50.0;
                let sc_et = (et * 5.0).floor() / 5.0;
                let mut f = frame(100.0 + et, d, 100.0 + sc_et, 1500.0 + sc_et * 50.0, 10);
                f.info.mEndET = 100.0 + 40.0 * 30.0;
                if let Some(r) = live.row(&f) {
                    coach.push(r);
                }
                et += 0.02;
            }
            coach.fuel_snapshot().unwrap().laps_left.unwrap()
        };
        let base = run(None);
        assert!((base - run(Some((true, Some(80.0)))) - 3.0).abs() < 1e-3);
        assert!((base - run(Some((true, None))) - 1.0).abs() < 1e-3);
    }

    /// Hypercar sans historique sur le circuit (nouvelle voiture, Alpine à Imola le 12 nov.) : la
    /// catégorie vient de la séance elle-même, pas du calculateur. Sans ça, un tour en trop et un faux
    /// « 1 arrêt au lieu de 0 » à 8 tours de la fin.
    #[test]
    fn hypercar_class_comes_from_the_live_session() {
        use crate::ingest::lmu::{live_yaml, testkit::*};
        let mut f = frame(0.0, 0.0, 0.0, 0.0, 10);
        if let Some(s) = f.scoring.as_mut() {
            s.mVehicleClass = [0; 32];
            s.mVehicleClass[..5].copy_from_slice(b"Hyper");
        }
        let (coach, _) = LiveCoach::new(&live_yaml(&f).unwrap(), None, vec![], None);
        assert!(coach.top_class);
        // Le calculateur sans historique ne la retire pas.
        let mut coach = coach;
        coach.set_race_context(false, None);
        assert!(coach.top_class);
        let (lmp2, _) = LiveCoach::new(&live_yaml(&frame(0.0, 0.0, 0.0, 0.0, 10)).unwrap(), None, vec![], None);
        assert!(!lmp2.top_class);
    }

    #[test]
    fn timed_laps_left_follows_the_leader() {
        // Leader toi-même : même compte qu'avant (tour en cours + tours entamés avant la fin du chrono).
        for &(rem, t) in &[(1.0f64, 870.0f64), (0.4, 760.0), (0.75, 30.0), (0.2, 5.0)] {
            let lt: f64 = 94.3;
            let old = rem + ((t - rem * lt) / lt).ceil().max(0.0);
            assert_eq!(timed_laps_left(rem, lt, t, 0.0, 0.0), old, "{rem} {t}");
        }
        // Leader 8 s devant (à ton rythme), qui reprend 2 s par tour, tours de 40 s, 870 s au chrono :
        // il passe la ligne à 32 s puis toutes les 38 s ; à 868 s le chrono n'est pas fini, il repart et
        // prend le drapeau à 906 s. Toi : passages à 40, 80… 880, 920 : tu finis à 920, 23 tours.
        assert_eq!(timed_laps_left(1.0, 40.0, 870.0, 8.0, 2.0), 23.0);
        // Sans leader devant, drapeau à ton passage de 880 s : 22 tours.
        assert_eq!(timed_laps_left(1.0, 40.0, 870.0, 0.0, 0.0), 22.0);
        // Leader 35 s devant : il passe à 5 s puis toutes les 38 s, drapeau à 879 s ; tu finis à 880 s,
        // 22 tours comme sans leader. Le leader ne peut que rajouter un tour, jamais en retirer.
        assert_eq!(timed_laps_left(1.0, 40.0, 850.0, 35.0, 2.0), 22.0);
    }

    #[test]
    fn race_voice_keeps_only_what_matters() {
        let m = |kind, text: &str| CoachMessage { kind, lap: 3, text: text.into(), focus: None, focus_loss_s: None, lap_delta_s: None, lap_time_s: None };
        assert!(race_voice(m(CoachMessageKind::Focus, "Plus 1 seconde 3. Focus Rivazza : freine 10 mètres plus tard.")).is_none());
        assert!(race_voice(m(CoachMessageKind::Invalid, "Tour de stand, non compté.")).is_none());
        assert!(race_voice(m(CoachMessageKind::Invalid, "Tour non compté, sortie de piste à Tosa. Focus toujours Rivazza.")).is_none());
        let kept = race_voice(m(CoachMessageKind::Invalid, "Tour non compté, troisième sortie à Villeneuve. Garde de la marge ici. Focus toujours Rivazza.")).unwrap();
        assert_eq!(kept.text, "Tour non compté, troisième sortie à Villeneuve. Garde de la marge ici.");
        assert!(race_voice(m(CoachMessageKind::Strategy, "Box, box, ce tour.")).is_some());
        assert!(race_voice(m(CoachMessageKind::Info, "Meilleur tour, 1 minute 32 point 9. Nouvelle référence.")).is_some());
    }
}
