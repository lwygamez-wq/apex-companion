//! Segmentation du circuit en virages et métriques par virage.
//!
//! Aucun simulateur ne fournit une définition des virages : on les détecte
//! sur le tour de référence (minima de vitesse proéminents). La numérotation
//! « V1, V2… » est donc la nôtre, pas forcément l'officielle du circuit.
//! Une table de correspondance manuelle par circuit est prévue (phase 3).

use super::resample::{smooth, Resampled};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CornerDef {
    pub id: u32,
    pub name: String,
    /// Début de fenêtre de recherche du point de freinage (m).
    pub search_start_m: f32,
    /// Entrée : juste avant le levé/freinage du tour de référence (m).
    pub entry_m: f32,
    /// Point de vitesse minimale du tour de référence (m).
    pub apex_m: f32,
    /// Sortie : après la remise pleine charge + marge pour capter l'effet sur la ligne droite (m).
    pub exit_m: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct CornerConfig {
    /// Proéminence minimale d'un minimum de vitesse pour être un virage (km/h).
    pub min_drop_kmh: f32,
    /// Fenêtre de lissage (m).
    pub smooth_m: f32,
    /// Marge ajoutée après la remise pleine charge (m).
    pub exit_margin_m: f32,
}

impl Default for CornerConfig {
    fn default() -> Self {
        Self { min_drop_kmh: 10.0, smooth_m: 20.0, exit_margin_m: 120.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Ext {
    Max(usize),
    Min(usize),
}

/// Détecte les virages sur un tour de référence rééchantillonné.
pub fn detect_corners(r: &Resampled, cfg: &CornerConfig) -> Vec<CornerDef> {
    let n = r.len();
    if n < 20 {
        return vec![];
    }
    let w = ((cfg.smooth_m / r.step_m).round() as usize).max(1);
    let v = smooth(&r.speed_kmh, w);

    // 1) Extrema alternés max/min, en commençant par un max.
    let mut ext: Vec<Ext> = vec![Ext::Max(0)];
    let mut rising = false;
    for i in 1..n {
        let d = v[i] - v[i - 1];
        if d > 0.0 && !rising {
            if let Some(Ext::Max(_)) = ext.last() {
                ext.push(Ext::Min(i - 1));
            }
            rising = true;
        } else if d < 0.0 && rising {
            if let Some(Ext::Min(_)) = ext.last() {
                ext.push(Ext::Max(i - 1));
            }
            rising = false;
        }
    }
    if let Some(Ext::Min(_)) = ext.last() {
        ext.push(Ext::Max(n - 1));
    }

    // 2) Suppression itérative des minima peu proéminents (fusion des max voisins).
    loop {
        let mut weakest: Option<(usize, f32)> = None;
        for k in (1..ext.len().saturating_sub(1)).step_by(1) {
            if let (Ext::Max(a), Ext::Min(m), Ext::Max(b)) = (ext[k - 1], ext[k], ext[k + 1]) {
                let prom = (v[a] - v[m]).min(v[b] - v[m]);
                if prom < cfg.min_drop_kmh && weakest.is_none_or(|(_, p)| prom < p) {
                    weakest = Some((k, prom));
                }
            }
        }
        let Some((k, _)) = weakest else { break };
        let (Ext::Max(a), Ext::Max(b)) = (ext[k - 1], ext[k + 1]) else { break };
        let keep = if v[a] >= v[b] { a } else { b };
        ext.splice(k - 1..=k + 1, [Ext::Max(keep)]);
    }

    // 3) Construction des fenêtres.
    let mut corners = Vec::new();
    let mut prev_exit_idx = 0usize;
    let mut prev_apex_idx = 0usize;
    for k in (1..ext.len().saturating_sub(1)).filter(|k| matches!(ext[*k], Ext::Min(_))) {
        let (Ext::Max(a), Ext::Min(m), Ext::Max(b)) = (ext[k - 1], ext[k], ext[k + 1]) else { continue };
        // Point de levé/freinage de la référence : dernier point avant l'apex à pleine charge sans frein.
        let mut lift = a;
        for i in (a..m).rev() {
            if r.throttle[i] >= 0.95 && r.brake[i] < 0.02 {
                lift = i;
                break;
            }
        }
        let entry = lift.saturating_sub((30.0 / r.step_m) as usize).max(prev_exit_idx).max(a.min(lift));
        // Remise pleine charge soutenue (≥ 30 m) après l'apex.
        let sustain = ((30.0 / r.step_m) as usize).max(1);
        let mut full = b;
        let mut run = 0usize;
        for i in m..b {
            if r.throttle[i] >= 0.95 {
                run += 1;
                if run >= sustain {
                    full = i + 1 - run;
                    break;
                }
            } else {
                run = 0;
            }
        }
        let exit = (full + (cfg.exit_margin_m / r.step_m) as usize).min(b).max(m + 1).min(n - 1);
        let search = entry.saturating_sub((150.0 / r.step_m) as usize).max(prev_apex_idx);
        let id = corners.len() as u32 + 1;
        corners.push(CornerDef {
            id,
            name: format!("V{id}"),
            search_start_m: r.dist[search],
            entry_m: r.dist[entry],
            apex_m: r.dist[m],
            exit_m: r.dist[exit],
        });
        prev_exit_idx = exit;
        prev_apex_idx = m;
    }
    corners
}

/// Métriques d'un tour sur un virage (fenêtre définie par la référence).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CornerMetrics {
    pub corner_id: u32,
    pub time_s: f32,
    pub brake_on_m: Option<f32>,
    pub brake_peak: f32,
    pub min_speed_kmh: f32,
    pub min_speed_at_m: f32,
    pub throttle_on_m: Option<f32>,
    pub exit_speed_kmh: f32,
    /// Durée de la montée des gaz de 20 % à 90 % (s) — brutalité de la remise des gaz.
    pub throttle_ramp_s: Option<f32>,
    /// Inversions de sens du volant (hystérésis 3°) — `None` si canal volant absent.
    pub steer_reversals: Option<u32>,
}

pub const BRAKE_ON: f32 = 0.10;
pub const THROTTLE_ON: f32 = 0.20;

pub fn corner_metrics(r: &Resampled, c: &CornerDef) -> CornerMetrics {
    let (s, e, x) = (r.idx(c.search_start_m), r.idx(c.entry_m), r.idx(c.exit_m));
    let apex_ref = r.idx(c.apex_m);

    // Vitesse minimale autour de l'apex de référence (±60 % de la fenêtre).
    let (mut min_i, mut min_v) = (apex_ref, f32::MAX);
    for i in e..=x {
        if r.speed_kmh[i] < min_v {
            min_v = r.speed_kmh[i];
            min_i = i;
        }
    }
    // Premier freinage entre le début de fenêtre et le point de vitesse minimale.
    let brake_on = (s..=min_i).find(|&i| r.brake[i] >= BRAKE_ON).map(|i| r.dist[i]);
    let brake_peak = (s..=min_i).map(|i| r.brake[i]).fold(0.0, f32::max);
    // Remise des gaz : première fois que l'accélérateur repasse THROTTLE_ON après le mini, et y reste 10 m.
    let hold = ((10.0 / r.step_m) as usize).max(1);
    let throttle_on_i = (min_i..=x).find(|&i| {
        (i..(i + hold).min(r.len())).all(|j| r.throttle[j] >= THROTTLE_ON)
    });
    let throttle_ramp_s = throttle_on_i.and_then(|i0| {
        let i90 = (i0..=x).find(|&j| r.throttle[j] >= 0.9)?;
        Some(r.time[i90] - r.time[i0])
    });
    let steer_reversals = r.steer_deg.as_ref().map(|st| count_reversals(&st[e..=x], 3.0));

    CornerMetrics {
        corner_id: c.id,
        time_s: r.time[x] - r.time[e],
        brake_on_m: brake_on,
        brake_peak,
        min_speed_kmh: min_v,
        min_speed_at_m: r.dist[min_i],
        throttle_on_m: throttle_on_i.map(|i| r.dist[i]),
        exit_speed_kmh: r.speed_kmh[x],
        throttle_ramp_s,
        steer_reversals,
    }
}

/// Nombre d'inversions de sens d'un signal avec hystérésis (évite de compter le bruit).
pub fn count_reversals(sig: &[f32], hyst: f32) -> u32 {
    let Some(&first) = sig.first() else { return 0 };
    let (mut lo, mut hi) = (first, first);
    let mut dir = 0i8; // 1 montant, -1 descendant
    let mut count = 0;
    for &v in sig {
        match dir {
            0 => {
                if v > lo + hyst {
                    dir = 1;
                    hi = v;
                } else if v < hi - hyst {
                    dir = -1;
                    lo = v;
                }
                lo = lo.min(v);
                hi = hi.max(v);
            }
            1 => {
                if v > hi {
                    hi = v;
                } else if v < hi - hyst {
                    dir = -1;
                    lo = v;
                    count += 1;
                }
            }
            _ => {
                if v < lo {
                    lo = v;
                } else if v > lo + hyst {
                    dir = 1;
                    hi = v;
                    count += 1;
                }
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reversals_ignore_noise_below_hysteresis() {
        let noisy: Vec<f32> = (0..100).map(|i| if i % 2 == 0 { 0.0 } else { 1.0 }).collect();
        assert_eq!(count_reversals(&noisy, 3.0), 0);
        let turn = [0.0, 10.0, 20.0, 30.0, 20.0, 10.0, 0.0];
        assert_eq!(count_reversals(&turn, 3.0), 1);
        let saw = [0.0, 20.0, 5.0, 25.0, 5.0, 20.0];
        assert_eq!(count_reversals(&saw, 3.0), 4);
    }
}
