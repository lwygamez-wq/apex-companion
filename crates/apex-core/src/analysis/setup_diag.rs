//! Diagnostic setup d'une séance LMU (Setup Builder, étape 2) : ce que la voiture a fait, mesuré,
//! sans aucun conseil de réglage.
//!
//! Source : l'enregistrement `.apexlmu` d'origine (pneus et freins à 5 Hz, hauteurs, accélérations et
//! rotation des roues à 60 Hz, setup lu dans l'API du jeu). Tours retenus comme pour la stratégie :
//! tours lancés chronométrés, hors stand, sortie, incident, jaune et coupure (pas sur `clean` : un tour
//! invalidé par les limites de piste reste une mesure valable de la voiture).
//!
//! Règles :
//! - chaque grandeur donne sa valeur, son unité, le nombre de tours utilisés et sa dispersion d'un tour
//!   à l'autre (écart entre le 9e et le 1er décile des médianes par tour) ;
//! - un constat n'est écrit que si l'écart est net (seuil) ET répété (même sens sur 80 % des tours) ;
//!   sinon l'app dit « pas d'écart mesurable » ;
//! - températures affichées telles que lues, jamais comparées à l'idéal du jeu : la calibration du
//!   20 nov. 2026 n'établit pas laquelle des mesures (bande de roulement ou carcasse) le jeu compare à
//!   son idéal (`docs/calibration-setup.md`) ;
//! - hauteur : pas de « talonnage » (aucun signal de contact du fond plat), seulement les points les
//!   plus bas et où ils se trouvent.

use crate::ingest::lmu::{unwrapped_distance, Recording, SetupValue, Tick};
use crate::model::{LapKind, LapMeta};
use serde::Serialize;
use std::collections::BTreeMap;

pub const WHEELS: [&str; 4] = ["Avant gauche", "Avant droit", "Arrière gauche", "Arrière droit"];

/// Une mesure : valeur médiane, unité, tours utilisés, dispersion d'un tour à l'autre.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Stat {
    pub value: f32,
    pub unit: &'static str,
    pub laps: usize,
    /// Écart entre le 9e et le 1er décile des valeurs par tour (même unité).
    pub spread: f32,
}

/// Un constat chiffré (jamais un conseil).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Finding {
    pub text: String,
    pub value: f32,
    pub unit: &'static str,
    pub laps: usize,
    pub spread: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TyreDiag {
    pub wheel: &'static str,
    /// Bande de roulement, en appui : extérieur, milieu, intérieur (°C, telles que lues).
    pub outer: Option<Stat>,
    pub middle: Option<Stat>,
    pub inner: Option<Stat>,
    /// Intérieur moins extérieur, et milieu moins moyenne des bords (°C), en appui.
    pub inner_minus_outer: Option<Stat>,
    pub middle_minus_edges: Option<Stat>,
    pub carcass: Option<Stat>,
    /// Couche interne (fichiers 0.41 et plus).
    pub inner_layer: Option<Stat>,
    /// Température à l'échelle du HUD du jeu, ESTIMÉE : (carcasse + couche interne) / 2, en appui
    /// comme les autres colonnes (fichiers 0.41 et plus). Formule tirée de 2 relevés du HUD : jamais comparée à l'idéal.
    pub game_scale: Option<Stat>,
    /// Pression à chaud (kPa), en piste.
    pub pressure_hot: Option<Stat>,
    /// Pression réglée au garage (valeur affichée par le jeu).
    pub pressure_set: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BrakeDiag {
    pub wheel: &'static str,
    pub median: Option<Stat>,
    /// Maximum de chaque tour, médiane sur les tours.
    pub peak: Option<Stat>,
}

/// Blocages répétés au même virage (fichiers 0.41 et plus).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LockZone {
    pub corner: String,
    pub wheel: &'static str,
    pub events: usize,
    pub laps: usize,
    /// Glissement maximal médian pendant les blocages (%).
    pub slip_pct: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HeightDiag {
    pub wheel: &'static str,
    pub median: Option<Stat>,
    /// Point bas robuste : 1er centile de chaque tour, médiane sur les tours (mm).
    pub low: Option<Stat>,
    /// Pointes isolées sous 5 mm (échantillons) : comptées, pas interprétées.
    pub spikes_under_5mm: usize,
    /// Endroit où se concentrent les points bas, s'il y en a un.
    pub low_zone: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BalancePhase {
    Entree,
    Milieu,
    Sortie,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BalanceTrend {
    SousVire,
    /// Rapport proche de 1, ou trop variable d'un tour à l'autre : pas d'écart mesurable.
    PasDEcart,
    Survire,
}

/// Équilibre d'un virage dans une phase : rotation de la voiture pour un même angle de volant,
/// comparée à celle des virages pris en douceur à la même vitesse (1,00 = pareil).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BalanceZone {
    pub corner: String,
    pub phase: BalancePhase,
    pub trend: BalanceTrend,
    /// Rapport de rotation (sans unité) ; < 1 = la voiture tourne moins (sous-virage).
    pub ratio: Stat,
    /// Part des échantillons en contre-braquage (%).
    pub countersteer_pct: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SetupRow {
    pub key: String,
    pub label: String,
    pub group: String,
    /// Cran actuel, crans mini et maxi, valeur affichée par le jeu pour le cran actuel.
    pub step: i32,
    pub min: i32,
    pub max: i32,
    pub display: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct SetupDiagnosis {
    pub car: String,
    pub track: String,
    pub laps_used: usize,
    pub tyres: Vec<TyreDiag>,
    pub tyre_findings: Vec<Finding>,
    pub brakes: Vec<BrakeDiag>,
    pub locks: Vec<LockZone>,
    pub heights: Vec<HeightDiag>,
    pub height_findings: Vec<Finding>,
    pub balance: Vec<BalanceZone>,
    pub setup: Vec<SetupRow>,
    /// Ce qui n'a pas pu être mesuré, et pourquoi.
    pub missing: Vec<String>,
    /// Comment lire les températures (nature de la mesure non établie).
    pub temperature_note: String,
    /// Comment lire la température à l'échelle du jeu (estimation à confirmer).
    pub game_scale_note: String,
}

/// Fenêtre d'un virage (distances depuis la ligne, en m), relevée sur le tour de référence.
#[derive(Debug, Clone, PartialEq)]
pub struct DiagCorner {
    pub name: String,
    pub entry_m: f32,
    pub apex_m: f32,
    pub throttle_m: f32,
    pub exit_m: f32,
}

/// Tour utilisable pour le diagnostic : mêmes filtres que la stratégie.
pub fn usable(l: &LapMeta) -> bool {
    l.kind == LapKind::Flying && l.lap_time_s.is_some() && !l.flags.is_disturbed()
}

pub const TEMPERATURE_NOTE: &str = "Températures telles que lues dans le jeu, pas comparées à l'idéal annoncé : on ne sait pas encore laquelle de ces mesures le jeu compare à son idéal.";
pub const GAME_SCALE_NOTE: &str = "Échelle du jeu : estimation d'après 2 relevés du HUD, à confirmer. Pas comparée à l'idéal de 92 °C tant qu'un 3e relevé n'a pas confirmé la formule.";

/// Température du pneu à l'échelle du HUD du jeu, ESTIMÉE : moyenne de la carcasse et de la couche
/// interne (moyenne de ses 3 zones). Relevés du HUD à Monza le 20 nov. 2026 (Cadillac, course et fin
/// d'essais) : 0,1 à 1,4 °C d'écart. Hypothèse à confirmer (`docs/calibration-setup.md`).
pub fn game_scale_c(carcass_c: f32, inner_c: [f32; 3]) -> f32 {
    (carcass_c + inner_c.iter().sum::<f32>() / 3.0) / 2.0
}

/// Seuils : écart de température net (°C), part des tours dans le même sens, tours minimum.
const TEMP_DELTA_C: f32 = 3.0;
const SAME_SIGN: f32 = 0.8;
const MIN_LAPS: usize = 3;
/// Échantillon « en appui » : accélération latérale (m/s²).
const LOADED: f32 = 12.0;

fn pct(v: &mut [f32], p: f32) -> f32 {
    v.sort_by(f32::total_cmp);
    v[((v.len() - 1) as f32 * p).round() as usize]
}

/// Médiane des valeurs par tour, et dispersion entre tours (9e − 1er décile).
fn stat(per_lap: &BTreeMap<i32, Vec<f32>>, unit: &'static str) -> Option<Stat> {
    let mut meds: Vec<f32> = per_lap.values().filter(|v| !v.is_empty()).map(|v| pct(&mut v.clone(), 0.5)).collect();
    if meds.len() < MIN_LAPS {
        return None;
    }
    let value = pct(&mut meds, 0.5);
    let spread = pct(&mut meds, 0.9) - pct(&mut meds, 0.1);
    Some(Stat { value, unit, laps: meds.len(), spread })
}

/// Médianes par tour : part des tours dont le signe est celui de la médiane globale.
fn same_sign(per_lap: &BTreeMap<i32, Vec<f32>>, s: &Stat) -> bool {
    let meds: Vec<f32> = per_lap.values().filter(|v| !v.is_empty()).map(|v| pct(&mut v.clone(), 0.5)).collect();
    let n = meds.iter().filter(|m| m.signum() == s.value.signum() && **m != 0.0).count();
    n as f32 >= SAME_SIGN * meds.len() as f32
}

fn fr1(v: f32) -> String {
    format!("{v:.1}").replace('.', ",")
}

/// Accélération latérale : mesurée (0.41+), sinon vitesse × lacet.
fn a_lat(t: &Tick) -> f32 {
    if t.accel_lat != 0.0 { t.accel_lat.abs() } else { (t.speed * t.yaw).abs() }
}

fn corner_at(corners: &[DiagCorner], d: f32) -> Option<&DiagCorner> {
    corners.iter().find(|c| d >= c.entry_m - 150.0 && d <= c.exit_m)
}

pub fn setup_diagnosis(rec: &Recording, laps: &[LapMeta], corners: &[DiagCorner]) -> SetupDiagnosis {
    let h = &rec.header;
    let keep: Vec<i32> = laps.iter().filter(|l| usable(l)).map(|l| l.number).collect();
    let mut d = SetupDiagnosis { car: h.car.clone(), track: h.track.clone(), laps_used: keep.len(), temperature_note: TEMPERATURE_NOTE.into(), game_scale_note: GAME_SCALE_NOTE.into(), ..Default::default() };
    if keep.len() < MIN_LAPS {
        d.missing.push(format!("Seulement {} tour(s) utilisable(s) : il en faut {MIN_LAPS} (hors stand, sortie, incident, jaune).", keep.len()));
    }
    d.setup = setup_rows(rec.setup.as_ref().map(|s| s.values.as_slice()).unwrap_or(&[]));
    if rec.setup.is_none() {
        d.missing.push("Setup de la séance : non enregistré (jeu fermé au départ, ou fichier d'avant 0.36).".into());
    }
    let len = h.track_length_m as f64;
    let dist: Vec<f32> = unwrapped_distance(&rec.ticks, &rec.anchors, len).iter().map(|x| if len > 0.0 { x.rem_euclid(len) as f32 } else { *x as f32 }).collect();
    let on = |t: &Tick| keep.contains(&t.lap) && !t.pit && t.speed > 20.0;

    // --- Pneus et freins (5 Hz) ------------------------------------------------------------------
    if rec.wheels.is_empty() {
        d.missing.push("Pneus et freins : non enregistrés sur cette séance (fichier d'avant 0.36).".into());
    } else {
        let tick_of = |et: f64| -> Option<&Tick> {
            let i = rec.ticks.partition_point(|t| t.et < et);
            rec.ticks.get(i.min(rec.ticks.len().saturating_sub(1)))
        };
        let samples: Vec<(&crate::ingest::lmu::WheelSample, &Tick)> = rec.wheels.iter().filter_map(|w| tick_of(w.et).map(|t| (w, t))).filter(|(_, t)| on(t)).collect();
        let set_pressure = |k: usize| rec.setup.as_ref().and_then(|s| s.values.iter().find(|v| v.key == format!("WM_PRESSURE-W_{}", ["FL", "FR", "RL", "RR"][k])).map(|v| v.display.clone()));
        let has_inner = samples.iter().any(|(w, _)| w.inner_c.iter().flatten().any(|v| *v != 0.0));
        for (k, &wheel) in WHEELS.iter().enumerate() {
            let left = k % 2 == 0;
            let mut m: [BTreeMap<i32, Vec<f32>>; 10] = Default::default();
            for (w, t) in &samples {
                let z = w.temp_c[k];
                let (outer, inner) = if left { (z[0], z[2]) } else { (z[2], z[0]) };
                m[7].entry(t.lap).or_default().push(w.pressure_kpa[k]);
                m[8].entry(t.lap).or_default().push(w.brake_c[k]);
                if a_lat(t) < LOADED || z.iter().any(|v| *v <= 0.0) {
                    continue;
                }
                m[0].entry(t.lap).or_default().push(outer);
                m[1].entry(t.lap).or_default().push(z[1]);
                m[2].entry(t.lap).or_default().push(inner);
                m[3].entry(t.lap).or_default().push(inner - outer);
                m[4].entry(t.lap).or_default().push(z[1] - (outer + inner) / 2.0);
                m[5].entry(t.lap).or_default().push(w.carcass_c[k]);
                if has_inner {
                    m[6].entry(t.lap).or_default().push(w.inner_c[k].iter().sum::<f32>() / 3.0);
                    if w.carcass_c[k] > 0.0 && w.inner_c[k].iter().all(|v| *v > 0.0) {
                        m[9].entry(t.lap).or_default().push(game_scale_c(w.carcass_c[k], w.inner_c[k]));
                    }
                }
            }
            let peaks: BTreeMap<i32, Vec<f32>> = m[8].iter().map(|(l, v)| (*l, vec![v.iter().copied().fold(0.0, f32::max)])).collect();
            let io = stat(&m[3], "°C");
            let me = stat(&m[4], "°C");
            if let Some(s) = io.as_ref().filter(|s| s.value.abs() >= TEMP_DELTA_C && same_sign(&m[3], s)) {
                let (a, b) = if s.value > 0.0 { ("intérieur", "l'extérieur") } else { ("extérieur", "l'intérieur") };
                d.tyre_findings.push(Finding { text: format!("{} : {a} plus chaud que {b} de {} °C en appui.", wheel, fr1(s.value.abs())), value: s.value, unit: "°C", laps: s.laps, spread: s.spread });
            }
            if let Some(s) = me.as_ref().filter(|s| s.value.abs() >= TEMP_DELTA_C && same_sign(&m[4], s)) {
                let txt = if s.value > 0.0 { "milieu plus chaud que les bords" } else { "bords plus chauds que le milieu" };
                d.tyre_findings.push(Finding { text: format!("{} : {txt} de {} °C en appui.", wheel, fr1(s.value.abs())), value: s.value, unit: "°C", laps: s.laps, spread: s.spread });
            }
            d.tyres.push(TyreDiag {
                wheel,
                outer: stat(&m[0], "°C"),
                middle: stat(&m[1], "°C"),
                inner: stat(&m[2], "°C"),
                inner_minus_outer: io,
                middle_minus_edges: me,
                carcass: stat(&m[5], "°C"),
                inner_layer: stat(&m[6], "°C"),
                game_scale: stat(&m[9], "°C"),
                pressure_hot: stat(&m[7], "kPa"),
                pressure_set: set_pressure(k),
            });
            d.brakes.push(BrakeDiag { wheel, median: stat(&m[8], "°C"), peak: stat(&peaks, "°C") });
        }
        if d.tyre_findings.is_empty() && d.tyres.iter().any(|t| t.inner_minus_outer.is_some()) {
            d.tyre_findings.push(Finding { text: "Pneus : pas d'écart mesurable entre intérieur, milieu et extérieur.".into(), value: 0.0, unit: "°C", laps: d.tyres.iter().filter_map(|t| t.inner_minus_outer.as_ref()).map(|s| s.laps).max().unwrap_or(0), spread: 0.0 });
        }
        if !has_inner {
            d.missing.push("Couche interne des pneus, et donc température à l'échelle du jeu : enregistrées à partir d'Apex 0.41.".into());
        }
    }

    // --- Blocages (60 Hz, 0.41+) -----------------------------------------------------------------
    let has_rot = rec.ticks.iter().any(|t| t.wheel_rot != [0.0; 4]);
    if has_rot {
        d.locks = locks(rec, &dist, corners, &on);
    } else {
        d.missing.push("Blocages de roue : rotation des roues enregistrée à partir d'Apex 0.41.".into());
    }

    // --- Hauteurs (60 Hz, 0.36+) -----------------------------------------------------------------
    if rec.ticks.iter().any(|t| t.ride_h_m != [0.0; 4]) {
        for (k, &wheel) in WHEELS.iter().enumerate() {
            let mut per: BTreeMap<i32, Vec<f32>> = BTreeMap::new();
            let mut spikes = 0;
            for t in rec.ticks.iter().filter(|t| on(t)) {
                let mm = t.ride_h_m[k] * 1000.0;
                if mm < 5.0 {
                    spikes += 1;
                    continue;
                }
                per.entry(t.lap).or_default().push(mm);
            }
            let p1: BTreeMap<i32, Vec<f32>> = per.iter().filter(|(_, v)| v.len() > 100).map(|(l, v)| (*l, vec![pct(&mut v.clone(), 0.01)])).collect();
            let low = stat(&p1, "mm");
            // Où se trouvent les points bas : échantillons sous le point bas robuste, par virage.
            let low_zone = low.as_ref().and_then(|s| {
                let mut by: BTreeMap<String, (usize, std::collections::BTreeSet<i32>)> = BTreeMap::new();
                let mut total = 0usize;
                for (i, t) in rec.ticks.iter().enumerate().filter(|(_, t)| on(t)) {
                    let mm = t.ride_h_m[k] * 1000.0;
                    if (5.0..=s.value).contains(&mm) {
                        total += 1;
                        let name = corner_at(corners, dist[i]).map(|c| c.name.clone()).unwrap_or_else(|| "en ligne droite".into());
                        let e = by.entry(name).or_default();
                        e.0 += 1;
                        e.1.insert(t.lap);
                    }
                }
                let (name, (n, l)) = by.into_iter().max_by_key(|(_, (n, _))| *n)?;
                (total > 0 && n * 3 >= total && l.len() >= MIN_LAPS).then(|| format!("{name} ({} tours sur {})", l.len(), s.laps))
            });
            if let (Some(s), Some(z)) = (&low, &low_zone) {
                let at = if z.starts_with("en ") { z.clone() } else { format!("à {z}") };
                d.height_findings.push(Finding { text: format!("{} : points les plus bas {at}, {} mm.", wheel, fr1(s.value)), value: s.value, unit: "mm", laps: s.laps, spread: s.spread });
            }
            d.heights.push(HeightDiag { wheel, median: stat(&per, "mm"), low, spikes_under_5mm: spikes, low_zone });
        }
    } else {
        d.missing.push("Hauteurs de caisse : non enregistrées sur cette séance (fichier d'avant 0.36).".into());
    }

    // --- Équilibre ---------------------------------------------------------------------------------
    d.balance = balance(rec, &dist, corners, &on);
    if d.balance.is_empty() {
        d.missing.push("Équilibre : pas assez de passages en appui avec le volant tourné pour le mesurer.".into());
    }
    d
}

/// Blocages : glissement de la roue sous le freinage (rotation × rayon contre vitesse du sol). Le
/// rayon de chaque roue est mesuré en roue libre sur la séance.
fn locks(rec: &Recording, dist: &[f32], corners: &[DiagCorner], on: &dyn Fn(&Tick) -> bool) -> Vec<LockZone> {
    let mut out = vec![];
    for (k, &wheel) in WHEELS.iter().enumerate() {
        let Some(radius) = wheel_radius(rec, k, on) else { continue };
        // Événement : au moins 6 échantillons (0,1 s à 60 Hz) à plus de 20 % de glissement.
        let mut by: BTreeMap<String, (usize, std::collections::BTreeSet<i32>, Vec<f32>)> = BTreeMap::new();
        let (mut run, mut worst) = (0usize, 0.0f32);
        for (i, t) in rec.ticks.iter().enumerate() {
            let slip = if on(t) { brake_slip(t, k, radius) } else { 0.0 };
            if slip > 0.2 {
                run += 1;
                worst = worst.max(slip);
                continue;
            }
            if run >= 6 {
                let name = corner_at(corners, dist[i]).map(|c| c.name.clone()).unwrap_or_else(|| "hors virage".into());
                let e = by.entry(name).or_default();
                e.0 += 1;
                e.1.insert(t.lap);
                e.2.push(worst * 100.0);
            }
            run = 0;
            worst = 0.0;
        }
        for (corner, (events, laps, mut slips)) in by {
            if laps.len() >= 2 {
                out.push(LockZone { corner, wheel, events, laps: laps.len(), slip_pct: pct(&mut slips, 0.5) });
            }
        }
    }
    out.sort_by(|a, b| b.laps.cmp(&a.laps).then(b.events.cmp(&a.events)));
    out
}

/// Rayon de roulement d'une roue (m), mesuré en roue libre (gaz, sans frein, en ligne droite) : vitesse
/// du sol sous la roue / rotation. `None` sans rotation enregistrée (fichiers d'avant 0.41).
pub fn wheel_radius(rec: &Recording, k: usize, on: &dyn Fn(&Tick) -> bool) -> Option<f32> {
    let mut r: Vec<f32> = rec
        .ticks
        .iter()
        .filter(|t| on(t) && t.brake < 0.02 && t.throttle > 0.3 && t.wheel_rot[k].abs() > 10.0 && t.wheel_vlong[k].abs() > 10.0 && a_lat(t) < 3.0)
        .map(|t| t.wheel_vlong[k].abs() / t.wheel_rot[k].abs())
        .collect();
    (r.len() >= 200).then(|| pct(&mut r, 0.5))
}

/// Glissement au freinage (0 = roule, 1 = bloquée), avec le rayon mesuré ; 0 hors freinage.
pub fn brake_slip(t: &Tick, k: usize, radius: f32) -> f32 {
    if t.brake > 0.2 && t.wheel_vlong[k].abs() > 5.0 {
        1.0 - t.wheel_rot[k].abs() * radius / t.wheel_vlong[k].abs()
    } else {
        0.0
    }
}

/// Équilibre par virage et par phase : rotation pour un angle de volant donné (lacet / (vitesse ×
/// volant)), rapportée à la même grandeur dans les passages en douceur (2 à 6 m/s²) à la même
/// vitesse. Le signe volant/lacet est calé sur la séance (convention du jeu inconnue à l'avance).
fn balance(rec: &Recording, dist: &[f32], corners: &[DiagCorner], on: &dyn Fn(&Tick) -> bool) -> Vec<BalanceZone> {
    let usable = |t: &Tick| on(t) && t.steer_rad.abs() > 0.05;
    let sign = {
        let s: f32 = rec.ticks.iter().filter(|t| usable(t) && a_lat(t) > 6.0).map(|t| (t.yaw * t.steer_rad).signum()).sum();
        if s >= 0.0 { 1.0 } else { -1.0 }
    };
    let gain = |t: &Tick| sign * t.yaw / (t.speed * t.steer_rad);
    let band = |t: &Tick| (t.speed * 3.6 / 50.0) as usize;
    // Référence par tranche de 50 km/h.
    let mut base: BTreeMap<usize, Vec<f32>> = BTreeMap::new();
    for t in rec.ticks.iter().filter(|t| usable(t) && (2.0..6.0).contains(&a_lat(t))) {
        base.entry(band(t)).or_default().push(gain(t));
    }
    let base: BTreeMap<usize, f32> = base.into_iter().filter(|(_, v)| v.len() >= 60).map(|(b, mut v)| (b, pct(&mut v, 0.5))).filter(|(_, g)| *g > 0.0).collect();
    // (virage, phase) → (rapports par tour, échantillons, échantillons en contre-braquage).
    type Acc = (BTreeMap<i32, Vec<f32>>, usize, usize);
    let mut acc: BTreeMap<(usize, u8), Acc> = BTreeMap::new();
    for (i, t) in rec.ticks.iter().enumerate() {
        if !usable(t) || a_lat(t) < 8.0 {
            continue;
        }
        let Some(b) = base.get(&band(t)) else { continue };
        let x = dist[i];
        let Some(ci) = corners.iter().position(|c| x >= c.entry_m && x <= c.exit_m) else { continue };
        let c = &corners[ci];
        let ph = if x < c.apex_m - 10.0 { 0 } else if x < c.throttle_m.max(c.apex_m + 10.0) { 1 } else { 2 };
        let e = acc.entry((ci, ph)).or_default();
        let g = gain(t);
        e.1 += 1;
        if g < 0.0 {
            e.2 += 1;
        }
        e.0.entry(t.lap).or_default().push(g / b);
    }
    let mut out = vec![];
    for ((ci, ph), (per, n, counter)) in acc {
        if n < 30 {
            continue;
        }
        let Some(ratio) = stat(&per, "") else { continue };
        let trend = if ratio.value < 0.85 && same_sign(&per.iter().map(|(l, v)| (*l, v.iter().map(|g| g - 0.85).collect())).collect(), &Stat { value: -1.0, ..ratio.clone() }) {
            BalanceTrend::SousVire
        } else if ratio.value > 1.15 && same_sign(&per.iter().map(|(l, v)| (*l, v.iter().map(|g| g - 1.15).collect())).collect(), &Stat { value: 1.0, ..ratio.clone() }) {
            BalanceTrend::Survire
        } else {
            BalanceTrend::PasDEcart
        };
        out.push(BalanceZone {
            corner: corners[ci].name.clone(),
            phase: [BalancePhase::Entree, BalancePhase::Milieu, BalancePhase::Sortie][ph as usize],
            trend,
            ratio,
            countersteer_pct: counter as f32 / n as f32 * 100.0,
        });
    }
    out
}

/// Nom français et groupe d'un réglage de l'API du jeu (« WM_CAMBER-W_FL » → « Carrossage AvG »).
pub fn api_label(key: &str) -> (String, String) {
    let (base, wheel) = match key.split_once("-W_") {
        Some((b, w)) => (b, Some(match w { "FL" => "AvG", "FR" => "AvD", "RL" => "ArG", "RR" => "ArD", _ => w })),
        None => (key, None),
    };
    let (name, group) = match base {
        "WM_CAMBER" => ("Carrossage", "Pneus"),
        "WM_PRESSURE" => ("Pression à froid", "Pneus"),
        "WM_RIDEHEIGHT" => ("Hauteur de caisse", "Suspension"),
        "WM_SPRING" => ("Ressort", "Suspension"),
        "WM_PACKER" => ("Butée (packer)", "Suspension"),
        "WM_SLOWBUMP" => ("Compression lente", "Amortisseurs"),
        "WM_FASTBUMP" => ("Compression rapide", "Amortisseurs"),
        "WM_SLOWREBOUND" => ("Détente lente", "Amortisseurs"),
        "WM_FASTREBOUND" => ("Détente rapide", "Amortisseurs"),
        "WM_BRAKEDISC" => ("Disque de frein", "Freins"),
        "WM_BRAKEPAD" => ("Plaquettes", "Freins"),
        "VM_BRAKE_BALANCE" => ("Répartition de freinage (AV:AR)", "Freins"),
        "VM_BRAKE_PRESSURE" => ("Pression de freinage", "Freins"),
        "VM_BRAKE_MIGRATION" => ("Migration de freinage", "Freins"),
        "VM_BRAKE_DUCTS" => ("Écopes de frein AV", "Freins"),
        "VM_BRAKE_DUCTS_REAR" => ("Écopes de frein AR", "Freins"),
        "VM_FRONT_WING" => ("Aileron avant", "Aéro"),
        "VM_REAR_WING" => ("Aileron arrière", "Aéro"),
        "VM_FRONT_TOEIN" => ("Pincement AV", "Suspension"),
        "VM_REAR_TOEIN" => ("Pincement AR", "Suspension"),
        "VM_FRONT_ANTISWAY" | "VM_FRONT_ANTISWAYBAR" => ("Barre antiroulis AV", "Suspension"),
        "VM_REAR_ANTISWAY" | "VM_REAR_ANTISWAYBAR" => ("Barre antiroulis AR", "Suspension"),
        "VM_FRONT_3RD_SPRING" => ("Ressort 3e élément AV", "Suspension"),
        "VM_REAR_3RD_SPRING" => ("Ressort 3e élément AR", "Suspension"),
        "VM_DIFF_POWER" => ("Différentiel en accélération", "Différentiel"),
        "VM_DIFF_COAST" => ("Différentiel au lâcher", "Différentiel"),
        "VM_DIFF_PRELOAD" => ("Précharge du différentiel", "Différentiel"),
        "VM_TRACTIONCONTROLMAP" => ("Antipatinage (TC)", "Aides"),
        "VM_ANTILOCKBRAKESYSTEMMAP" => ("ABS", "Aides"),
        _ => {
            let raw = base.trim_start_matches("VM_").trim_start_matches("WM_").replace('_', " ").to_lowercase();
            return (wheel.map_or(raw.clone(), |w| format!("{raw} {w}")), "Autres".into());
        }
    };
    (wheel.map_or(name.to_string(), |w| format!("{name} {w}")), group.into())
}

/// Réglages réglables de la séance (au moins deux crans, valeur affichée connue), groupés.
pub fn setup_rows(values: &[SetupValue]) -> Vec<SetupRow> {
    let mut rows: Vec<SetupRow> = values
        .iter()
        .filter(|v| v.max > v.min && v.display != "N/A" && !v.display.is_empty())
        .map(|v| {
            let (label, group) = api_label(&v.key);
            SetupRow { key: v.key.clone(), label, group, step: v.value, min: v.min, max: v.max, display: v.display.clone() }
        })
        .collect();
    rows.sort_by(|a, b| a.group.cmp(&b.group).then(a.label.cmp(&b.label)));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::lmu::{Anchor, Header, SetupSnapshot, WheelSample};
    use crate::model::{LapFlags, Validity, ValiditySource};

    // Fixtures SYNTHÉTIQUES (pas une séance réelle) : circuit de 1000 m, 60 Hz, 50 m/s, 5 tours de 20 s.
    const LEN: f32 = 1000.0;

    fn lap_meta(n: i32, pit: bool) -> LapMeta {
        LapMeta {
            number: n,
            kind: LapKind::Flying,
            lap_time_s: Some(20.0),
            sector_times_s: vec![],
            flags: LapFlags { pit, ..Default::default() },
            validity: Validity::Valid,
            validity_source: ValiditySource::SimReported,
            fuel_start_l: None,
            fuel_used_l: None,
            energy_used_pct: None,
            tyre_wear_pct: None,
        }
    }

    /// Un virage à gauche de 400 à 600 m (apex 500) ; `f` règle chaque échantillon.
    fn recording(f: &dyn Fn(&mut Tick, f32)) -> Recording {
        let mut ticks = vec![];
        let mut wheels = vec![];
        for i in 0..(5 * 20 * 60) {
            let et = i as f64 / 60.0;
            let x = (et * 50.0) as f32;
            let mut t = Tick { et, lap: (x / LEN) as i32 + 1, speed: 50.0, throttle: 1.0, ..Default::default() };
            let d = x % LEN;
            t.ride_h_m = [0.030, 0.030, 0.035, 0.035];
            if (400.0..600.0).contains(&d) {
                t.accel_lat = 20.0;
                t.steer_rad = 0.2;
                t.yaw = 20.0 / 50.0;
            } else if (100.0..300.0).contains(&d) {
                // Courbe douce : référence de l'équilibre (4,8 m/s²), lacet / (v × volant) = 0,032.
                t.accel_lat = 4.8;
                t.steer_rad = 0.06;
                t.yaw = 0.096;
            }
            f(&mut t, d);
            if i % 12 == 0 {
                let loaded = (400.0..600.0).contains(&d);
                // Roue gauche : gauche = extérieur. Intérieur 6 °C plus chaud en appui.
                let z = if loaded { [60.0, 63.0, 66.0] } else { [50.0, 50.0, 50.0] };
                wheels.push(WheelSample { et, temp_c: [z, [66.0, 63.0, 60.0], z, [66.0, 63.0, 60.0]], carcass_c: [75.0; 4], pressure_kpa: [155.0; 4], brake_c: [400.0, 400.0, 300.0, 300.0], inner_c: [[0.0; 3]; 4] });
            }
            ticks.push(t);
        }
        Recording {
            header: Header { track: "Synthétique".into(), car: "Proto".into(), track_length_m: LEN, hz: 60.0, ..Default::default() },
            anchors: (0..100).map(|k| Anchor { et: k as f64, lap_dist: ((k as f32 * 50.0) % LEN), total_laps: (k as f32 * 50.0 / LEN) as i32 }).collect(),
            ticks,
            laps: vec![],
            wheels,
            setup: Some(SetupSnapshot {
                values: vec![
                    SetupValue { key: "WM_CAMBER-W_FL".into(), value: 30, display: "-1.0 deg".into(), min: 0, max: 31 },
                    SetupValue { key: "WM_PRESSURE-W_FL".into(), value: 0, display: "131 kPa".into(), min: 0, max: 36 },
                    SetupValue { key: "VM_ANTILOCK_BRAKES".into(), value: 0, display: "N/A".into(), min: 0, max: 1 },
                ],
                ..Default::default()
            }),
        }
    }

    fn corners() -> Vec<DiagCorner> {
        vec![DiagCorner { name: "V1".into(), entry_m: 400.0, apex_m: 500.0, throttle_m: 520.0, exit_m: 600.0 }]
    }

    #[test]
    fn tyres_read_inner_outer_by_wheel_side_and_never_compare_to_the_ideal() {
        let laps: Vec<LapMeta> = (1..=5).map(|n| lap_meta(n, n == 5)).collect();
        let d = setup_diagnosis(&recording(&|_, _| {}), &laps, &corners());
        assert_eq!(d.laps_used, 4, "tour au stand écarté");
        let fl = &d.tyres[0];
        assert_eq!((fl.outer.as_ref().unwrap().value, fl.inner.as_ref().unwrap().value), (60.0, 66.0));
        assert_eq!(fl.inner_minus_outer.as_ref().unwrap().value, 6.0);
        assert_eq!(fl.inner_minus_outer.as_ref().unwrap().laps, 4);
        // Roue droite : gauche = intérieur. Mêmes chiffres, même constat.
        assert_eq!(d.tyres[1].inner_minus_outer.as_ref().unwrap().value, 6.0);
        assert_eq!(fl.pressure_set.as_deref(), Some("131 kPa"));
        assert_eq!(fl.pressure_hot.as_ref().unwrap().value, 155.0);
        assert_eq!(d.tyre_findings[0].text, "Avant gauche : intérieur plus chaud que l'extérieur de 6,0 °C en appui.");
        assert_eq!(d.tyre_findings.len(), 4, "milieu = moyenne des bords : pas de constat");
        assert!(d.temperature_note.contains("pas comparées à l'idéal"));
        assert!(d.missing.iter().any(|m| m.contains("Couche interne")));
        // Freins : médiane par roue.
        assert_eq!(d.brakes[0].median.as_ref().unwrap().value, 400.0);
        assert_eq!(d.brakes[2].peak.as_ref().unwrap().value, 300.0);
    }

    /// Relevé RÉEL du HUD (Monza, Cadillac, course de 20 h 09, 20 nov. 2026, 3330,4 s) recopié en
    /// fixture : carcasse et couche interne lues par Apex au même instant, température affichée par le jeu.
    #[test]
    fn game_scale_estimate_stays_within_1_5_c_of_the_hud() {
        let carcass = [77.6, 68.3, 80.2, 74.5];
        let inner_mean = [61.6, 56.1, 63.2, 59.8];
        let hud = [68.8, 62.1, 71.0, 66.7];
        for k in 0..4 {
            let est = game_scale_c(carcass[k], [inner_mean[k]; 3]);
            assert!((est - hud[k]).abs() <= 1.5, "roue {k} : {est} contre {}", hud[k]);
        }
        assert!((game_scale_c(77.6, [60.0, 62.0, 62.8]) - 69.6).abs() < 1e-4);
    }

    #[test]
    fn game_scale_needs_the_inner_layer_and_is_never_compared_to_the_ideal() {
        let laps: Vec<LapMeta> = (1..=5).map(|n| lap_meta(n, false)).collect();
        let old = setup_diagnosis(&recording(&|_, _| {}), &laps, &corners());
        assert!(old.tyres.iter().all(|t| t.game_scale.is_none()));
        assert!(old.missing.iter().any(|m| m.contains("échelle du jeu")));
        let mut r = recording(&|_, _| {});
        for w in &mut r.wheels {
            w.carcass_c = [77.6, 68.3, 80.2, 74.5];
            w.inner_c = [[61.6; 3], [56.1; 3], [63.2; 3], [59.8; 3]];
        }
        let d = setup_diagnosis(&r, &laps, &corners());
        let g: Vec<f32> = d.tyres.iter().map(|t| (t.game_scale.as_ref().unwrap().value * 10.0).round() / 10.0).collect();
        assert_eq!(g, vec![69.6, 62.2, 71.7, 67.2]);
        assert_eq!(d.tyres[0].game_scale.as_ref().unwrap().laps, 5);
        assert!(!d.missing.iter().any(|m| m.contains("Couche interne")));
        // Aucun constat ne parle de l'idéal ni de 92 °C.
        assert!(d.tyre_findings.iter().all(|f| !f.text.contains("92") && !f.text.contains("idéal")));
        assert!(GAME_SCALE_NOTE.contains("à confirmer") && GAME_SCALE_NOTE.contains("Pas comparée"));
    }

    #[test]
    fn no_difference_means_no_finding_sentence() {
        let laps: Vec<LapMeta> = (1..=5).map(|n| lap_meta(n, false)).collect();
        let mut r = recording(&|_, _| {});
        for w in &mut r.wheels {
            w.temp_c = [[62.0, 62.5, 61.5]; 4];
        }
        let d = setup_diagnosis(&r, &laps, &corners());
        assert_eq!(d.tyre_findings.len(), 1);
        assert_eq!(d.tyre_findings[0].text, "Pneus : pas d'écart mesurable entre intérieur, milieu et extérieur.");
    }

    #[test]
    fn low_points_are_located_and_spikes_are_only_counted() {
        let laps: Vec<LapMeta> = (1..=5).map(|n| lap_meta(n, false)).collect();
        // Arrière droit : 18 mm au freinage du virage (400 à 420 m), une pointe à 1 mm en ligne droite.
        let d = setup_diagnosis(
            &recording(&|t, d| {
                if (400.0..420.0).contains(&d) {
                    t.ride_h_m[3] = 0.018;
                }
                if (800.0..801.0).contains(&d) {
                    t.ride_h_m[3] = 0.001;
                }
            }),
            &laps,
            &corners(),
        );
        let rr = &d.heights[3];
        assert_eq!(rr.low.as_ref().unwrap().value, 18.0);
        assert_eq!(rr.median.as_ref().unwrap().value, 35.0);
        // 1 m de pointe = 2 échantillons par tour (0,83 m par échantillon) : comptés, pas interprétés.
        assert_eq!(rr.spikes_under_5mm, 10);
        assert_eq!(rr.low_zone.as_deref(), Some("V1 (5 tours sur 5)"));
        assert!(d.height_findings.iter().any(|f| f.text == "Arrière droit : points les plus bas à V1 (5 tours sur 5), 18,0 mm."));
        assert!(!format!("{d:?}").to_lowercase().contains("talonn"));
        // Points bas en ligne droite (cas réel de Monza : appui aéro à pleine vitesse).
        let d = setup_diagnosis(&recording(&|t, d| if (750.0..900.0).contains(&d) { t.ride_h_m[2] = 0.020 }), &laps, &corners());
        assert!(d.height_findings.iter().any(|f| f.text == "Arrière gauche : points les plus bas en ligne droite (5 tours sur 5), 20,0 mm."), "{:?}", d.height_findings);
    }

    #[test]
    fn balance_reads_understeer_and_oversteer_by_phase() {
        let laps: Vec<LapMeta> = (1..=5).map(|n| lap_meta(n, false)).collect();
        // Référence (courbe douce) : lacet / (v × volant) = 0,096 / (50 × 0,06) = 0,032.
        // Entrée : volant 0,2 pour 0,4 rad/s → 0,04 → rapport 1,25 (survire). Sortie : volant 0,4 → 0,625 (sous-vire).
        let d = setup_diagnosis(
            &recording(&|t, d| {
                if (520.0..600.0).contains(&d) {
                    t.steer_rad = 0.4;
                }
            }),
            &laps,
            &corners(),
        );
        let get = |p| d.balance.iter().find(|b| b.phase == p).unwrap();
        assert_eq!(get(BalancePhase::Entree).trend, BalanceTrend::Survire);
        assert!((get(BalancePhase::Entree).ratio.value - 1.25).abs() < 1e-3);
        assert_eq!(get(BalancePhase::Sortie).trend, BalanceTrend::SousVire);
        assert!((get(BalancePhase::Sortie).ratio.value - 0.625).abs() < 1e-3);
        assert_eq!(get(BalancePhase::Sortie).ratio.laps, 5);
        assert_eq!(get(BalancePhase::Entree).countersteer_pct, 0.0);
    }

    #[test]
    fn locks_need_wheel_rotation_and_repeat_at_the_same_corner() {
        let laps: Vec<LapMeta> = (1..=5).map(|n| lap_meta(n, false)).collect();
        // Roue libre : rayon 0,35 m (rotation 142,9 rad/s à 50 m/s). Freinage 410–430 m : avant gauche
        // à 40 % de glissement.
        let d = setup_diagnosis(
            &recording(&|t, d| {
                t.wheel_rot = [50.0 / 0.35; 4];
                t.wheel_vlong = [50.0; 4];
                if (410.0..430.0).contains(&d) {
                    t.throttle = 0.0;
                    t.brake = 0.8;
                    t.wheel_rot[0] = 0.6 * 50.0 / 0.35;
                }
            }),
            &laps,
            &corners(),
        );
        assert_eq!(d.locks.len(), 1, "{:?}", d.locks);
        let l = &d.locks[0];
        assert_eq!((l.corner.as_str(), l.wheel, l.laps), ("V1", "Avant gauche", 5));
        assert!((l.slip_pct - 40.0).abs() < 0.5);
        let old = setup_diagnosis(&recording(&|_, _| {}), &laps, &corners());
        assert!(old.locks.is_empty() && old.missing.iter().any(|m| m.contains("Blocages")));
    }

    #[test]
    fn setup_rows_keep_only_adjustable_settings_in_game_values() {
        let rows = setup_rows(&recording(&|_, _| {}).setup.unwrap().values);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].label, "Carrossage AvG");
        assert_eq!((rows[0].step, rows[0].min, rows[0].max, rows[0].display.as_str()), (30, 0, 31, "-1.0 deg"));
        assert_eq!(api_label("VM_BRAKE_BALANCE"), ("Répartition de freinage (AV:AR)".into(), "Freins".into()));
    }
}
