//! Banc de rejeu de l'ingénieur de course : un enregistrement `.apexlmu` est rejoué comme en direct
//! (mêmes échantillons, même `LiveCoach`, même `Engineer`), puis chaque consigne est comparée à ce qui
//! s'est réellement passé ensuite dans la course.
//!
//! Le temps de séance restant et l'écart au leader ne sont enregistrés qu'aux passages de ligne
//! (`LapEvent`, 0.38+) : entre deux passages, le chrono est décompté au temps écoulé et l'écart reste
//! celui du dernier passage, comme le jeu les donne. Le MFD du jeu n'est pas enregistré : absent ici.

use crate::analysis::strategy::Unit;
use crate::ingest::ibt::LIVE_N;
use crate::ingest::lmu::{session_label, unwrapped_distance, Recording};
use crate::live_coach::{CoachMessageKind, LiveCoach};
use crate::race_engineer::EngineerPlan;
use serde::Serialize;

/// Une consigne de l'ingénieur au passage de la ligne, et la vérité.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LineCall {
    /// Passage de ligne (1 = fin du premier tour vu par le coach).
    pub lap: i32,
    /// Tours restant à couvrir annoncés (tour qui commence compris) et réellement courus ensuite.
    pub laps_left: u32,
    pub laps_left_true: u32,
    /// Distance exacte restante (tours) : estimée par l'ingénieur à la ligne, et réelle.
    pub exact_est: Option<f32>,
    pub exact_true: f32,
    pub per_lap: f32,
    pub fill: Option<f32>,
    pub laps_after_stop: Option<u32>,
}

/// Le plein d'un arrêt : ce que l'ingénieur a demandé, ce qu'il fallait, et pourquoi l'écart.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StopReview {
    /// Tour de rentrée.
    pub lap: i32,
    pub set: f32,
    /// Niveau annoncé : à l'entrée des stands (distance exacte), sinon dernière consigne à la ligne.
    pub announced: Option<f32>,
    pub announced_at_entry: bool,
    /// Distance de course restante depuis l'entrée des stands : estimée par l'ingénieur, réelle (tours).
    pub laps_est: Option<f32>,
    pub laps_true: f32,
    /// Consommation retenue par l'ingénieur, et consommation réelle après l'arrêt (par tour).
    pub per_lap_est: Option<f32>,
    pub per_lap_true: f32,
    /// Énergie (ou litres) consommée de la sortie des stands à l'arrivée.
    pub used_after: f32,
    /// Il fallait : distance réelle × consommation réelle + une demi-tour de marge.
    pub needed: f32,
    /// Ce qui serait resté à l'arrivée en réglant l'annonce, en tours (cible 0,5).
    pub spare_laps_if_announced: Option<f32>,
    /// Restait réellement à l'arrivée, en tours.
    pub spare_laps_actual: f32,
    /// Décomposition de `announced − needed` : erreur de distance, de consommation, arrondi et marge.
    pub err_distance: Option<f32>,
    pub err_consumption: Option<f32>,
    pub err_rounding: Option<f32>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct ReplayReport {
    pub unit: Option<Unit>,
    pub laps: i32,
    pub line_calls: Vec<LineCall>,
    pub stops: Vec<StopReview>,
    /// Messages de stratégie dits pendant le rejeu (tour, texte).
    pub said: Vec<(i32, String)>,
    pub finish_level: f32,
    pub missing: Vec<String>,
}

/// En-tête de séance façon iRacing, comme celui du direct (`lmu::live_yaml`).
fn yaml(r: &Recording) -> String {
    let h = &r.header;
    let mut y = String::from("---\nWeekendInfo:\n ApexSim: lmu\n");
    y += &format!(" TrackDisplayName: {}\n TrackLength: {:.4} km\n", h.track.replace(['\n', '\r'], " "), h.track_length_m / 1000.0);
    y += &format!(" WeekendOptions:\n  IsFixedSetup: {}\n", h.fixed_setup.unwrap_or(false) as u8);
    y += "SessionInfo:\n Sessions:\n";
    for n in 0..=13 {
        y += &format!(" - SessionNum: {n}\n   SessionType: {}\n", session_label(n));
    }
    y += "DriverInfo:\n DriverCarIdx: 0\n";
    if let Some(f) = h.fuel_capacity_l.filter(|f| *f > 0.0) {
        y += &format!(" DriverCarFuelMaxLtr: {f:.3}\n");
    }
    y += &format!(" Drivers:\n - CarIdx: 0\n   CarScreenName: {}\n", h.car.replace(['\n', '\r'], " "));
    if let Some(c) = h.car_class.as_ref().filter(|c| !c.is_empty()) {
        y += &format!("   CarClassShortName: {c}\n");
    }
    y
}

/// Rejoue la course. `pit_loss_s` : temps perdu par arrêt connu avant la course (historique du circuit).
pub fn replay(r: &Recording, plan: EngineerPlan, pit_loss_s: Option<f32>) -> ReplayReport {
    let mut rep = ReplayReport::default();
    let t = &r.ticks;
    if t.len() < 100 || r.header.track_length_m <= 100.0 {
        rep.missing.push("Enregistrement trop court.".into());
        return rep;
    }
    let len = r.header.track_length_m as f64;
    let dist = unwrapped_distance(t, &r.anchors, len);
    let energy = t.iter().any(|k| k.energy > 0.0);
    rep.unit = Some(if energy { Unit::Energy } else { Unit::Litres });
    let level = |k: &crate::ingest::lmu::Tick| if energy { k.energy } else { k.fuel };
    if !r.laps.iter().any(|e| e.time_remain_s.is_some()) {
        rep.missing.push("Fin de chrono non enregistrée (avant Apex 0.38) : tours restants estimés sans le temps de séance.".into());
    }
    rep.missing.push("MFD du jeu non enregistré : rejoué sans.".into());

    let y = yaml(r);
    let (mut coach, _) = LiveCoach::new(&y, None, vec![], None);
    coach.set_engineer(plan, None, None);
    let top = crate::analysis::strategy::is_top_class(crate::model::SimId::Lmu, r.header.car_class.as_deref());
    coach.set_race_context(top, pit_loss_s);

    // Passages de ligne réels (distance déroulée) : vérité des tours restants.
    let mut crossings: Vec<(usize, i32)> = vec![];
    for i in 1..t.len() {
        if t[i].lap != t[i - 1].lap {
            crossings.push((i, t[i].lap));
        }
    }
    let finish_i = crossings.last().map(|c| c.0).unwrap_or(t.len() - 1);
    let laps_after = |i: usize| -> f32 { ((dist[finish_i] - dist[i]) / len).max(0.0) as f32 };

    // Arrêts réels (ravitaillement).
    struct Stop {
        entry: usize,
        exit: usize,
    }
    let mut stops: Vec<Stop> = vec![];
    let mut cur: Option<usize> = None;
    for i in 1..t.len() {
        match (t[i].pit, cur) {
            (true, None) => cur = Some(i),
            (false, Some(e)) => {
                if level(&t[i]) > level(&t[e]) + 2.0 && i > 0 {
                    stops.push(Stop { entry: e, exit: i });
                }
                cur = None;
            }
            _ => {}
        }
    }

    let ev_at = |et: f64| r.laps.iter().rev().find(|e| e.et <= et).or(r.laps.first());
    let mut entry_fill: Vec<(usize, f32, f32, f32)> = vec![]; // (tick, fill, laps_exact, per_lap)
    let mut was_pit = false;
    let mut prev_pct = 0.0f64;
    for (i, k) in t.iter().enumerate() {
        let ld = dist[i].rem_euclid(len);
        let ev = ev_at(k.et);
        let time_remain = ev.and_then(|e| e.time_remain_s.map(|tr| tr - (k.et - e.et))).map(|v| v.max(0.0)).unwrap_or(-1.0);
        let (lb, tb) = ev.filter(|e| e.et <= k.et).map(|e| (e.laps_behind_leader.max(0) as f64, e.time_behind_leader.max(0.0))).unwrap_or((0.0, 0.0));
        let mut row = [0.0f64; LIVE_N];
        row[0] = k.et;
        row[1] = ld;
        row[2] = ld / len;
        row[3] = k.speed as f64;
        row[4] = k.throttle as f64;
        row[5] = k.brake as f64;
        row[6] = k.lap as f64;
        row[7] = k.gear as f64;
        row[8] = k.rpm as f64;
        row[9] = k.steer_rad as f64;
        row[12] = k.fuel as f64;
        row[13] = k.pit as u8 as f64;
        row[14] = if k.off { 0.0 } else { 3.0 };
        row[15] = k.impacts as f64;
        row[16] = if k.yellow { crate::ingest::lmu::FLAG_YELLOW } else { 0.0 };
        row[17] = r.header.session as f64;
        row[18] = k.yaw as f64;
        row[19] = k.material as f64;
        row[20] = time_remain;
        row[21] = 32767.0;
        row[22] = k.energy as f64;
        row[23] = k.tl_steps as f64;
        row[25] = lb;
        row[26] = tb;
        let entering = k.pit && !was_pit;
        was_pit = k.pit;
        let msg = coach.push(row);
        // Passage de ligne vu par le coach (distance qui repart de zéro), pas par le compteur de tours,
        // qui change un échantillon plus tôt.
        let crossed = prev_pct > 0.9 && row[2] < 0.1;
        prev_pct = row[2];
        if let Some(m) = msg.filter(|m| m.kind == CoachMessageKind::Strategy) {
            rep.said.push((m.lap, m.text.clone()));
            if entering {
                if let Some(c) = coach.fuel_snapshot().and_then(|f| f.engineer) {
                    if let (Some(f), Some(_)) = (c.fill, c.laps_after_stop) {
                        // Distance exacte et conso retenues à cet instant (celles de `on_pit_entry`).
                        let x = coach_laps_exact(&coach).unwrap_or(f32::NAN);
                        // Consommation retenue pour ce plein (marge d'une demi-tour comprise dans `f`).
                        let per_used = f / (x + plan.margin_laps.max(0.0));
                        entry_fill.push((i, f, x, per_used));
                    }
                }
            }
        }
        if crossed {
            if let Some(c) = coach.fuel_snapshot().and_then(|f| f.engineer) {
                let left_true = crossings.iter().filter(|(j, _)| *j > i).count() as u32;
                rep.line_calls.push(LineCall { lap: rep.line_calls.len() as i32 + 1, laps_left: c.laps_left, laps_left_true: left_true.min(crossings.len() as u32), exact_est: coach_laps_exact(&coach), exact_true: laps_after(i), per_lap: c.per_lap, fill: c.fill, laps_after_stop: c.laps_after_stop });
            }
        }
    }
    rep.laps = crossings.len() as i32;
    let fin = t[finish_i.saturating_sub(1)..].iter().chain(t[crossings.iter().rev().nth(1).map(|c| c.0).unwrap_or(0)..finish_i].iter()).filter(|k| !k.pit).map(&level).fold(f32::MAX, f32::min);
    rep.finish_level = if fin == f32::MAX { 0.0 } else { fin };

    for s in &stops {
        let used_after = level(&t[s.exit]) - rep.finish_level;
        let laps_true = laps_after(s.entry);
        let per_true = if laps_true > 0.1 { used_after / laps_true } else { 0.0 };
        let needed = laps_true * per_true + 0.5 * per_true;
        let entry = entry_fill.iter().find(|e| e.0 >= s.entry && e.0 <= s.exit);
        let line = rep.line_calls.iter().rev().find(|c| crossings.iter().any(|(j, l)| *l == c.lap + 1 && *j <= s.entry)).and_then(|c| c.fill);
        let announced = entry.map(|e| e.1).or(line);
        let up = |v: f32| (v - 0.05).ceil();
        let spare = |set: f32| if per_true > 0.0 { (set - used_after) / per_true } else { 0.0 };
        let (laps_est, per_est) = entry.map(|e| (Some(e.2), Some(e.3))).unwrap_or((None, None));
        rep.stops.push(StopReview {
            lap: t[s.entry].lap,
            set: level(&t[s.exit]),
            announced: announced.map(up),
            announced_at_entry: entry.is_some(),
            laps_est,
            laps_true,
            per_lap_est: per_est,
            per_lap_true: per_true,
            used_after,
            needed,
            spare_laps_if_announced: announced.map(|a| spare(up(a))),
            spare_laps_actual: spare(level(&t[s.exit])),
            err_distance: laps_est.zip(per_est).map(|(x, p)| (x - laps_true) * p),
            err_consumption: per_est.map(|p| laps_true * (p - per_true) + 0.5 * (p - per_true)),
            err_rounding: announced.map(|a| up(a) - a),
        });
    }
    rep
}

fn coach_laps_exact(c: &LiveCoach) -> Option<f32> {
    c.race_laps_left_now().map(|x| x as f32)
}
