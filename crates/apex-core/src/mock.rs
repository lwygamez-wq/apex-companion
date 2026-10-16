//! Générateur de sessions de démonstration réalistes (déterministe).
//!
//! Circuit fictif défini par segments (lignes droites / virages à rayon constant),
//! profil de vitesse « physique simplifiée » : v_max virage = √(μ·g·R),
//! accélération dépendante de la vitesse, freinage à décélération bornée.
//! Chaque tour reçoit des défauts injectés **connus** (freinage anticipé,
//! vitesse de corde, remise des gaz tardive), ce qui permet de tester que
//! l'analyse retrouve exactement ce qui a été injecté.

use crate::model::*;

#[derive(Debug, Clone, Copy)]
pub enum Seg {
    Straight(f32),
    /// rayon (m), angle (degrés, signe = sens)
    Corner(f32, f32),
}

/// « Circuit Démo » ≈ 3,6 km, 7 virages.
pub const DEMO_TRACK: &[Seg] = &[
    Seg::Straight(620.0),
    Seg::Corner(28.0, 95.0),   // V1 épingle serrée
    Seg::Straight(300.0),
    Seg::Corner(85.0, -70.0),  // V2
    Seg::Straight(240.0),
    Seg::Corner(140.0, 60.0),  // V3 rapide
    Seg::Straight(420.0),
    Seg::Corner(22.0, 150.0),  // V4 épingle
    Seg::Straight(380.0),
    Seg::Corner(60.0, -80.0),  // V5
    Seg::Straight(180.0),
    Seg::Corner(45.0, 90.0),   // V6
    Seg::Straight(260.0),
    Seg::Corner(70.0, 75.0),   // V7
    Seg::Straight(110.0),
];

/// Défaut injecté sur un virage (index de virage 0-based).
#[derive(Debug, Clone, Copy, Default)]
pub struct CornerFault {
    /// Freine `early_brake_m` mètres plus tôt (atteint la vitesse de corde en avance).
    pub early_brake_m: f32,
    /// Facteur sur la vitesse de corde (1.0 = référence).
    pub apex_speed_factor: f32,
    /// Maintient la vitesse de corde `late_throttle_m` mètres après la sortie du virage.
    pub late_throttle_m: f32,
    /// Ajoute des corrections de volant en sortie.
    pub wobble: bool,
}

#[derive(Debug, Clone, Copy)]
struct Pt {
    x: f32,
    y: f32,
    radius: f32, // f32::INFINITY en ligne droite
    corner: Option<usize>,
    dir: f32,
}

fn geometry(track: &[Seg]) -> Vec<Pt> {
    let (mut x, mut y, mut h) = (0f32, 0f32, 0f32);
    let mut pts = Vec::new();
    let mut ci = 0usize;
    for seg in track {
        match *seg {
            Seg::Straight(len) => {
                for _ in 0..len as usize {
                    pts.push(Pt { x, y, radius: f32::INFINITY, corner: None, dir: 0.0 });
                    x += h.cos();
                    y += h.sin();
                }
            }
            Seg::Corner(r, ang) => {
                let len = r * ang.abs().to_radians();
                let dh = ang.signum() / r;
                for _ in 0..len as usize {
                    pts.push(Pt { x, y, radius: r, corner: Some(ci), dir: ang.signum() });
                    h += dh;
                    x += h.cos();
                    y += h.sin();
                }
                ci += 1;
            }
        }
    }
    pts
}

/// Profil de vitesse par mètre avec défauts.
fn speed_profile(pts: &[Pt], faults: &[CornerFault], mu: f32) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let n = pts.len();
    let g = 9.81;
    let vmax = 88.0;
    let mut limit = vec![vmax; n];
    for (i, p) in pts.iter().enumerate() {
        if p.radius.is_finite() {
            let f = p.corner.and_then(|c| faults.get(c)).map_or(1.0, |f| if f.apex_speed_factor > 0.0 { f.apex_speed_factor } else { 1.0 });
            limit[i] = ((mu * g * p.radius).sqrt() * f).min(vmax);
        }
    }
    // Défauts de freinage anticipé / remise des gaz tardive : prolongent la limite de vitesse.
    let mut starts = vec![];
    for i in 0..n {
        if let Some(c) = pts[i].corner {
            if i == 0 || pts[i - 1].corner != Some(c) {
                starts.push((c, i));
            }
        }
    }
    let base = limit.clone();
    for &(c, s) in &starts {
        let Some(f) = faults.get(c) else { continue };
        let e = (s..n).take_while(|&k| pts[k].corner == Some(c)).last().unwrap_or(s);
        let vc = base[s..=e].iter().copied().fold(f32::MAX, f32::min);
        let pre = f.early_brake_m as usize;
        for l in &mut limit[s.saturating_sub(pre)..s] {
            *l = l.min(vc);
        }
        let post = f.late_throttle_m as usize;
        for l in &mut limit[e..(e + post).min(n)] {
            *l = l.min(vc);
        }
    }
    // Passe arrière (freinage, ~2,6 g) puis passe avant (accélération).
    let mut v = limit.clone();
    let decel = 25.0;
    for i in (0..n).rev() {
        let next = v[(i + 1) % n];
        v[i] = v[i].min((next * next + 2.0 * decel).sqrt());
    }
    for _ in 0..2 {
        for i in 0..n {
            let prev = v[(i + n - 1) % n];
            let a = (11.0 - 0.1 * prev).max(0.5);
            v[i] = v[i].min((prev * prev + 2.0 * a).sqrt());
        }
    }
    // Pédales déduites du profil.
    let mut thr = vec![0f32; n];
    let mut brk = vec![0f32; n];
    for i in 0..n {
        let dv = v[(i + 1) % n] - v[i];
        if dv < -0.05 {
            let a = (v[i] * v[i] - v[(i + 1) % n].powi(2)) / 2.0;
            brk[i] = (a / decel).clamp(0.0, 1.0);
        } else if dv > 0.02 {
            thr[i] = 1.0;
        } else {
            // Vitesse maintenue : gaz partiels en appui.
            thr[i] = if v[i] >= vmax - 0.5 { 1.0 } else { 0.35 };
        }
    }
    (v, thr, brk)
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 40) as f32) / ((1u64 << 24) as f32)
    }
    fn noise(&mut self, amp: f32) -> f32 {
        (self.next() - 0.5) * 2.0 * amp
    }
}

/// Génère un tour lancé (ou de sortie) échantillonné à `hz`.
pub fn generate_lap(faults: &[CornerFault], hz: f32, seed: u64) -> (LapTrace, f64) {
    generate_lap_on(DEMO_TRACK, faults, hz, seed)
}

/// Variante sur un circuit quelconque (ex. circuit long pour les tests de charge).
pub fn generate_lap_on(track: &[Seg], faults: &[CornerFault], hz: f32, seed: u64) -> (LapTrace, f64) {
    let pts = geometry(track);
    let (v, thr, brk) = speed_profile(&pts, faults, 1.9);
    let n = pts.len();
    // Temps cumulé par mètre.
    let mut tcum = vec![0f64; n + 1];
    for i in 0..n {
        tcum[i + 1] = tcum[i] + 1.0 / v[i] as f64;
    }
    let lap_time = tcum[n];
    let mut rng = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
    let mut tr = LapTrace::default();
    let (mut gear, mut steer, mut rpm, mut xs, mut ys) = (vec![], vec![], vec![], vec![], vec![]);
    let dt = 1.0 / hz as f64;
    let mut t = 0f64;
    let mut j = 0usize;
    while t < lap_time {
        while j + 1 < n && tcum[j + 1] < t {
            j += 1;
        }
        let frac = ((t - tcum[j]) / (tcum[j + 1] - tcum[j])) as f32;
        let d = j as f32 + frac;
        let p = pts[j];
        let spd = v[j];
        tr.time_s.push(t as f32);
        tr.dist_m.push(d);
        tr.speed_mps.push(spd + rng.noise(0.05));
        tr.throttle.push(thr[j]);
        tr.brake.push(brk[j]);
        // Rapport avec hystérésis : montée au-delà de la bande, descente avec marge.
        let band = spd / 88.0 * 6.0;
        let target = band.ceil().clamp(1.0, 6.0);
        let prev_g = gear.last().copied().unwrap_or(target);
        let g = if (target > prev_g && band > prev_g + 0.02) || (target < prev_g && band < prev_g - 1.15) { target } else { prev_g };
        gear.push(g);
        rpm.push(4000.0 + 4500.0 * ((spd / 88.0 * 6.0) - (g - 1.0)).clamp(0.0, 1.0));
        let mut s = if p.radius.is_finite() { p.dir * (2.7 / p.radius).atan() * 12.0 } else { 0.0 };
        // Corrections de volant injectées sur les 40 m suivant la sortie du virage.
        if p.corner.is_none() {
            if let Some(c) = pts[j.saturating_sub(40)].corner {
                if faults.get(c).is_some_and(|f| f.wobble) {
                    s += 0.12 * (d * 0.3).sin();
                }
            }
        }
        steer.push(s + rng.noise(0.004));
        xs.push(p.x);
        ys.push(p.y);
        t += dt;
    }
    tr.gear = Some(gear);
    tr.rpm = Some(rpm);
    tr.steer_rad = Some(steer);
    tr.pos_x = Some(xs);
    tr.pos_y = Some(ys);
    (tr, lap_time)
}

pub fn track_length_m() -> f32 {
    geometry(DEMO_TRACK).len() as f32
}

/// Session de démonstration : tour de sortie, 6 tours lancés avec défauts variés, tour de retour.
pub fn demo_session(seed: u64, started_at_unix: i64) -> ImportedSession {
    let nc = 7;
    let clean = vec![CornerFault { apex_speed_factor: 1.0, ..Default::default() }; nc];
    let with = |c: usize, f: CornerFault| {
        let mut v = clean.clone();
        v[c] = f;
        v
    };
    let plans: Vec<(LapKind, Vec<CornerFault>, bool)> = vec![
        (LapKind::OutLap, vec![CornerFault { apex_speed_factor: 0.85, early_brake_m: 30.0, late_throttle_m: 40.0, wobble: false }; nc], false),
        (LapKind::Flying, with(0, CornerFault { early_brake_m: 14.0, apex_speed_factor: 0.95, ..Default::default() }), false),
        (LapKind::Flying, clean.clone(), false),
        (LapKind::Flying, with(3, CornerFault { apex_speed_factor: 0.9, ..Default::default() }), false),
        (LapKind::Flying, with(4, CornerFault { apex_speed_factor: 1.0, late_throttle_m: 35.0, wobble: true, ..Default::default() }), true),
        (LapKind::Flying, with(0, CornerFault { early_brake_m: 12.0, apex_speed_factor: 0.96, ..Default::default() }), false),
        (LapKind::InLap, vec![CornerFault { apex_speed_factor: 0.8, early_brake_m: 40.0, late_throttle_m: 60.0, wobble: false }; nc], false),
    ];
    let laps = plans
        .into_iter()
        .enumerate()
        .map(|(i, (kind, faults, off))| {
            let (trace, lt) = generate_lap(&faults, 60.0, seed + i as u64);
            let validity = match kind {
                LapKind::Flying if off => Validity::Invalid,
                LapKind::Flying => Validity::Valid,
                _ => Validity::Unknown,
            };
            ImportedLap {
                meta: LapMeta {
                    number: i as i32,
                    kind,
                    lap_time_s: Some(lt),
                    sector_times_s: vec![],
                    flags: LapFlags { off_track: off, pit: kind != LapKind::Flying, ..Default::default() },
                    validity,
                    validity_source: ValiditySource::SimReported,
                    fuel_start_l: None,
                    fuel_used_l: None,
                },
                trace,
            }
        })
        .collect();
    ImportedSession {
        meta: SessionMeta {
            sim: SimId::Demo,
            track: "Circuit Démo".into(),
            track_config: Some("Complet".into()),
            track_length_m: Some(track_length_m()),
            car: "Formule Démo".into(),
            kind: SessionKind::Practice,
            started_at_unix,
            weather: Weather { air_temp_c: Some(22.0), track_temp_c: Some(31.0), description: Some("Ensoleillé".into()), wet: Some(false) },
            source_fingerprint: format!("demo:{seed}"),
            source_path: None,
        },
        laps,
        sample_rate_hz: Some(60.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_lap_is_plausible() {
        let (t, lt) = generate_lap(&[], 60.0, 1);
        t.validate().unwrap();
        assert!(lt > 60.0 && lt < 130.0, "temps au tour {lt}");
        let vmax = t.speed_mps.iter().copied().fold(0.0, f32::max) * 3.6;
        let vmin = t.speed_mps.iter().copied().fold(f32::MAX, f32::min) * 3.6;
        assert!(vmax > 220.0 && vmin < 100.0, "vmax={vmax} vmin={vmin}");
        assert!((t.max_dist() - track_length_m()).abs() < 5.0);
    }

    #[test]
    fn faults_cost_time() {
        let (_, clean) = generate_lap(&[], 60.0, 1);
        let f = [CornerFault { early_brake_m: 15.0, apex_speed_factor: 0.95, ..Default::default() }];
        let (_, slow) = generate_lap(&f, 60.0, 1);
        assert!(slow > clean + 0.05, "clean={clean} slow={slow}");
    }
}
