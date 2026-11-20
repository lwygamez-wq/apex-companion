//! Débrief de course : où est parti le temps, et quoi viser à la prochaine course.
//!
//! Chaîne suivie pour chaque ligne : donnée enregistrée → mesure → interprétation → priorité. Rien
//! n'est estimé « à vue » : une perte n'apparaît que si elle se mesure (temps au tour contre ta médiane
//! de course, arrêts, énergie restante à l'arrivée, écart au leader enregistré). Ce qui n'a pas pu être
//! mesuré est listé dans `missing` au lieu d'être deviné.
//!
//! Calcul déterministe : aucune IA ici.

use crate::analysis::offtrack::OffTrackZone;
use crate::ingest::lmu::Recording;
use crate::model::{LapKind, LapMeta};
use serde::Serialize;

/// Un arrêt au stand, mesuré dans l'enregistrement.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StopFact {
    /// Tour de rentrée.
    pub lap: i32,
    /// Temps passé dans la voie des stands (s), dont à l'arrêt.
    pub lane_s: f64,
    pub still_s: f64,
    /// Énergie virtuelle à l'entrée et à la sortie (%) ; 0 si la voiture n'en a pas.
    pub energy_in: f32,
    pub energy_out: f32,
    pub fuel_in_l: f32,
    pub fuel_out_l: f32,
}

/// Ce que seul l'enregistrement d'origine contient (LMU) : arrêts, énergie, écart au leader.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct RaceExtras {
    pub stops: Vec<StopFact>,
    /// Énergie par tour en piste, médiane (%).
    pub energy_per_lap: Option<f32>,
    /// Énergie au plus bas du dernier tour (%).
    pub energy_finish: Option<f32>,
    /// Tours bouclés après le dernier plein.
    pub laps_after_last_fill: Option<u32>,
    /// (tour bouclé, retard sur le leader en s) quand il est enregistré et dans le même tour.
    pub gaps: Vec<(i32, f64)>,
    /// (tour bouclé, médiane des derniers tours des autres voitures de la catégorie en s, voitures
    /// comptées, meilleur de ces tours en s) : enregistré depuis Apex 0.39.
    pub class_laps: Vec<(i32, f64, u8, f64)>,
}

/// Lit arrêts, énergie et écarts dans un enregistrement LMU.
pub fn race_extras(r: &Recording) -> RaceExtras {
    let t = &r.ticks;
    if t.len() < 2 {
        return RaceExtras::default();
    }
    let mut stops = vec![];
    let mut cur: Option<(usize, f64)> = None;
    for i in 1..t.len() {
        let k = &t[i];
        if k.pit && cur.is_none() {
            cur = Some((i, 0.0));
        }
        if let Some((start, still)) = cur.as_mut() {
            if k.pit && k.speed < 0.5 {
                *still += k.et - t[i - 1].et;
            }
            if !k.pit {
                let a = &t[*start];
                stops.push(StopFact { lap: a.lap, lane_s: k.et - a.et, still_s: *still, energy_in: a.energy, energy_out: k.energy, fuel_in_l: a.fuel, fuel_out_l: k.fuel });
                cur = None;
            }
        }
    }
    // Sortie des stands au départ et simples traversées : pas des arrêts.
    let first_lap = t[0].lap;
    stops.retain(|s| s.lap > first_lap && (s.lane_s > 15.0 || s.still_s > 2.0));

    let mut starts: Vec<&crate::ingest::lmu::Tick> = vec![];
    for k in t {
        if starts.last().is_none_or(|p| p.lap != k.lap) {
            starts.push(k);
        }
    }
    let pit_laps: Vec<i32> = stops.iter().flat_map(|s| [s.lap, s.lap + 1]).collect();
    let mut per: Vec<f32> = starts.windows(2).filter(|w| !pit_laps.contains(&w[0].lap) && w[0].lap > first_lap).map(|w| w[0].energy - w[1].energy).filter(|d| *d > 0.3 && *d < 15.0).collect();
    per.sort_by(|a, b| a.total_cmp(b));
    let energy_per_lap = per.get(per.len() / 2).copied();
    let last_lap = t.last().map(|k| k.lap).unwrap_or(0);
    // Le jeu remet parfois 100 % après le drapeau : on prend le plus bas du dernier tour roulé.
    let low = t.iter().filter(|k| k.lap >= last_lap - 1 && !k.pit && k.energy > 0.0).map(|k| k.energy).fold(f32::MAX, f32::min);
    let energy_finish = (energy_per_lap.is_some() && low != f32::MAX).then_some(low);
    let laps_after_last_fill = stops.iter().rev().find(|s| s.energy_out > s.energy_in + 2.0 || s.fuel_out_l > s.fuel_in_l + 1.0).map(|s| starts.iter().filter(|k| k.lap > s.lap).count().saturating_sub(1) as u32);
    let gaps = r.laps.iter().filter(|e| e.laps_behind_leader == 0 && e.time_behind_leader > 0.0).map(|e| (e.total_laps, e.time_behind_leader)).collect();
    let class_laps = r.laps.iter().filter_map(|e| Some((e.total_laps, e.class_median_s?, e.class_cars, e.class_best_s.unwrap_or(0.0)))).filter(|c| c.2 > 0).collect();
    RaceExtras { stops, energy_per_lap, energy_finish, laps_after_last_fill, gaps, class_laps }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LossKind {
    /// Tours nettement plus lents que ta médiane (erreur, sortie, trafic).
    Errors,
    /// Arrêts au stand.
    Stops,
    /// Énergie en trop à l'arrivée : temps d'arrêt qui aurait pu être évité.
    Overfill,
    /// Rythme de course face au leader (écart enregistré).
    Pace,
    /// Rythme de course face aux autres voitures de ta catégorie (Apex 0.39).
    ClassPace,
}

/// Ton rythme face aux autres voitures de ta catégorie, tour par tour.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ClassPaceView {
    /// Tours comparés (tours normaux, hors stand et tours ratés) et voitures comptées au plus.
    pub laps: usize,
    pub cars: u8,
    /// Écart médian par tour avec la médiane de la catégorie (s ; positif = tu es plus lent).
    pub gap_s: f64,
    /// Tes tours plus rapides que la médiane de la catégorie.
    pub faster_laps: usize,
    /// Médiane de la catégorie sur ces tours et meilleur tour vu dans la catégorie (s).
    pub class_median_s: f64,
    pub class_best_s: f64,
    /// Écart médian sur la première et la seconde moitié de ces tours (s).
    pub first_half_gap_s: f64,
    pub second_half_gap_s: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TimeLoss {
    pub kind: LossKind,
    pub seconds: f64,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DebriefAction {
    pub title: String,
    pub detail: String,
    /// Temps mesuré que cette priorité vise (s).
    pub seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StintPace {
    pub from_lap: i32,
    pub to_lap: i32,
    pub laps: usize,
    pub median_s: f64,
    /// Médiane de la première et de la seconde moitié des tours propres du relais (s).
    pub first_half_s: f64,
    pub second_half_s: f64,
    /// Où le temps se perd d'une moitié du relais à l'autre (virage, phase), et usure des pneus.
    pub drift: Option<crate::analysis::stint_drift::StintDrift>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct RaceDebrief {
    /// Médiane de tes tours de course sans incident (s) : la référence de toutes les pertes.
    pub median_s: f64,
    pub clean_laps: usize,
    pub race_laps: usize,
    /// Pertes mesurées, de la plus grosse à la plus petite.
    pub losses: Vec<TimeLoss>,
    pub stints: Vec<StintPace>,
    pub stops: Vec<StopFact>,
    pub energy_per_lap: Option<f32>,
    pub energy_finish: Option<f32>,
    /// Tours d'énergie restants à l'arrivée.
    pub spare_laps: Option<f32>,
    /// Niveau qu'il aurait fallu régler au dernier plein (%), et niveau réglé.
    pub fill_needed: Option<f32>,
    pub fill_set: Option<f32>,
    /// Au plus trois priorités pour la prochaine course, chacune appuyée sur une perte mesurée.
    pub actions: Vec<DebriefAction>,
    /// Ce qui n'a pas pu être mesuré, et pourquoi.
    pub missing: Vec<String>,
    /// Rythme face à ta catégorie (courses enregistrées depuis Apex 0.39).
    pub class_pace: Option<ClassPaceView>,
    /// Usure anormale d'un pneu sur un tour, avec l'endroit et ce qui a été mesuré sur ce tour.
    #[serde(default)]
    pub wear_events: Vec<WearEvent>,
}

/// Usure d'un pneu sur un tour au moins 3 fois sa médiane de course (et plus d'un point de %).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WearEvent {
    pub lap: i32,
    pub wheel: &'static str,
    /// Usure sur ce tour et médiane des tours de la course (points de %).
    pub wear_pct: f32,
    pub median_pct: f32,
    /// Endroit où l'usure a le plus progressé dans le tour (virage), s'il est connu.
    pub at: Option<String>,
    /// Phrase complète : chiffres et ce qui a été mesuré (sortie, incident, blocage), ou ce qui manque.
    pub detail: String,
}

fn median(v: &mut [f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    Some(v[v.len() / 2])
}

fn secs(v: f64) -> String {
    format!("{v:.1}").replace('.', ",")
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n > 1 { many } else { one })
}

/// Seuil au-delà duquel un tour compte comme raté : 2 s ou 2 % de la médiane, le plus grand des deux.
pub fn error_threshold(median_s: f64) -> f64 {
    (median_s * 0.02).max(2.0)
}

/// Débrief d'une course. `laps` : tous les tours de la séance, dans l'ordre. `extras` : absent quand
/// l'enregistrement d'origine n'est pas lisible (ou hors LMU).
pub fn race_debrief(laps: &[LapMeta], off_tracks: &[OffTrackZone], extras: Option<&RaceExtras>) -> Option<RaceDebrief> {
    let timed: Vec<&LapMeta> = laps.iter().filter(|l| l.lap_time_s.is_some() && l.kind != LapKind::Incomplete).collect();
    let first = timed.iter().map(|l| l.number).min()?;
    // Tours de course « normaux » : lancés, hors stand, hors drapeau jaune, hors premier tour (départ).
    let racing: Vec<&LapMeta> = timed.iter().copied().filter(|l| l.kind == LapKind::Flying && !l.flags.pit && !l.flags.yellow_flag && l.number > first).collect();
    let mut undisturbed: Vec<f64> = racing.iter().filter(|l| !l.flags.off_track && !l.flags.incident).filter_map(|l| l.lap_time_s).collect();
    if undisturbed.len() < 3 {
        return None;
    }
    let med = median(&mut undisturbed)?;
    let thr = error_threshold(med);
    let mut d = RaceDebrief { median_s: med, clean_laps: undisturbed.len(), race_laps: timed.len(), ..Default::default() };

    // --- Tours ratés -------------------------------------------------------------------------
    let mut bad: Vec<(i32, f64, bool)> = racing.iter().filter_map(|l| l.lap_time_s.map(|t| (l.number, t - med, l.flags.off_track))).filter(|(_, e, _)| *e > thr).collect();
    bad.sort_by(|a, b| b.1.total_cmp(&a.1));
    let err_s: f64 = bad.iter().map(|b| b.1).sum();
    let off_n = bad.iter().filter(|b| b.2).count();
    // Endroit où ces tours sont le plus souvent sortis.
    let zone = off_tracks
        .iter()
        .filter_map(|z| z.corner.as_ref().map(|c| (c, z.laps.iter().filter(|n| bad.iter().any(|b| b.0 == **n as i32)).count())))
        .filter(|(_, n)| *n >= 2)
        .max_by_key(|(_, n)| *n);
    if !bad.is_empty() {
        let worst: Vec<String> = bad.iter().take(3).map(|b| format!("tour {} (+{} s)", b.0, secs(b.1))).collect();
        let mut detail = format!("{} à plus de {} s de ta médiane ({}). Les plus coûteux : {}.", plural(bad.len(), "tour", "tours"), secs(thr), fmt_lap(med), worst.join(", "));
        if off_n > 0 {
            detail += &format!(" {off_n} avec sortie de piste");
            if let Some((c, n)) = zone {
                detail += &format!(", dont {n} à {c}");
            }
            detail += ".";
        }
        d.losses.push(TimeLoss { kind: LossKind::Errors, seconds: err_s, title: format!("Tours ratés : {} s", secs(err_s)), detail });
    }

    // --- Arrêts ------------------------------------------------------------------------------
    let mut stop_s = 0.0;
    let mut n_stops = 0;
    for w in timed.windows(2) {
        if let (LapKind::InLap, LapKind::OutLap, Some(a), Some(b)) = (w[0].kind, w[1].kind, w[0].lap_time_s, w[1].lap_time_s) {
            let loss = a + b - 2.0 * med;
            if (5.0..900.0).contains(&loss) {
                stop_s += loss;
                n_stops += 1;
            }
        }
    }
    if let Some(x) = extras {
        d.stops = x.stops.clone();
        d.energy_per_lap = x.energy_per_lap;
        d.energy_finish = x.energy_finish;
    }
    if n_stops > 0 {
        let mut detail = format!("{} : tour de rentrée et tour de sortie comparés à deux tours à ta médiane.", plural(n_stops, "arrêt", "arrêts"));
        if let Some(s) = d.stops.iter().max_by(|a, b| a.still_s.total_cmp(&b.still_s)) {
            detail += &format!(" Voie des stands {} s, dont {} s à l'arrêt.", secs(s.lane_s), secs(s.still_s));
        }
        d.losses.push(TimeLoss { kind: LossKind::Stops, seconds: stop_s, title: format!("Arrêts : {} s", secs(stop_s)), detail });
    }

    // --- Énergie en trop à l'arrivée -------------------------------------------------------------
    let mut overfill: Option<(f64, f32, f32, f32)> = None;
    if let Some(x) = extras {
        if let (Some(c), Some(end)) = (x.energy_per_lap.filter(|c| *c > 0.0), x.energy_finish) {
            let spare = end / c;
            d.spare_laps = Some(spare);
            let fill = x.stops.iter().rev().find(|s| s.energy_out > s.energy_in + 2.0);
            if let (Some(s), Some(after)) = (fill, x.laps_after_last_fill) {
                // Tours courus après le plein, plus une demi-tour de marge.
                let needed = (after as f32 + 0.5) * c;
                d.fill_needed = Some(needed);
                d.fill_set = Some(s.energy_out);
                let extra = s.energy_out - needed;
                let added = s.energy_out - s.energy_in;
                // Au-delà d'un tour d'énergie en trop : temps d'arrêt évitable, au rythme de ton propre plein.
                if extra > c && added > 1.0 && s.still_s > 1.0 {
                    let sec = (s.still_s / added as f64) * extra as f64;
                    overfill = Some((sec, extra, needed, s.energy_out));
                    d.losses.push(TimeLoss {
                        kind: LossKind::Overfill,
                        seconds: sec,
                        title: format!("Plein trop généreux : au plus {} s", secs(sec)),
                        detail: format!(
                            "Arrivée avec {} % d'énergie, soit {} tours. Réglé {:.0} %, il fallait {:.0} % ({} tours courus ensuite et une demi-tour de marge). Ton plein a pris {} s pour {:.0} % : les {:.0} % en trop valent au plus {} s, moins si l'arrêt comptait aussi un changement de pneus.",
                            secs(end as f64),
                            secs(spare as f64),
                            s.energy_out,
                            needed,
                            after,
                            secs(s.still_s),
                            added,
                            extra,
                            secs(sec)
                        ),
                    });
                }
            }
        }
        if x.energy_per_lap.is_none() {
            d.missing.push("Énergie : non enregistrée sur cette course (version d'Apex trop ancienne, ou voiture sans énergie virtuelle).".into());
        }
    } else {
        d.missing.push("Arrêts détaillés et énergie : enregistrement d'origine introuvable ou hors Le Mans Ultimate.".into());
    }

    // --- Rythme face à la catégorie -----------------------------------------------------------------
    // Chaque tour normal comparé à la médiane des derniers tours des autres voitures de ta catégorie au
    // même passage. Il remplace l'écart au leader quand il existe : le leader est un cas à part.
    if let Some(cl) = extras.map(|x| &x.class_laps).filter(|c| !c.is_empty()) {
        let bad_laps: Vec<i32> = bad.iter().map(|b| b.0).collect();
        let pairs: Vec<(f64, f64, u8, f64)> = racing
            .iter()
            .filter(|l| !bad_laps.contains(&l.number) && !l.flags.off_track && !l.flags.incident)
            .filter_map(|l| cl.iter().find(|c| c.0 == l.number).map(|c| (l.lap_time_s.unwrap_or(0.0), c.1, c.2, c.3)))
            .collect();
        if pairs.len() >= 5 {
            let mut diffs: Vec<f64> = pairs.iter().map(|p| p.0 - p.1).collect();
            let half = diffs.len() / 2;
            let first_half_gap_s = median(&mut diffs[..half].to_vec()).unwrap_or(0.0);
            let second_half_gap_s = median(&mut diffs[half..].to_vec()).unwrap_or(0.0);
            let gap_s = median(&mut diffs).unwrap_or(0.0);
            let view = ClassPaceView {
                laps: pairs.len(),
                cars: pairs.iter().map(|p| p.2).max().unwrap_or(0),
                gap_s,
                faster_laps: pairs.iter().filter(|p| p.0 < p.1).count(),
                class_median_s: median(&mut pairs.iter().map(|p| p.1).collect::<Vec<_>>()).unwrap_or(0.0),
                class_best_s: pairs.iter().map(|p| p.3).filter(|b| *b > 0.0).fold(f64::MAX, f64::min).min(1e9),
                first_half_gap_s,
                second_half_gap_s,
            };
            if gap_s > 0.05 {
                let total = gap_s * racing.len() as f64;
                d.losses.push(TimeLoss {
                    kind: LossKind::ClassPace,
                    seconds: total,
                    title: format!("Rythme face à ta catégorie : {} s", secs(total)),
                    detail: format!("Sur un tour normal, tu es {} s plus lent que la médiane des autres voitures de ta catégorie ({} tours comparés, jusqu'à {} voitures), soit {} s sur {} tours de course.", secs(gap_s), view.laps, view.cars, secs(total), racing.len()),
                });
            }
            d.class_pace = Some(view);
        } else {
            d.missing.push(format!("Rythme face à ta catégorie : seulement {} tours comparables (il en faut 5).", pairs.len()));
        }
    } else if extras.is_some() {
        d.missing.push("Rythme face à ta catégorie : non enregistré sur cette course (enregistré depuis Apex 0.39).".into());
    }

    // --- Rythme face au leader -------------------------------------------------------------------
    match extras.filter(|_| d.class_pace.is_none()).map(|x| &x.gaps) {
        Some(g) if g.len() >= 8 => {
            let bad_laps: Vec<i32> = bad.iter().map(|b| b.0).collect();
            let pit: Vec<i32> = timed.iter().filter(|l| l.flags.pit || l.kind != LapKind::Flying).map(|l| l.number).collect();
            // Ce que le leader reprend sur un tour normal : médiane des variations d'écart, hors arrêts et tours ratés.
            let mut inc: Vec<f64> = g.windows(2).filter(|w| w[1].0 == w[0].0 + 1 && !pit.contains(&w[1].0) && !bad_laps.contains(&w[1].0)).map(|w| w[1].1 - w[0].1).filter(|v| v.abs() < thr * 3.0).collect();
            if inc.len() >= 6 {
                let per_lap = median(&mut inc).unwrap_or(0.0);
                let total = per_lap * racing.len() as f64;
                if per_lap > 0.05 {
                    d.losses.push(TimeLoss {
                        kind: LossKind::Pace,
                        seconds: total,
                        title: format!("Rythme face au leader : {} s", secs(total)),
                        detail: format!("Sur un tour normal, le leader te reprend {} s (médiane de {} tours, hors arrêts et tours ratés), soit {} s sur {} tours de course.", secs(per_lap), inc.len(), secs(total), racing.len()),
                    });
                }
            }
        }
        Some(_) => d.missing.push("Écart au leader : non enregistré sur cette course (versions avant la 0.35, ou leader à plus d'un tour).".into()),
        None => {}
    }
    d.losses.sort_by(|a, b| b.seconds.total_cmp(&a.seconds));

    // --- Rythme par relais -----------------------------------------------------------------------
    let mut cur: Vec<&LapMeta> = vec![];
    let flush = |cur: &mut Vec<&LapMeta>, out: &mut Vec<StintPace>| {
        let mut t: Vec<f64> = cur.iter().filter(|l| !l.flags.off_track && !l.flags.incident).filter_map(|l| l.lap_time_s).filter(|t| *t - med <= thr).collect();
        if t.len() >= 6 {
            // Ordre de course conservé : première moitié du relais contre seconde moitié.
            let half = t.len() / 2;
            let (fa, fb) = (median(&mut t[..half].to_vec()).unwrap_or(med), median(&mut t[half..].to_vec()).unwrap_or(med));
            let all = median(&mut t).unwrap_or(med);
            out.push(StintPace { from_lap: cur[0].number, to_lap: cur[cur.len() - 1].number, laps: cur.len(), median_s: all, first_half_s: fa, second_half_s: fb, drift: None });
        }
        cur.clear();
    };
    for l in &timed {
        if l.kind != LapKind::Flying || l.flags.pit {
            flush(&mut cur, &mut d.stints);
        } else if !l.flags.yellow_flag && l.number > first {
            cur.push(l);
        }
    }
    flush(&mut cur, &mut d.stints);

    // --- Priorités -------------------------------------------------------------------------------
    if err_s >= 5.0 {
        let mut detail = format!("{} ratés t'ont coûté {} s, soit {:.0} % d'un tour.", plural(bad.len(), "tour", "tours"), secs(err_s), err_s / med * 100.0);
        if let Some((c, n)) = zone {
            detail += &format!(" {n} sorties à {c} : c'est là qu'il faut garder de la marge.");
        } else if off_n > 0 {
            detail += &format!(" {off_n} avec sortie de piste.");
        }
        d.actions.push(DebriefAction { title: "Finir tous les tours".into(), detail, seconds: Some(err_s) });
    }
    // Une priorité seulement à partir de deux tours d'énergie en trop : en dessous, c'est la marge normale.
    if let Some((sec, extra, needed, set)) = overfill.filter(|o| d.energy_per_lap.is_some_and(|c| o.1 >= 2.0 * c)) {
        d.actions.push(DebriefAction {
            title: "Régler un plein plus juste".into(),
            detail: format!("{extra:.0} % d'énergie en trop à l'arrêt ({set:.0} % réglés pour {needed:.0} % nécessaires) : au plus {} s d'arrêt évitables.", secs(sec)),
            seconds: Some(sec),
        });
    }
    if let Some(s) = d.stints.iter().filter(|s| s.second_half_s - s.first_half_s > 0.4).max_by(|a, b| (a.second_half_s - a.first_half_s).total_cmp(&(b.second_half_s - b.first_half_s))) {
        let drop = s.second_half_s - s.first_half_s;
        d.actions.push(DebriefAction {
            title: "Tenir le rythme en fin de relais".into(),
            detail: format!("Relais des tours {} à {} : {} en première moitié, {} en seconde, soit +{} s par tour.", s.from_lap, s.to_lap, fmt_lap(s.first_half_s), fmt_lap(s.second_half_s), secs(drop)),
            seconds: Some(drop * (s.laps / 2) as f64),
        });
    }
    // Rythme : priorité à partir d'une demi-seconde par tour, avec la moitié de course où tu perds le plus.
    if let Some(c) = d.class_pace.as_ref().filter(|c| c.gap_s >= 0.5) {
        let where_ = if c.second_half_gap_s - c.first_half_gap_s > 0.3 {
            format!(" L'écart grandit en fin de course : {} s puis {} s par tour.", secs(c.first_half_gap_s), secs(c.second_half_gap_s))
        } else {
            String::new()
        };
        d.actions.push(DebriefAction {
            title: "Gagner du rythme sur ta catégorie".into(),
            detail: format!("{} s par tour de retard sur la médiane de ta catégorie ({} tours sur {} plus rapides qu'elle).{where_} Compare ton meilleur tour au tour de référence pour voir où.", secs(c.gap_s), c.faster_laps, c.laps),
            seconds: Some(c.gap_s * racing.len() as f64),
        });
    }
    d.actions.sort_by(|a, b| b.seconds.unwrap_or(0.0).total_cmp(&a.seconds.unwrap_or(0.0)));
    d.actions.truncate(3);
    Some(d)
}

/// `94.331` → `1:34.3`.
fn fmt_lap(s: f64) -> String {
    let m = (s / 60.0).floor();
    format!("{}:{:04.1}", m as i64, s - m * 60.0)
}

/// Tours d'un relais retenus pour son rythme (mêmes critères que `StintPace`), dans l'ordre.
pub fn stint_pace_laps<'a>(d: &RaceDebrief, s: &StintPace, laps: &'a [LapMeta]) -> Vec<&'a LapMeta> {
    let thr = error_threshold(d.median_s);
    laps.iter()
        .filter(|l| l.number >= s.from_lap && l.number <= s.to_lap && l.kind == LapKind::Flying)
        .filter(|l| !l.flags.pit && !l.flags.yellow_flag && !l.flags.off_track && !l.flags.incident)
        .filter(|l| l.lap_time_s.is_some_and(|t| t - d.median_s <= thr))
        .collect()
}

/// Range la dérive mesurée d'un relais et la cite dans la priorité « fin de relais » qui le concerne.
pub fn attach_drift(d: &mut RaceDebrief, idx: usize, mut drift: crate::analysis::stint_drift::StintDrift) {
    let Some(s) = d.stints.get_mut(idx) else { return };
    let key = format!("Relais des tours {} à {} :", s.from_lap, s.to_lap);
    drift.summary = crate::analysis::stint_drift::drift_sentence(&drift);
    if let Some(sentence) = drift.summary.clone() {
        if let Some(a) = d.actions.iter_mut().find(|a| a.title == "Tenir le rythme en fin de relais" && a.detail.starts_with(&key)) {
            a.detail = format!("{} {sentence}", a.detail);
        }
    }
    s.drift = Some(drift);
}

const WHEEL_NAMES: [&str; 4] = ["Avant gauche", "Avant droit", "Arrière gauche", "Arrière droit"];

/// Pics d'usure d'une course. `laps` : tours importés (usure par tour) ; `rec` : enregistrement d'origine
/// (usure à 60 Hz pour situer le pic, rotation des roues pour un blocage, 0.41+) ; `corners` : (nom,
/// début, fin) en mètres depuis la ligne.
pub fn wear_events(rec: &Recording, laps: &[LapMeta], corners: &[(String, f32, f32)]) -> Vec<WearEvent> {
    let rows: Vec<(&LapMeta, [f32; 4])> = laps.iter().filter_map(|l| l.tyre_wear_pct.map(|w| (l, w))).collect();
    if rows.len() < 5 {
        return vec![];
    }
    let len = rec.header.track_length_m as f64;
    let dist = crate::ingest::lmu::unwrapped_distance(&rec.ticks, &rec.anchors, len);
    let has_rot = rec.ticks.iter().any(|t| t.wheel_rot != [0.0; 4]);
    let on = |t: &crate::ingest::lmu::Tick| !t.pit && t.speed > 20.0;
    let mut out = vec![];
    for k in 0..4 {
        let mut v: Vec<f32> = rows.iter().map(|r| r.1[k]).collect();
        v.sort_by(f32::total_cmp);
        let med = v[v.len() / 2];
        for (l, w) in rows.iter().filter(|r| r.1[k] > 1.0 && r.1[k] >= 3.0 * med.max(0.05)) {
            // Endroit : plus forte baisse du pneu restant sur une seconde, dans ce tour.
            let idx: Vec<usize> = (0..rec.ticks.len()).filter(|&i| rec.ticks[i].lap == l.number && rec.ticks[i].wear[k] > 0.0).collect();
            let win = (rec.header.hz.max(1.0)) as usize;
            let at = idx
                .windows(win.max(2))
                .map(|w| (w[0], rec.ticks[w[0]].wear[k] - rec.ticks[*w.last().unwrap()].wear[k]))
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .filter(|(_, d)| *d > 0.0)
                .map(|(i, _)| dist[i].rem_euclid(len) as f32)
                .map(|x| corners.iter().find(|c| x >= c.1 && x <= c.2).map(|c| c.0.clone()).unwrap_or_else(|| format!("{x:.0} m")));
            let mut seen: Vec<String> = vec![];
            if l.flags.off_track {
                seen.push("sortie de piste".into());
            }
            if l.flags.incident {
                seen.push("contact".into());
            }
            if let Some(r) = has_rot.then(|| crate::analysis::setup_diag::wheel_radius(rec, k, &on)).flatten() {
                let lock = idx.iter().filter(|&&i| crate::analysis::setup_diag::brake_slip(&rec.ticks[i], k, r) > 0.2).count();
                if lock as f32 >= rec.header.hz * 0.1 {
                    seen.push(format!("roue bloquée {} s au freinage", format!("{:.1}", lock as f32 / rec.header.hz).replace('.', ",")));
                }
            }
            let cause = if seen.is_empty() {
                if has_rot { "Ni sortie, ni contact, ni blocage mesuré sur ce tour.".to_string() } else { "Ni sortie ni contact enregistrés ; blocage non mesurable (rotation des roues enregistrée à partir d'Apex 0.41).".to_string() }
            } else {
                let list = match seen.len() {
                    1 => seen[0].clone(),
                    n => format!("{} et {}", seen[..n - 1].join(", "), seen[n - 1]),
                };
                let mut c = format!("{list} sur ce tour.");
                c.replace_range(..1, &c[..1].to_uppercase());
                c
            };
            let fr = |x: f32| format!("{x:.2}").replace('.', ",");
            let place = at
                .as_ref()
                .map(|a| match a {
                    a if a.ends_with(" m") => format!(", surtout à {a} du départ"),
                    a if a.starts_with("Virages ") => format!(", surtout aux {}", a.to_lowercase()),
                    a if a.starts_with("Virage ") => format!(", surtout au {}", a.to_lowercase()),
                    a => format!(", surtout à {a}"),
                })
                .unwrap_or_default();
            out.push(WearEvent {
                lap: l.number,
                wheel: WHEEL_NAMES[k],
                wear_pct: w[k],
                median_pct: med,
                at,
                detail: format!("{} : {} % d'usure au tour {} (médiane {} %){place}. {cause}", WHEEL_NAMES[k], fr(w[k]), l.number, fr(med)),
            });
        }
    }
    out.sort_by_key(|e| e.lap);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LapFlags, Validity, ValiditySource};

    // Tours synthétiques (inventés pour le test, pas de la télémétrie réelle).
    fn lap(n: i32, kind: LapKind, t: f64, off: bool) -> LapMeta {
        LapMeta { number: n, kind, lap_time_s: Some(t), sector_times_s: vec![], flags: LapFlags { off_track: off, pit: kind != LapKind::Flying, ..Default::default() }, validity: Validity::Valid, validity_source: ValiditySource::Derived, fuel_start_l: None, fuel_used_l: None, energy_used_pct: None, tyre_wear_pct: None }
    }

    fn race() -> Vec<LapMeta> {
        let mut v = vec![lap(1, LapKind::Flying, 110.0, false)];
        // Relais 1 : 100,0 s puis 100,6 s en seconde moitié ; tour 6 raté (+12 s, sortie), tour 9 (+3 s).
        for n in 2..=13 {
            let base = if n <= 7 { 100.0 } else { 100.6 };
            let t = match n {
                6 => 112.0,
                9 => 103.4,
                _ => base,
            };
            v.push(lap(n, LapKind::Flying, t, n == 6));
        }
        v.push(lap(14, LapKind::InLap, 115.0, false));
        v.push(lap(15, LapKind::OutLap, 150.0, false));
        for n in 16..=27 {
            v.push(lap(n, LapKind::Flying, 100.0, false));
        }
        v
    }

    #[test]
    fn measures_where_the_time_went() {
        let zones = [OffTrackZone { at_m: 800.0, corner: Some("Tosa".into()), laps: vec![6] }];
        let extras = RaceExtras {
            stops: vec![StopFact { lap: 14, lane_s: 70.0, still_s: 36.0, energy_in: 2.0, energy_out: 62.0, fuel_in_l: 2.0, fuel_out_l: 50.0 }],
            energy_per_lap: Some(4.0),
            energy_finish: Some(14.0),
            laps_after_last_fill: Some(12),
            gaps: vec![],
            class_laps: vec![],
        };
        let d = race_debrief(&race(), &zones, Some(&extras)).unwrap();
        // Médiane des tours sans incident ; seuil d'un tour raté : 2 s (2 % de 100 s).
        assert!((d.median_s - 100.0).abs() < 1e-9);
        assert_eq!(error_threshold(100.0), 2.0);
        let get = |k| d.losses.iter().find(|l| l.kind == k).unwrap();
        // Tours 6 (+12) et 9 (+3,4) : 15,4 s. Le premier tour (départ) ne compte pas.
        assert!((get(LossKind::Errors).seconds - 15.4).abs() < 1e-6, "{:?}", get(LossKind::Errors));
        assert!(get(LossKind::Errors).detail.contains("tour 6 (+12,0 s)"));
        // Arrêt : 115 + 150 − 2 × 100 = 65 s.
        assert!((get(LossKind::Stops).seconds - 65.0).abs() < 1e-6);
        // Plein : 12 tours ensuite + demi-tour = 50 % nécessaires, 62 % réglés : 12 % en trop, à 36 s pour 60 % = 7,2 s.
        assert_eq!((d.fill_needed, d.fill_set, d.spare_laps), (Some(50.0), Some(62.0), Some(3.5)));
        assert!((get(LossKind::Overfill).seconds - 7.2).abs() < 1e-6);
        // Pertes triées, priorités : erreurs, plein, puis fin de relais (+0,6 s par tour sur le relais 1).
        assert_eq!(d.losses[0].kind, LossKind::Stops);
        let titles: Vec<_> = d.actions.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(titles, ["Finir tous les tours", "Régler un plein plus juste", "Tenir le rythme en fin de relais"]);
        assert_eq!(d.stints.len(), 2);
        assert!((d.stints[0].second_half_s - d.stints[0].first_half_s - 0.6).abs() < 1e-9);
        // Toute priorité cite un chiffre.
        assert!(d.actions.iter().all(|a| a.detail.chars().any(|c| c.is_ascii_digit())));
        // Dérive du relais 1 mesurée : citée dans la priorité « fin de relais », rangée dans le relais.
        let laps = race();
        let kept = stint_pace_laps(&d, &d.stints[0], &laps);
        // Relais 1 (tours 2 à 13) sans les tours ratés : 6 (sortie, +12 s) et 9 (+3,4 s).
        assert_eq!(kept.iter().map(|l| l.number).collect::<Vec<_>>(), [2, 3, 4, 5, 7, 8, 10, 11, 12, 13]);
        let drift = crate::analysis::stint_drift::StintDrift {
            laps_first: 3,
            laps_second: 3,
            by_phase: vec![(crate::analysis::phases::CornerPhase::Sortie, 0.4)],
            top: vec![crate::analysis::stint_drift::PhaseDrift { corner: "Rivazza".into(), phase: crate::analysis::phases::CornerPhase::Sortie, drift_s: 0.3 }],
            total_s: 0.4,
            wear_front_pct: None,
            wear_rear_pct: None,
            summary: None,
        };
        let mut d = d;
        attach_drift(&mut d, 0, drift);
        let a = d.actions.iter().find(|a| a.title == "Tenir le rythme en fin de relais").unwrap();
        assert!(a.detail.ends_with("Rivazza en sortie +0,30 s. Sur tout le tour, 0,40 s en sortie (100 % de la perte)."), "{}", a.detail);
        assert!(d.stints[0].drift.as_ref().unwrap().summary.as_ref().unwrap().starts_with("Ce qui ralentit le plus"));
    }

    /// Données synthétiques : 6 tours, l'avant droit s'use de 3,5 % au tour 4 (0,45 % ailleurs), d'un coup
    /// entre 1000 et 1050 m, sur un tour avec sortie de piste.
    #[test]
    fn wear_spike_is_an_event_with_place_and_cause() {
        use crate::ingest::lmu::{Anchor, Header, Recording, Tick};
        let mut laps: Vec<LapMeta> = (1..=6).map(|n| lap(n, LapKind::Flying, 100.0, n == 4)).collect();
        for l in &mut laps {
            l.tyre_wear_pct = Some([0.4, if l.number == 4 { 3.5 } else { 0.45 }, 0.5, 0.5]);
        }
        // Circuit de 2000 m à 50 m/s, 60 Hz ; pneu restant avant droit.
        let mut ticks = vec![];
        let mut left = 100.0f32;
        for i in 0..(6 * 40 * 60) {
            let et = i as f64 / 60.0;
            let x = et * 50.0;
            let lap_n = (x / 2000.0) as i32 + 1;
            let d = x % 2000.0;
            left -= if lap_n == 4 && (1000.0..1050.0).contains(&d) { 3.05 / 60.0 } else { 0.45 / 2400.0 };
            ticks.push(Tick { et, lap: lap_n, speed: 50.0, wear: [90.0, left, 90.0, 90.0], ..Default::default() });
        }
        let rec = Recording { header: Header { track_length_m: 2000.0, hz: 60.0, ..Default::default() }, anchors: (0..240).map(|k| Anchor { et: k as f64, lap_dist: ((k as f32 * 50.0) % 2000.0), total_laps: (k as f32 * 50.0 / 2000.0) as i32 }).collect(), ticks, laps: vec![], wheels: vec![], setup: None };
        let mut laps2 = laps.clone();
        laps2[3].flags.incident = true;
        let ev2 = wear_events(&rec, &laps2, &[("Virage 4".into(), 950.0, 1150.0)]);
        assert!(ev2[0].detail.ends_with("surtout au virage 4. Sortie de piste et contact sur ce tour."), "{}", ev2[0].detail);
        let ev = wear_events(&rec, &laps, &[("Rettifilo".into(), 950.0, 1150.0)]);
        assert_eq!(ev.len(), 1, "{ev:?}");
        assert_eq!((ev[0].lap, ev[0].wheel, ev[0].at.as_deref()), (4, "Avant droit", Some("Rettifilo")));
        assert_eq!(ev[0].detail, "Avant droit : 3,50 % d'usure au tour 4 (médiane 0,45 %), surtout à Rettifilo. Sortie de piste sur ce tour.");
        // Usure régulière : aucun événement.
        for l in &mut laps {
            l.tyre_wear_pct = Some([0.4, 0.45, 0.5, 0.5]);
        }
        assert!(wear_events(&rec, &laps, &[]).is_empty());
    }

    #[test]
    fn says_nothing_when_nothing_is_measurable() {
        // Course propre et régulière : aucune perte, aucune priorité inventée.
        let laps: Vec<LapMeta> = (1..=12).map(|n| lap(n, LapKind::Flying, 100.0 + (n % 2) as f64 * 0.2, false)).collect();
        let d = race_debrief(&laps, &[], None).unwrap();
        assert!(d.losses.is_empty() && d.actions.is_empty());
        assert_eq!(d.missing.len(), 1);
        // Trop peu de tours pour une médiane : pas de débrief du tout.
        assert!(race_debrief(&laps[..3], &[], None).is_none());
    }

    #[test]
    fn pace_against_the_leader_needs_recorded_gaps() {
        // Le leader reprend 0,8 s par tour ; tour 6 (raté) exclu de la mesure.
        let gaps: Vec<(i32, f64)> = (1..=13).map(|n| (n, 5.0 + 0.8 * n as f64 + if n >= 6 { 12.0 } else { 0.0 })).collect();
        let d = race_debrief(&race(), &[], Some(&RaceExtras { gaps, ..Default::default() })).unwrap();
        let p = d.losses.iter().find(|l| l.kind == LossKind::Pace).unwrap();
        // 24 tours de course (tours 2 à 13 et 16 à 27 : hors premier tour et tours de stand) × 0,8 s.
        assert!((p.seconds - 0.8 * 24.0).abs() < 1e-6, "{p:?}");
        let d = race_debrief(&race(), &[], Some(&RaceExtras::default())).unwrap();
        assert!(d.losses.iter().all(|l| l.kind != LossKind::Pace));
        assert!(d.missing.iter().any(|m| m.contains("Écart au leader")));
    }

    #[test]
    fn pace_against_the_class_replaces_the_leader_gap() {
        // Catégorie synthétique : médiane à 99,4 s sur les 12 premiers tours, 99,0 s ensuite (tes tours
        // normaux : 100,0 s et 100,6 s). Leader enregistré aussi : il ne compte plus.
        let class_laps: Vec<(i32, f64, u8, f64)> = (2..=27).map(|n| (n, if n <= 13 { 99.4 } else { 99.0 }, 7, 98.1)).collect();
        let gaps: Vec<(i32, f64)> = (1..=27).map(|n| (n, 2.0 * n as f64)).collect();
        let d = race_debrief(&race(), &[], Some(&RaceExtras { class_laps, gaps, ..Default::default() })).unwrap();
        let c = d.class_pace.clone().unwrap();
        // Tours comparés : 2 à 13 et 16 à 27, sans les tours 6 et 9 (ratés) = 22.
        assert_eq!((c.laps, c.cars, c.faster_laps), (22, 7, 0));
        // Écarts : tours 2-7 (5 tours, 0,6 s), tours 8-13 (4 tours, 1,2 s), tours 16-27 (12 tours, 1,0 s) → médiane 1,0 s.
        assert!((c.gap_s - 1.0).abs() < 1e-9, "{c:?}");
        assert_eq!(c.class_best_s, 98.1);
        let p = d.losses.iter().find(|l| l.kind == LossKind::ClassPace).unwrap();
        assert!((p.seconds - 24.0).abs() < 1e-6, "{p:?}");
        assert!(d.losses.iter().all(|l| l.kind != LossKind::Pace));
        assert!(d.actions.iter().any(|a| a.title == "Gagner du rythme sur ta catégorie" && a.detail.starts_with("1,0 s par tour")), "{:?}", d.actions);
        // Course enregistrée avant la 0.39 : c'est dit, rien n'est deviné.
        let d = race_debrief(&race(), &[], Some(&RaceExtras::default())).unwrap();
        assert!(d.class_pace.is_none());
        assert!(d.missing.iter().any(|m| m.contains("enregistré depuis Apex 0.39")));
    }
}
