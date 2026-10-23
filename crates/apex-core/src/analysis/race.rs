//! Coach de course : lecture d'une course à partir de ce que contient le fichier iRacing du pilote
//! (grille, positions au passage de la ligne, incidents localisés, temps au tour, temps de
//! qualification du plateau). Le fichier ne contient pas les autres voitures en piste : pas d'écarts
//! ni de lieux de dépassement, seulement les positions tour par tour.

use crate::model::{IncidentKind, LapKind, LapMeta, RaceData};
use serde::Serialize;

/// Incident replacé sur le circuit (nom du virage s'il est connu).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IncidentView {
    pub lap: i32,
    pub dist_m: f32,
    pub corner: Option<String>,
    pub kind: IncidentKind,
    pub points: u32,
    /// « Sortie de piste (1x) », « Accrochage (4x) »…
    pub label: String,
    /// Temps perdu sur ce tour par rapport à ton tour médian (s), si mesurable.
    pub lap_loss_s: Option<f64>,
}

/// Plusieurs incidents au même endroit : le point noir de la course.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IncidentHotspot {
    pub corner: String,
    pub count: usize,
    pub points: u32,
    pub laps: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RacePace {
    pub laps: usize,
    pub best_s: f64,
    pub median_s: f64,
    /// Écart-type des tours propres (s).
    pub spread_s: f64,
    /// Moyenne du premier et du dernier tiers des tours propres (s).
    pub first_third_s: f64,
    pub last_third_s: f64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RaceInsightKind {
    Danger,
    Warning,
    Good,
    Info,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RaceInsight {
    pub kind: RaceInsightKind,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RaceReport {
    pub field_size: u32,
    pub start_position: Option<u32>,
    pub finish_position: Option<u32>,
    /// Places gagnées (positif) ou perdues (négatif) entre la grille et l'arrivée.
    pub positions_gained: Option<i32>,
    /// (tour, position) en commençant par (0, grille).
    pub positions: Vec<(i32, u32)>,
    pub incidents: Vec<IncidentView>,
    pub incident_total: u32,
    pub hotspots: Vec<IncidentHotspot>,
    pub pace: Option<RacePace>,
    pub my_irating: Option<u32>,
    pub field_irating: Option<u32>,
    pub irating_rank: Option<u32>,
    pub my_qualify_s: Option<f64>,
    pub pole_s: Option<f64>,
    /// Position de qualification qu'aurait donnée ton meilleur tour de course.
    pub race_lap_as_qualify: Option<u32>,
    pub headline: String,
    pub insights: Vec<RaceInsight>,
}

/// Rayon (m) dans lequel un incident est rattaché au virage nommé le plus proche.
const CORNER_RADIUS_M: f32 = 160.0;

fn kind_label(k: IncidentKind) -> &'static str {
    match k {
        IncidentKind::LossOfControl => "Perte de contrôle",
        IncidentKind::OffTrack => "Sortie de piste",
        IncidentKind::WorldContact => "Contact avec le décor",
        IncidentKind::WorldCollision => "Choc avec le décor",
        IncidentKind::CarContact => "Contact avec une voiture",
        IncidentKind::CarCollision => "Accrochage",
        IncidentKind::Other => "Incident",
    }
}

fn nearest_corner(names: &[(f32, String)], d: f32, track_len: f32) -> Option<String> {
    names
        .iter()
        .map(|(m, n)| {
            let mut dd = (m - d).abs();
            if track_len > 0.0 {
                dd = dd.min(track_len - dd);
            }
            (dd, n)
        })
        .filter(|(dd, _)| *dd <= CORNER_RADIUS_M)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, n)| n.clone())
}

fn median(v: &mut [f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 })
}

fn place(p: u32) -> String {
    format!("P{p}")
}

/// Analyse d'une course. `laps` : tous les tours de la session ; `names` : virages (distance, nom).
pub fn race_report(race: &RaceData, laps: &[LapMeta], names: &[(f32, String)], track_len: f32) -> RaceReport {
    // Rythme : tours lancés propres, premier tour exclu (départ arrêté ou lancé, trafic).
    let clean: Vec<(i32, f64)> = laps.iter().filter(|l| l.is_clean() && l.number > 1).filter_map(|l| Some((l.number, l.lap_time_s?))).collect();
    let mut times: Vec<f64> = clean.iter().map(|x| x.1).collect();
    let med = median(&mut times.clone());
    let pace = med.filter(|_| clean.len() >= 3).map(|median_s| {
        let mean = times.iter().sum::<f64>() / times.len() as f64;
        let spread_s = (times.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / times.len() as f64).sqrt();
        let k = (clean.len() / 3).max(1);
        let avg = |s: &[(i32, f64)]| s.iter().map(|x| x.1).sum::<f64>() / s.len() as f64;
        times.sort_by(f64::total_cmp);
        RacePace { laps: clean.len(), best_s: times[0], median_s, spread_s, first_third_s: avg(&clean[..k]), last_third_s: avg(&clean[clean.len() - k..]) }
    });
    let lap_time = |n: i32| laps.iter().find(|l| l.number == n && l.kind == LapKind::Flying).and_then(|l| l.lap_time_s);

    let incidents: Vec<IncidentView> = race
        .incidents
        .iter()
        // Contacts à 0x sans conséquence : listés seulement s'ils sont seuls sur le tour.
        .map(|e| IncidentView {
            lap: e.lap,
            dist_m: e.dist_m,
            corner: nearest_corner(names, e.dist_m, track_len),
            kind: e.kind,
            points: e.points,
            label: format!("{} ({}x)", kind_label(e.kind), e.points),
            lap_loss_s: match (lap_time(e.lap), med) {
                (Some(t), Some(m)) if e.lap > 1 => Some(t - m).filter(|x| *x > 0.0),
                _ => None,
            },
        })
        .collect();

    // Points noirs : plusieurs incidents comptés au même virage.
    let mut hotspots: Vec<IncidentHotspot> = vec![];
    for i in incidents.iter().filter(|i| i.points > 0 || i.kind != IncidentKind::CarContact) {
        let Some(c) = &i.corner else { continue };
        match hotspots.iter_mut().find(|h| &h.corner == c) {
            Some(h) => {
                if !h.laps.contains(&i.lap) {
                    h.count += 1;
                    h.laps.push(i.lap);
                }
                h.points += i.points;
            }
            None => hotspots.push(IncidentHotspot { corner: c.clone(), count: 1, points: i.points, laps: vec![i.lap] }),
        }
    }
    hotspots.retain(|h| h.count >= 2);
    hotspots.sort_by(|a, b| b.count.cmp(&a.count).then(b.points.cmp(&a.points)));

    let mut positions = vec![];
    if let Some(g) = race.start_position {
        positions.push((0, g));
    }
    positions.extend(race.positions.iter().copied());
    let total_laps = race.positions.last().map(|p| p.0).unwrap_or(0).max(laps.iter().map(|l| l.number).max().unwrap_or(0));
    // Dernier tour (arrivée) : iRacing ne recompte plus les tours après le drapeau à damier.
    if let (Some(f), Some(&(last, _))) = (race.finish_position, positions.last()) {
        if last < total_laps {
            positions.push((total_laps, f));
        }
    }
    let positions_gained = match (race.start_position, race.finish_position) {
        (Some(s), Some(f)) => Some(s as i32 - f as i32),
        _ => None,
    };
    let race_lap_as_qualify = pace.as_ref().filter(|_| !race.qualify_times.is_empty()).map(|p| race.qualify_times.iter().filter(|&&q| q < p.best_s).count() as u32 + 1);

    let mut insights: Vec<RaceInsight> = vec![];
    // Départ.
    if let (Some(g), Some(&(_, p1))) = (race.start_position, race.positions.first()) {
        let d = g as i32 - p1 as i32;
        if d <= -2 {
            insights.push(RaceInsight {
                kind: RaceInsightKind::Warning,
                title: format!("Départ : {} places perdues au premier tour", -d),
                detail: format!("{} sur la grille, {} après un tour. Le premier tour se joue au freinage du virage 1 et dans le peloton : vise la sortie propre plutôt que l'attaque.", place(g), place(p1)),
            });
        } else if d >= 2 {
            insights.push(RaceInsight { kind: RaceInsightKind::Good, title: format!("Bon départ : {d} places gagnées au premier tour"), detail: format!("{} sur la grille, {} après un tour.", place(g), place(p1)) });
        }
    }
    // Point noir.
    if let Some(h) = hotspots.first() {
        insights.push(RaceInsight {
            kind: RaceInsightKind::Danger,
            title: format!("{} : {} incidents, {}x", h.corner, h.count, h.points),
            detail: format!(
                "Incidents aux tours {}. Même erreur, même endroit : en course, freine un repère plus tôt et garde de la marge en sortie ici ; le temps perdu en restant en piste est bien plus faible que celui d'une sortie.",
                h.laps.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(", ")
            ),
        });
    }
    // Accrochages.
    let collisions: Vec<&IncidentView> = incidents.iter().filter(|i| i.kind == IncidentKind::CarCollision).collect();
    if !collisions.is_empty() {
        let c = collisions[0];
        insights.push(RaceInsight {
            kind: RaceInsightKind::Danger,
            title: format!("{} accrochage(s) avec une autre voiture", collisions.len()),
            detail: format!(
                "Tour {}{} : {}x. Un accrochage coûte 4x d'un coup et souvent des places : en bataille, laisse la place quand tu n'es pas à hauteur des roues avant au point de corde.",
                c.lap,
                c.corner.as_deref().map(|n| format!(", {n}")).unwrap_or_default(),
                c.points
            ),
        });
    }
    // Fin de course : dernier quart.
    if total_laps >= 8 {
        let from = total_laps - (total_laps / 4).max(2) + 1;
        let late_pts: u32 = incidents.iter().filter(|i| i.lap >= from).map(|i| i.points).sum();
        let before = race.positions.iter().rev().find(|p| p.0 < from).map(|p| p.1).or(race.start_position);
        let lost = match (before, race.finish_position) {
            (Some(b), Some(f)) if f > b => f - b,
            _ => 0,
        };
        if late_pts >= 3 || lost >= 2 {
            insights.push(RaceInsight {
                kind: RaceInsightKind::Warning,
                title: format!("Fin de course : {late_pts}x{}", if lost > 0 { format!(" et {lost} place(s) perdue(s)") } else { String::new() }),
                detail: format!(
                    "À partir du tour {from}. La concentration baisse en fin de course : c'est là qu'on perd ce qu'on a gagné. Sur les derniers tours, protège ta position et roule à 98 %."
                ),
            });
        }
    }
    // Remontée.
    if let Some(g) = positions_gained.filter(|g| *g >= 3) {
        insights.push(RaceInsight {
            kind: RaceInsightKind::Good,
            title: format!("{g} places gagnées sur la course"),
            detail: format!("De {} à {}.", race.start_position.map(place).unwrap_or_default(), race.finish_position.map(place).unwrap_or_default()),
        });
    }
    // Qualification : écart à la pole et ce qu'aurait donné le rythme course.
    if let (Some(q), Some(pole), Some(rq), Some(g)) = (race.my_qualify_s, race.pole_s, race_lap_as_qualify, race.start_position) {
        if rq + 3 <= g {
            insights.push(RaceInsight {
                kind: RaceInsightKind::Warning,
                title: format!("Qualification : tu pars {} alors que ton rythme vaut {}", place(g), place(rq)),
                detail: format!(
                    "Ton tour de qualif est à {:+.2} s de la pole ; ton meilleur tour en course t'aurait placé {}. {}",
                    q - pole,
                    place(rq),
                    if g * 2 > race.field_size {
                        "Une qualif propre t'épargne le trafic du fond de grille et les accrochages du premier tour."
                    } else {
                        "Partir devant, c'est rouler dans l'air libre dès le départ : prépare un vrai tour de qualif (pneus chauds, piste libre devant)."
                    }
                ),
            });
        }
    }
    // Rythme.
    if let Some(p) = &pace {
        if p.last_third_s - p.first_third_s >= 0.4 {
            insights.push(RaceInsight {
                kind: RaceInsightKind::Info,
                title: format!("Baisse de rythme en fin de course : {:+.2} s par tour", p.last_third_s - p.first_third_s),
                detail: "Pneus, piste qui chauffe ou fatigue : compare tes premiers et derniers tours propres dans l'analyse pour voir où.".into(),
            });
        } else if p.spread_s <= 0.4 && p.laps >= 5 {
            insights.push(RaceInsight {
                kind: RaceInsightKind::Good,
                title: format!("Rythme très régulier : ±{:.2} s sur {} tours propres", p.spread_s, p.laps),
                detail: "La régularité est acquise ; la marge est maintenant dans la vitesse pure (comparaison avec une référence plus rapide).".into(),
            });
        }
    }
    insights.sort_by_key(|i| i.kind);

    let headline = match (race.start_position, race.finish_position) {
        (Some(s), Some(f)) => format!(
            "{} → {} sur {} pilotes, {}x{}",
            place(s),
            place(f),
            race.field_size,
            race.incident_total,
            hotspots.first().map(|h| format!(" ; point noir : {}", h.corner)).unwrap_or_default()
        ),
        _ => format!("{} pilotes, {}x", race.field_size, race.incident_total),
    };

    RaceReport {
        field_size: race.field_size,
        start_position: race.start_position,
        finish_position: race.finish_position,
        positions_gained,
        positions,
        incidents,
        incident_total: race.incident_total,
        hotspots,
        pace,
        my_irating: race.my_irating,
        field_irating: race.field_irating,
        irating_rank: race.irating_rank,
        my_qualify_s: race.my_qualify_s,
        pole_s: race.pole_s,
        race_lap_as_qualify,
        headline,
        insights,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LapFlags, RaceIncident, Validity, ValiditySource};

    fn lap(n: i32, t: f64, clean: bool) -> LapMeta {
        LapMeta {
            number: n,
            kind: LapKind::Flying,
            lap_time_s: Some(t),
            sector_times_s: vec![],
            flags: LapFlags { off_track: !clean, ..Default::default() },
            validity: if clean { Validity::Valid } else { Validity::Invalid },
            validity_source: ValiditySource::Derived,
            fuel_start_l: None,
            fuel_used_l: None,
        }
    }

    fn inc(lap: i32, d: f32, kind: IncidentKind, points: u32) -> RaceIncident {
        RaceIncident { lap, dist_m: d, kind, points, time_s: lap as f64 * 88.0 }
    }

    /// Reproduit la course réelle d'Interlagos du 17/10 (valeurs extraites du .ibt).
    #[test]
    fn interlagos_race_of_17_october() {
        let race = RaceData {
            field_size: 23,
            start_position: Some(19),
            finish_position: Some(11),
            positions: vec![(1, 21), (2, 18), (3, 17), (4, 14), (5, 15), (6, 15), (7, 17), (8, 17), (9, 15), (10, 14), (11, 14), (12, 12), (13, 12), (14, 9), (15, 9), (16, 9), (17, 9), (18, 9), (19, 11)],
            incidents: vec![
                inc(1, 1436.6, IncidentKind::OffTrack, 1),
                inc(7, 1462.0, IncidentKind::OffTrack, 1),
                inc(11, 871.7, IncidentKind::OffTrack, 1),
                inc(15, 2759.3, IncidentKind::OffTrack, 1),
                inc(19, 1437.4, IncidentKind::OffTrack, 1),
                inc(19, 1451.1, IncidentKind::LossOfControl, 1),
                inc(20, 2156.0, IncidentKind::CarContact, 0),
                inc(20, 2181.7, IncidentKind::CarCollision, 4),
            ],
            incident_total: 10,
            my_irating: Some(1695),
            field_irating: Some(1820),
            irating_rank: Some(17),
            my_qualify_s: Some(93.4811),
            pole_s: Some(86.0543),
            qualify_times: vec![86.05, 86.22, 86.47, 86.55, 86.68, 86.73, 86.77, 87.12, 87.13, 87.17, 87.22, 87.28, 87.47, 87.55, 87.56, 88.03, 88.94, 89.17, 93.48],
        };
        let names: Vec<(f32, String)> = [(326.0, "S do Senna"), (1379.0, "Descida do Lago"), (2103.0, "Ferradura"), (2292.0, "Laranjinha"), (2415.0, "Pinheirinho"), (2696.0, "Bico de Pato"), (3194.0, "Junção")]
            .iter()
            .map(|(d, n)| (*d, n.to_string()))
            .collect();
        let mut laps: Vec<LapMeta> = (1..=20).map(|n| lap(n, 87.9 + (n % 3) as f64 * 0.15, true)).collect();
        for n in [1, 7, 11, 15, 19, 20] {
            laps[n as usize - 1] = lap(n, 89.5, false);
        }
        let r = race_report(&race, &laps, &names, 4309.0);
        assert_eq!(r.positions_gained, Some(8));
        assert_eq!(r.positions.first(), Some(&(0, 19)));
        assert_eq!(r.positions.last(), Some(&(20, 11)), "arrivée placée au dernier tour");
        assert_eq!(r.hotspots[0].corner, "Descida do Lago");
        assert_eq!(r.hotspots[0].laps, vec![1, 7, 19]);
        assert_eq!(r.incidents.iter().find(|i| i.kind == IncidentKind::CarCollision).unwrap().corner.as_deref(), Some("Ferradura"));
        assert_eq!(r.incidents.iter().find(|i| i.lap == 11).unwrap().corner, None, "Curva do Sol n'est pas dans le relevé : pas de faux nom");
        assert!(r.headline.starts_with("P19 → P11 sur 23 pilotes, 10x"), "{}", r.headline);
        let titles: Vec<&str> = r.insights.iter().map(|i| i.title.as_str()).collect();
        assert!(titles.iter().any(|t| t.starts_with("Descida do Lago : 3 incidents")), "{titles:?}");
        assert!(titles.iter().any(|t| t.contains("accrochage")), "{titles:?}");
        assert!(titles.iter().any(|t| t.starts_with("Fin de course")), "{titles:?}");
        assert!(titles.iter().any(|t| t.starts_with("Qualification : tu pars P19")), "{titles:?}");
        assert!(titles.iter().any(|t| t.starts_with("Départ : 2 places perdues")), "{titles:?}");
        assert_eq!(r.insights[0].kind, RaceInsightKind::Danger);
    }

    #[test]
    fn quiet_race_has_no_false_alarm() {
        let race = RaceData { field_size: 6, start_position: Some(4), finish_position: Some(2), positions: (1..=13).map(|l| (l, if l < 9 { 3 } else { 2 })).collect(), ..Default::default() };
        let laps: Vec<LapMeta> = (1..=14).map(|n| lap(n, 120.5 + (n % 2) as f64 * 0.2, true)).collect();
        let r = race_report(&race, &laps, &[], 5513.0);
        assert!(r.insights.iter().all(|i| i.kind != RaceInsightKind::Danger && i.kind != RaceInsightKind::Warning), "{:?}", r.insights);
        assert_eq!(r.headline, "P4 → P2 sur 6 pilotes, 0x");
    }
}
