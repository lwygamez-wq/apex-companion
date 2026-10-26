//! Régularité par virage : ce que tu laisses en ne refaisant pas ton meilleur passage à chaque tour,
//! et ce qui change entre tes bons et tes mauvais passages (freinage, vitesse mini, gaz, sortie).
//! Uniquement tes propres tours propres de la session : pas de référence externe, pas d'alignement.

use crate::analysis::corners::{corner_metrics, CornerDef, CornerMetrics};
use crate::analysis::resample::Resampled;
use serde::Serialize;

/// Écart notable entre tes bons et tes mauvais passages sur un critère.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PassDifference {
    /// `brake`, `min_speed`, `throttle`, `exit_speed`.
    pub metric: String,
    /// Médiane de tes bons passages, puis de tes mauvais (m ou km/h).
    pub fast: f32,
    pub slow: f32,
    /// « Tu freines 9 m plus tard »…
    pub label: String,
    /// Écart rapporté au seuil de détection (pour classer).
    #[serde(skip)]
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CornerRegularity {
    pub name: String,
    pub apex_m: f32,
    pub passes: usize,
    pub best_s: f64,
    pub median_s: f64,
    /// Temps médian − meilleur temps sur le segment du virage (s).
    pub gain_s: f64,
    /// Tour du meilleur passage.
    pub best_lap: i32,
    /// Dispersion (écart-type) du point de freinage, si freinage (m).
    pub brake_spread_m: Option<f32>,
    /// Écarts bons / mauvais passages, le plus marqué d'abord.
    pub differences: Vec<PassDifference>,
    /// Phrase de conseil.
    pub advice: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RegularityReport {
    pub laps: usize,
    /// Somme des gains par virage : ce que vaudrait ton tour médian avec tes meilleurs passages (s).
    pub total_gain_s: f64,
    /// Virages classés par gain décroissant (gain ≥ 0,05 s).
    pub corners: Vec<CornerRegularity>,
    pub headline: String,
}

/// Virage de la référence : nom, entrée, apex, sortie (m).
pub type Window = (String, f32, f32, f32);

pub const MIN_LAPS: usize = 5;

fn median(v: &mut [f32]) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 })
}

fn stddev(v: &[f32]) -> Option<f32> {
    if v.len() < 3 {
        return None;
    }
    let m = v.iter().sum::<f32>() / v.len() as f32;
    Some((v.iter().map(|x| (x - m).powi(2)).sum::<f32>() / (v.len() - 1) as f32).sqrt())
}

fn fmt_s(v: f64) -> String {
    format!("{v:.2}").replace('.', ",")
}

fn differences(fast: &[&CornerMetrics], slow: &[&CornerMetrics]) -> Vec<PassDifference> {
    let med = |set: &[&CornerMetrics], f: &dyn Fn(&CornerMetrics) -> Option<f32>| {
        let mut v: Vec<f32> = set.iter().filter_map(|m| f(m)).collect();
        // Le critère doit exister sur la majorité des passages des deux groupes.
        if v.len() * 2 < set.len().max(1) {
            return None;
        }
        median(&mut v)
    };
    let mut out = vec![];
    type Getter = fn(&CornerMetrics) -> Option<f32>;
    let specs: [(&str, Getter, f32); 4] = [
        ("brake", |m| m.brake_on_m, 5.0),
        ("min_speed", |m| Some(m.min_speed_kmh), 2.0),
        ("throttle", |m| m.throttle_on_m, 8.0),
        ("exit_speed", |m| Some(m.exit_speed_kmh), 2.0),
    ];
    for (metric, get, threshold) in specs {
        let (Some(f), Some(s)) = (med(fast, &get), med(slow, &get)) else { continue };
        let d = f - s;
        if d.abs() < threshold {
            continue;
        }
        let a = d.abs().round();
        let label = match (metric, d > 0.0) {
            ("brake", true) => format!("tu freines {a} m plus tard"),
            ("brake", false) => format!("tu freines {a} m plus tôt"),
            ("min_speed", true) => format!("tu portes {a} km/h de plus au point le plus lent"),
            ("min_speed", false) => format!("tu passes {a} km/h moins vite au point le plus lent, mais tu sors mieux"),
            ("throttle", true) => format!("tu remets les gaz {a} m plus tard"),
            ("throttle", false) => format!("tu remets les gaz {a} m plus tôt"),
            ("exit_speed", true) => format!("tu sors {a} km/h plus vite"),
            _ => format!("tu sors {a} km/h moins vite"),
        };
        out.push(PassDifference { metric: metric.into(), fast: f, slow: s, label, score: d.abs() / threshold });
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out
}

/// `laps` : tours propres rééchantillonnés sur la même grille, avec leur numéro.
pub fn regularity(laps: &[(i32, &Resampled)], windows: &[Window]) -> Option<RegularityReport> {
    if laps.len() < MIN_LAPS || windows.is_empty() {
        return None;
    }
    let r0 = laps[0].1;
    let len_m = *r0.dist.last()?;
    let mut ws: Vec<&Window> = windows.iter().filter(|w| w.2 > 0.0 && w.2 < len_m).collect();
    ws.sort_by(|a, b| a.2.total_cmp(&b.2));
    if ws.is_empty() {
        return None;
    }
    // Segments bornés par les milieux entre apex consécutifs : chaque virage garde sa ligne droite de sortie.
    let mut bounds = vec![0f32];
    for p in ws.windows(2) {
        bounds.push((p[0].2 + p[1].2) / 2.0);
    }
    bounds.push(len_m);

    let mut corners = vec![];
    let mut total = 0f64;
    for (k, w) in ws.iter().enumerate() {
        let (a, b) = (bounds[k], bounds[k + 1]);
        let def = CornerDef {
            id: k as u32,
            name: w.0.clone(),
            search_start_m: a.max(w.1 - 250.0),
            entry_m: w.1.max(a),
            apex_m: w.2,
            exit_m: w.3.min(b).max(w.2 + 1.0),
        };
        let mut passes: Vec<(i32, f64, CornerMetrics)> = laps
            .iter()
            .map(|(n, r)| {
                let t = (r.time[r.idx(b)] - r.time[r.idx(a)]) as f64;
                (*n, t, corner_metrics(r, &def))
            })
            .collect();
        passes.sort_by(|x, y| x.1.total_cmp(&y.1));
        let n = passes.len();
        let best = passes[0].1;
        let med = if n % 2 == 1 { passes[n / 2].1 } else { (passes[n / 2 - 1].1 + passes[n / 2].1) / 2.0 };
        let gain = (med - best).max(0.0);
        total += gain;
        if gain < 0.05 {
            continue;
        }
        let third = (n / 3).max(2);
        let fast: Vec<&CornerMetrics> = passes[..third].iter().map(|p| &p.2).collect();
        let slow: Vec<&CornerMetrics> = passes[n - third..].iter().map(|p| &p.2).collect();
        let diffs = differences(&fast, &slow);
        let brakes: Vec<f32> = passes.iter().filter_map(|p| p.2.brake_on_m).collect();
        let brake_spread_m = if brakes.len() * 2 >= n { stddev(&brakes) } else { None };
        let advice = match diffs.first() {
            Some(d) => {
                let mut s = format!("Sur tes meilleurs passages, {}", d.label);
                if let Some(d2) = diffs.get(1) {
                    s += &format!(" et {}", d2.label);
                }
                s + "."
            }
            None => match brake_spread_m {
                Some(sd) if sd >= 6.0 => format!("Point de freinage qui varie de ±{} m : prends un repère fixe.", sd.round()),
                _ => "Pas de cause nette : trafic, ou petites variations de trajectoire.".into(),
            },
        };
        corners.push(CornerRegularity {
            name: w.0.clone(),
            apex_m: w.2,
            passes: n,
            best_s: best,
            median_s: med,
            gain_s: gain,
            best_lap: passes[0].0,
            brake_spread_m,
            differences: diffs,
            advice,
        });
    }
    corners.sort_by(|a, b| b.gain_s.total_cmp(&a.gain_s));
    let headline = match corners.first() {
        Some(c) => format!(
            "En refaisant tes meilleurs passages à chaque tour, ton tour médian gagnerait {} s. Le plus gros : {} ({} s).",
            fmt_s(total),
            c.name,
            fmt_s(c.gain_s)
        ),
        None => "Tes passages sont réguliers partout : la marge est dans la vitesse pure.".into(),
    };
    Some(RegularityReport { laps: laps.len(), total_gain_s: total, corners, headline })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tour synthétique de 1000 m : virage à 500 m, freinage à `brake_at`, mini à `vmin`.
    fn lap(brake_at: f32, vmin: f32) -> Resampled {
        let n = 1001;
        let dist: Vec<f32> = (0..n).map(|i| i as f32).collect();
        let mut speed = vec![200f32; n];
        let mut brake = vec![0f32; n];
        let mut throttle = vec![1f32; n];
        for i in 0..n {
            let d = i as f32;
            if d >= brake_at && d < 500.0 {
                speed[i] = 200.0 - (200.0 - vmin) * (d - brake_at) / (500.0 - brake_at);
                brake[i] = 0.8;
                throttle[i] = 0.0;
            } else if (500.0..600.0).contains(&d) {
                speed[i] = vmin + (200.0 - vmin) * (d - 500.0) / 100.0;
            }
        }
        let mut time = vec![0f32; n];
        for i in 1..n {
            time[i] = time[i - 1] + 1.0 / (speed[i] / 3.6);
        }
        Resampled { step_m: 1.0, dist, time, speed_kmh: speed, throttle, brake, gear: None, steer_deg: None, x: None, y: None, coverage: 1.0, source_density: 1.0 }
    }

    #[test]
    fn finds_gain_and_cause() {
        // Bons passages : freinage tard (430 m) et plus de vitesse mini ; mauvais : freinage tôt.
        let rs = [lap(430.0, 92.0), lap(431.0, 91.0), lap(429.0, 92.0), lap(410.0, 86.0), lap(409.0, 85.0), lap(411.0, 86.0)];
        let laps: Vec<(i32, &Resampled)> = rs.iter().enumerate().map(|(i, r)| (i as i32 + 2, r)).collect();
        let rep = regularity(&laps, &[("Virage 1".into(), 400.0, 500.0, 600.0)]).unwrap();
        assert_eq!(rep.laps, 6);
        let c = &rep.corners[0];
        assert_eq!(c.name, "Virage 1");
        assert!(c.gain_s > 0.05, "{}", c.gain_s);
        assert!(matches!(c.best_lap, 2..=4));
        assert!(c.differences.iter().any(|d| d.metric == "brake" && d.label.contains("plus tard")), "{:?}", c.differences);
        assert!(c.advice.starts_with("Sur tes meilleurs passages"), "{}", c.advice);
        assert!(rep.headline.contains("Virage 1"));
    }

    #[test]
    fn regular_laps_have_no_corner() {
        let rs: Vec<Resampled> = (0..6).map(|_| lap(430.0, 92.0)).collect();
        let laps: Vec<(i32, &Resampled)> = rs.iter().enumerate().map(|(i, r)| (i as i32, r)).collect();
        let rep = regularity(&laps, &[("Virage 1".into(), 400.0, 500.0, 600.0)]).unwrap();
        assert!(rep.corners.is_empty());
        assert!(rep.headline.contains("réguliers"));
    }

    #[test]
    fn needs_enough_laps() {
        let rs: Vec<Resampled> = (0..3).map(|_| lap(430.0, 92.0)).collect();
        let laps: Vec<(i32, &Resampled)> = rs.iter().map(|r| (1, r)).collect();
        assert!(regularity(&laps, &[("V".into(), 400.0, 500.0, 600.0)]).is_none());
    }
}
