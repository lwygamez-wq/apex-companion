#![allow(clippy::type_complexity, clippy::needless_range_loop)]
//! Données de circuits embarquées.
//!
//! - Largeurs de piste mesurées (base TUM racetrack-database, LGPL-3.0, voir
//!   `data/TUM-racetrack-database-LICENSE.txt`) : ligne médiane + largeur gauche / droite pour 25 circuits.
//!   Le repère de la base est local : la ligne médiane est recalée sur ta trajectoire (rotation +
//!   translation, sens et point de départ quelconques) avant d'en tirer les bords.

use crate::analysis::surface::TrackEdges;
use serde::Deserialize;
use std::io::Read;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct TumTrack {
    id: String,
    names: Vec<String>,
    /// x, y, largeur à droite, largeur à gauche (m).
    pts: Vec<[f32; 4]>,
}

static TUM: OnceLock<Vec<TumTrack>> = OnceLock::new();

fn tum() -> &'static [TumTrack] {
    TUM.get_or_init(|| {
        let raw = include_bytes!("../data/tum_tracks.json.gz");
        let mut s = String::new();
        if flate2::read::GzDecoder::new(&raw[..]).read_to_string(&mut s).is_err() {
            return vec![];
        }
        serde_json::from_str(&s).unwrap_or_default()
    })
}

pub(crate) fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' | 'ã' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            c => c,
        })
        .collect()
}

/// Points régulièrement espacés le long d'une boucle (abscisse curviligne), `n` points.
fn resample_loop(p: &[[f32; 2]], n: usize) -> Vec<[f32; 2]> {
    let m = p.len();
    if m < 3 {
        return vec![];
    }
    let mut cum = vec![0.0f32; m + 1];
    for i in 0..m {
        let (a, b) = (p[i], p[(i + 1) % m]);
        cum[i + 1] = cum[i] + ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    }
    let total = cum[m];
    let mut out = Vec::with_capacity(n);
    let mut j = 0;
    for k in 0..n {
        let s = total * k as f32 / n as f32;
        while j < m && cum[j + 1] < s {
            j += 1;
        }
        let seg = (cum[j + 1] - cum[j]).max(1e-6);
        let t = (s - cum[j]) / seg;
        let (a, b) = (p[j % m], p[(j + 1) % m]);
        out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
    }
    out
}

fn length(p: &[[f32; 2]]) -> f32 {
    (0..p.len()).map(|i| {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
    }).sum()
}

/// Recalage rigide 2D (Kabsch) de `a` sur `b` (mêmes indices) : (cos, sin, tx, ty, rms).
fn kabsch(a: &[[f32; 2]], b: &[[f32; 2]]) -> (f32, f32, f32, f32, f32) {
    let n = a.len() as f32;
    let (mut ax, mut ay, mut bx, mut by) = (0.0, 0.0, 0.0, 0.0);
    for (p, q) in a.iter().zip(b) {
        ax += p[0];
        ay += p[1];
        bx += q[0];
        by += q[1];
    }
    let (ax, ay, bx, by) = (ax / n, ay / n, bx / n, by / n);
    let (mut sxx, mut sxy) = (0.0f32, 0.0f32);
    for (p, q) in a.iter().zip(b) {
        let (px, py, qx, qy) = (p[0] - ax, p[1] - ay, q[0] - bx, q[1] - by);
        sxx += px * qx + py * qy;
        sxy += px * qy - py * qx;
    }
    let th = sxy.atan2(sxx);
    let (c, s) = (th.cos(), th.sin());
    let (tx, ty) = (bx - (c * ax - s * ay), by - (s * ax + c * ay));
    let mut e = 0.0;
    for (p, q) in a.iter().zip(b) {
        let (x, y) = (c * p[0] - s * p[1] + tx, s * p[0] + c * p[1] + ty);
        e += (x - q[0]).powi(2) + (y - q[1]).powi(2);
    }
    (c, s, tx, ty, (e / n).sqrt())
}

/// Bords de piste mesurés (base TUM) pour ce circuit, recalés sur `spine` (ta trajectoire, points
/// réguliers, repère de la carte). `None` si le circuit n'est pas dans la base ou si le tracé ne
/// colle pas (autre configuration) : écart moyen > 8 m après recalage, ou longueur trop différente.
pub fn measured_edges(track_name: &str, spine: &[[f32; 2]]) -> Option<TrackEdges> {
    let name = fold(track_name);
    let t = tum().iter().find(|t| t.names.iter().any(|n| name.contains(n.as_str())))?;
    let center: Vec<[f32; 2]> = t.pts.iter().map(|p| [p[0], p[1]]).collect();
    let (lc, ls) = (length(&center), length(spine));
    if ls < 500.0 || (lc / ls - 1.0).abs() > 0.08 {
        return None;
    }
    const N: usize = 360;
    let a0 = resample_loop(&center, N);
    let b = resample_loop(spine, N);
    if a0.len() != N || b.len() != N {
        return None;
    }
    // Meilleur recalage sur tous les points de départ, dans les deux sens.
    let mut best: Option<(f32, usize, bool, (f32, f32, f32, f32))> = None;
    for rev in [false, true] {
        let a: Vec<[f32; 2]> = if rev { a0.iter().rev().copied().collect() } else { a0.clone() };
        for k in 0..N {
            let rot: Vec<[f32; 2]> = (0..N).map(|i| a[(i + k) % N]).collect();
            let (c, s, tx, ty, rms) = kabsch(&rot, &b);
            if best.as_ref().is_none_or(|x| rms < x.0) {
                best = Some((rms, k, rev, (c, s, tx, ty)));
            }
        }
    }
    let (rms0, _, rev, (mut c, mut s, mut tx, mut ty)) = best?;
    if rms0 > 25.0 {
        log::info!("largeurs TUM {} : tracé différent (écart {rms0:.1} m), ignorées", t.id);
        return None;
    }
    // Affinage ICP : chaque point de ta trajectoire est apparié au point le plus proche de la ligne
    // médiane (ta trajectoire coupe les virages : l'appariement par abscisse dérive de dizaines de mètres).
    let dense: Vec<[f32; 2]> = resample_loop(&center, (lc / 2.0) as usize);
    let sub: Vec<[f32; 2]> = resample_loop(spine, 600);
    let nearest = |q: [f32; 2], pts: &[[f32; 2]]| -> (usize, f32) {
        let mut best = (0, f32::INFINITY);
        for (i, p) in pts.iter().enumerate() {
            let d = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2);
            if d < best.1 {
                best = (i, d);
            }
        }
        best
    };
    let mut err = f32::INFINITY;
    for _ in 0..25 {
        let moved: Vec<[f32; 2]> = dense.iter().map(|p| [c * p[0] - s * p[1] + tx, s * p[0] + c * p[1] + ty]).collect();
        let pairs: Vec<([f32; 2], [f32; 2])> = sub.iter().map(|q| (dense[nearest(*q, &moved).0], *q)).collect();
        let (a, b): (Vec<[f32; 2]>, Vec<[f32; 2]>) = pairs.into_iter().unzip();
        let (c2, s2, tx2, ty2, e) = kabsch(&a, &b);
        (c, s, tx, ty) = (c2, s2, tx2, ty2);
        if (err - e).abs() < 0.01 {
            err = e;
            break;
        }
        err = e;
    }
    if std::env::var("APEX_DBG").is_ok() {
        eprintln!("TUM {} rms index {rms0:.2} → ICP {err:.2} lc {lc:.0} ls {ls:.0}", t.id);
    }
    // Contrôle : ta trajectoire doit rester entre les bords (1 m de tolérance) sur 97 % du tour.
    let n = t.pts.len();
    let inside = sub
        .iter()
        .filter(|q| {
            // Point ramené dans le repère de la base.
            let (x, y) = (q[0] - tx, q[1] - ty);
            let p = [c * x + s * y, -s * x + c * y];
            let (i, _) = nearest(p, &center);
            let (a, b) = (t.pts[(i + n - 1) % n], t.pts[(i + 1) % n]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l = (dx * dx + dy * dy).sqrt().max(1e-6);
            let off = (p[0] - t.pts[i][0]) * (-dy / l) + (p[1] - t.pts[i][1]) * (dx / l);
            off <= t.pts[i][3] + 1.0 && off >= -t.pts[i][2] - 1.0
        })
        .count() as f32
        / sub.len() as f32;
    if std::env::var("APEX_DBG").is_ok() {
        eprintln!("TUM {} : {:.1} % de la trajectoire entre les bords", t.id, inside * 100.0);
    }
    if err > 6.0 || inside < 0.97 {
        log::info!("largeurs TUM {} : recalage insuffisant ({err:.1} m, {:.0} % dans la piste), ignorées", t.id, inside * 100.0);
        return None;
    }
    // Bords dans le repère TUM (gauche / droite selon le sens de la base), puis transformés.
    let mut left = Vec::with_capacity(n);
    let mut right = Vec::with_capacity(n);
    for i in 0..n {
        let (p, q) = (t.pts[(i + n - 1) % n], t.pts[(i + 1) % n]);
        let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
        let l = (dx * dx + dy * dy).sqrt().max(1e-6);
        let (nx, ny) = (-dy / l, dx / l); // normale à gauche
        let [x, y, wr, wl] = t.pts[i];
        let tf = |x: f32, y: f32| [c * x - s * y + tx, s * x + c * y + ty];
        left.push(tf(x + nx * wl, y + ny * wl));
        right.push(tf(x - nx * wr, y - ny * wr));
    }
    // Base décrite dans l'autre sens que ta trajectoire : sa gauche est ta droite.
    if rev {
        std::mem::swap(&mut left, &mut right);
    }
    left.push(left[0]);
    right.push(right[0]);
    Some(TrackEdges { left, right, source: "tum".into() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tum_edges_are_recovered_after_rotation_reversal_and_shift() {
        let t = tum().iter().find(|t| t.id == "Spa").expect("base TUM embarquée");
        // Trajectoire synthétique : la ligne médiane de Spa tournée de 50°, décalée, parcourue à l'envers
        // et commençant ailleurs, échantillonnée tous les ~2 m.
        let (c, s) = (50f32.to_radians().cos(), 50f32.to_radians().sin());
        let mut pts: Vec<[f32; 2]> = t.pts.iter().map(|p| [c * p[0] - s * p[1] + 1234.0, s * p[0] + c * p[1] - 567.0]).collect();
        pts.reverse();
        pts.rotate_left(300);
        let spine = resample_loop(&pts, 3500);
        let e = measured_edges("Circuit de Spa-Francorchamps", &spine).expect("recalage");
        assert_eq!(e.source, "tum");
        // Largeur reconstruite ≈ somme des largeurs de la base, partout.
        let i = 100;
        let w = ((e.left[i][0] - e.right[i][0]).powi(2) + (e.left[i][1] - e.right[i][1]).powi(2)).sqrt();
        assert!((w - (t.pts[i][2] + t.pts[i][3])).abs() < 0.5, "{w}");
        // Bord gauche bien à gauche du sens de ta trajectoire.
        let (a, b) = (spine[0], spine[5]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let near = |pts: &[[f32; 2]]| *pts.iter().min_by(|p, q| ((p[0] - a[0]).powi(2) + (p[1] - a[1]).powi(2)).total_cmp(&((q[0] - a[0]).powi(2) + (q[1] - a[1]).powi(2)))).unwrap();
        let l = near(&e.left);
        assert!(dx * (l[1] - a[1]) - dy * (l[0] - a[0]) > 0.0, "gauche à gauche");
        // Autre circuit sous un faux nom : refusé.
        let monza = tum().iter().find(|t| t.id == "Monza").unwrap();
        let other = resample_loop(&monza.pts.iter().map(|p| [p[0], p[1]]).collect::<Vec<_>>(), 3000);
        assert!(measured_edges("Spa-Francorchamps", &other).is_none());
        assert!(measured_edges("Circuit inconnu", &spine).is_none());
    }
}

// ---------------------------------------------------------------------------------------------
// Banque de circuits OpenStreetMap (open-racetrack-db, ODbL 1.0, © contributeurs OpenStreetMap) :
// 752 sites, 1 363 configurations, tracés GPS. Sert à identifier le circuit et la configuration,
// et à numéroter les virages d'après le vrai tracé quand aucun relevé n'est intégré.

#[derive(Deserialize)]
struct OsmLayoutRaw {
    n: String,
    len: f32,
    /// Deltas entiers lat, lon en millionièmes de degré.
    p: Vec<i64>,
}

#[derive(Deserialize)]
struct OsmVenueRaw {
    n: String,
    c: String,
    loc: [f64; 2],
    l: Vec<OsmLayoutRaw>,
}

static OSM: OnceLock<Vec<OsmVenueRaw>> = OnceLock::new();

fn osm() -> &'static [OsmVenueRaw] {
    OSM.get_or_init(|| {
        let raw = include_bytes!("../data/osm_tracks.json.gz");
        let mut s = String::new();
        if flate2::read::GzDecoder::new(&raw[..]).read_to_string(&mut s).is_err() {
            return vec![];
        }
        serde_json::from_str(&s).unwrap_or_default()
    })
}

/// Nombre de sites et de configurations dans la banque.
pub fn bank_size() -> (usize, usize) {
    (osm().len(), osm().iter().map(|v| v.l.len()).sum())
}

fn decode(p: &[i64]) -> Vec<(f64, f64)> {
    let (mut a, mut b) = (0i64, 0i64);
    p.chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| {
            a += c[0];
            b += c[1];
            (a as f64 / 1e6, b as f64 / 1e6)
        })
        .collect()
}

fn haversine_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la1, lo1, la2, lo2) = (a.0.to_radians(), a.1.to_radians(), b.0.to_radians(), b.1.to_radians());
    let h = ((la2 - la1) / 2.0).sin().powi(2) + la1.cos() * la2.cos() * ((lo2 - lo1) / 2.0).sin().powi(2);
    2.0 * 6_371_000.0 * h.sqrt().asin()
}

/// Circuit reconnu dans la banque, avec son tracé dans le repère des trajectoires de la session.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BankMatch {
    pub venue: String,
    pub layout: String,
    pub country: String,
    pub length_m: f32,
    /// Écart moyen entre ta trajectoire et le tracé (m).
    pub fit_m: f32,
    #[serde(skip)]
    pub centerline: Vec<[f32; 2]>,
}

/// Cherche le circuit (site à moins de 5 km de `track_pos`) et la configuration dont le tracé colle
/// à ta trajectoire `spine` (repère d'origine `origin`) : écart moyen ≤ 12 m et longueur à ±12 %.
pub fn bank_match(track_pos: (f64, f64), origin: [f64; 2], spine: &[[f32; 2]]) -> Option<BankMatch> {
    if spine.len() < 50 {
        return None;
    }
    let ls = length(spine);
    let sub = resample_loop(spine, 300);
    let mut best: Option<BankMatch> = None;
    for v in osm().iter().filter(|v| haversine_m((v.loc[0], v.loc[1]), track_pos) < 5000.0) {
        for l in &v.l {
            let pts: Vec<[f32; 2]> = decode(&l.p).into_iter().map(|(la, lo)| {
                let (x, y) = crate::ingest::latlon_to_xy(la, lo, (origin[0], origin[1]));
                [x, y]
            }).collect();
            if pts.len() < 10 {
                continue;
            }
            let lc = if l.len > 0.0 { l.len } else { length(&pts) };
            if (lc / ls - 1.0).abs() > 0.12 {
                continue;
            }
            let dense = resample_loop(&pts, (lc / 3.0).max(50.0) as usize);
            let fit = sub
                .iter()
                .map(|q| dense.iter().map(|p| (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).fold(f32::INFINITY, f32::min).sqrt())
                .sum::<f32>()
                / sub.len() as f32;
            if fit <= 12.0 && best.as_ref().is_none_or(|b| fit < b.fit_m) {
                best = Some(BankMatch { venue: v.n.clone(), layout: l.n.clone(), country: v.c.clone(), length_m: lc, fit_m: fit, centerline: dense });
            }
        }
    }
    best
}

/// Virages du tracé (méthode de la courbure, comme les tables intégrées) : (index du point d'apex,
/// sens +1 gauche / −1 droite, angle total en degrés), sur une boucle échantillonnée à pas constant `step`.
pub fn geometric_turns(p: &[[f32; 2]], step: f32) -> Vec<(usize, i8, f32)> {
    geometric_turns_with(p, step, 0.006, 12.0)
}

/// Comme `geometric_turns`, en cherchant les seuils qui donnent le nombre officiel de virages
/// (fiche iRacing) : la numérotation colle alors mieux à celle du circuit.
pub fn geometric_turns_official(p: &[[f32; 2]], step: f32, official: Option<u16>) -> Vec<(usize, i8, f32)> {
    let Some(target) = official.filter(|n| *n >= 3) else { return geometric_turns(p, step) };
    let mut best: Option<(u32, f32, Vec<(usize, i8, f32)>)> = None;
    for thr in [0.006f32, 0.005, 0.007, 0.004, 0.008, 0.003, 0.010] {
        for min_deg in [12.0f32, 10.0, 15.0, 8.0, 18.0, 22.0, 6.0, 26.0, 30.0] {
            let t = geometric_turns_with(p, step, thr, min_deg);
            let miss = (t.len() as i32 - target as i32).unsigned_abs();
            // À écart égal, on garde les seuils les plus proches des valeurs par défaut (essayés d'abord).
            let cost = ((thr - 0.006).abs() / 0.002) + ((min_deg - 12.0).abs() / 6.0);
            if best.as_ref().is_none_or(|b| miss < b.0 || (miss == b.0 && cost < b.1)) {
                best = Some((miss, cost, t));
            }
        }
    }
    best.map(|b| b.2).unwrap_or_default()
}

fn geometric_turns_with(p: &[[f32; 2]], step: f32, thr: f32, min_deg: f32) -> Vec<(usize, i8, f32)> {
    let n = p.len();
    if n < 60 {
        return vec![];
    }
    let k = ((15.0 / step).round() as usize).max(1);
    let mut h: Vec<f32> = (0..n)
        .map(|i| {
            let (a, b) = (p[(i + n - k) % n], p[(i + k) % n]);
            (b[1] - a[1]).atan2(b[0] - a[0])
        })
        .collect();
    for i in 1..n {
        let mut d = h[i] - h[i - 1];
        while d > std::f32::consts::PI {
            d -= 2.0 * std::f32::consts::PI;
        }
        while d < -std::f32::consts::PI {
            d += 2.0 * std::f32::consts::PI;
        }
        h[i] = h[i - 1] + d;
    }
    // Courbure (rad / m), lissée sur ±10 m.
    let raw: Vec<f32> = (0..n).map(|i| (h[(i + 1).min(n - 1)] - h[i.saturating_sub(1)]) / (2.0 * step)).collect();
    let w = ((10.0 / step).round() as isize).max(1);
    let c: Vec<f32> = (0..n as isize)
        .map(|i| (-w..=w).map(|d| raw[((i + d).rem_euclid(n as isize)) as usize]).sum::<f32>() / (2 * w + 1) as f32)
        .collect();
    let mut out = vec![];
    let mut i = 0;
    while i < n {
        if c[i].abs() > thr {
            let sg = c[i].signum();
            let mut j = i;
            while j < n && c[j].abs() > thr * 0.6 && c[j].signum() == sg {
                j += 1;
            }
            let turn = (c[i..j].iter().sum::<f32>() * step).to_degrees();
            if turn.abs() >= min_deg {
                let ap = i + c[i..j].iter().enumerate().max_by(|a, b| a.1.abs().total_cmp(&b.1.abs())).map(|x| x.0).unwrap_or(0);
                out.push((ap, if turn > 0.0 { 1 } else { -1 }, turn.abs()));
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

/// Virages numérotés d'après le tracé de la banque, ramenés sur ton tour : (distance de l'apex sur
/// ton tour, numéro). La numérotation suit ton sens de roulage depuis la ligne de départ.
/// `lap_xy` / `lap_dist` : positions et distances de ton tour de référence.
pub fn bank_turns_on_lap(m: &BankMatch, lap_xy: &[[f32; 2]], lap_dist: &[f32], official: Option<u16>) -> Vec<(f32, u32)> {
    if lap_xy.len() < 20 || lap_xy.len() != lap_dist.len() {
        return vec![];
    }
    let step = m.length_m / m.centerline.len().max(1) as f32;
    let mut at: Vec<f32> = geometric_turns_official(&m.centerline, step, official)
        .into_iter()
        .filter_map(|(i, _, _)| {
            let q = m.centerline[i];
            let (j, d2) = lap_xy.iter().enumerate().map(|(j, p)| (j, (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2))).min_by(|a, b| a.1.total_cmp(&b.1))?;
            (d2 < 30.0 * 30.0).then_some(lap_dist[j])
        })
        .collect();
    at.sort_by(|a, b| a.total_cmp(b));
    at.dedup_by(|a, b| (*a - *b).abs() < 25.0);
    at.into_iter().enumerate().map(|(k, d)| (d, k as u32 + 1)).collect()
}
