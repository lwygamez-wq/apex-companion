//! Tours exportés depuis Garage 61 (« Export CSV ») : sert de référence externe.
//!
//! Format observé (export d'octobre 2026) : une ligne d'en-tête avec les noms de canaux iRacing
//! (`Speed,LapDistPct,Lat,Lon,Brake,Throttle,RPM,SteeringWheelAngle,Gear,…,YawRate,PositionType`),
//! un échantillon par ligne à 60 Hz, sans colonne de temps. Le fichier contient un seul tour ;
//! le nom du fichier donne le pilote, la voiture, le circuit et le temps au tour :
//! `Garage 61 - <pilote> - <voiture> - <circuit> - <MM.SS.mmm> - <id>.csv`.

use crate::model::LapTrace;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct G61Name {
    pub driver: String,
    pub car: String,
    /// Circuit et configuration tels qu'écrits par Garage 61 (« Circuit of the Americas Grand Prix »).
    pub track: String,
    pub lap_time_s: f64,
    pub id: String,
}

/// Lit le nom de fichier Garage 61 (tolère un préfixe ajouté devant « Garage 61 - »).
pub fn parse_file_name(name: &str) -> Option<G61Name> {
    let start = name.find("Garage 61 - ")?;
    let stem = name[start..].strip_suffix(".csv").unwrap_or(&name[start..]);
    let parts: Vec<&str> = stem.split(" - ").collect();
    if parts.len() < 6 {
        return None;
    }
    let n = parts.len();
    let t: Vec<&str> = parts[n - 2].split('.').collect();
    let lap_time_s = match t.as_slice() {
        [m, s, ms] => m.parse::<f64>().ok()? * 60.0 + s.parse::<f64>().ok()? + ms.parse::<f64>().ok()? / 10f64.powi(ms.len() as i32),
        _ => return None,
    };
    Some(G61Name {
        driver: parts[1].trim().to_string(),
        car: parts[2].trim().to_string(),
        track: parts[3..n - 2].join(" - ").trim().to_string(),
        lap_time_s,
        id: parts[n - 1].trim().to_string(),
    })
}

/// Mots normalisés (minuscules, sans accents ni ponctuation) : « Miami International Autodrome
/// (Grand Prix) » et « Miami International Autodrome Grand Prix » donnent les mêmes mots.
pub fn tokens(s: &str) -> Vec<String> {
    crate::tracks::fold_name(s).split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()).map(String::from).collect()
}

/// Le circuit écrit par Garage 61 (`g61_track`, configuration comprise) correspond-il à une session
/// iRacing (`track` + `config`) ? Renvoie un score (nombre de mots en commun) ou `None`.
///
/// Règles : tous les mots du nom du circuit iRacing doivent figurer chez Garage 61 ; pour la
/// configuration, soit tous ses mots y figurent, soit Garage 61 n'en précise aucune (circuit à
/// tracé unique). Une configuration différente (« East » contre « Grand Prix ») est refusée.
pub fn track_match_score(g61_track: &str, track: &str, config: Option<&str>) -> Option<usize> {
    let g = tokens(g61_track);
    let t = tokens(track);
    if t.is_empty() || !t.iter().all(|w| g.contains(w)) {
        return None;
    }
    let c = tokens(config.unwrap_or(""));
    let extra: Vec<&String> = g.iter().filter(|w| !t.contains(w)).collect();
    if c.iter().all(|w| g.contains(w)) {
        Some(t.len() + c.len())
    } else if extra.is_empty() {
        Some(t.len())
    } else {
        None
    }
}

/// Même voiture : les mots de l'un sont tous contenus dans l'autre
/// (« Dallara F3 » / « Dallara F312 F3 », « Super Formula Lights » / « Super Formula Lights 324 »).
pub fn car_matches(g61_car: &str, car: &str) -> bool {
    let (g, c) = (tokens(g61_car), tokens(car));
    !g.is_empty() && !c.is_empty() && (g.iter().all(|t| c.contains(t)) || c.iter().all(|t| g.contains(t)))
}

/// Tour décodé : trace et indicateur de sortie de piste.
pub struct G61Lap {
    pub trace: LapTrace,
    pub off_track: bool,
}

/// Décode le CSV. `track_length_m` : longueur du tracé (distance = LapDistPct × longueur).
pub fn parse_csv(text: &str, track_length_m: f32, lap_time_s: f64) -> Result<G61Lap, String> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header: Vec<&str> = lines.next().ok_or("fichier vide")?.split(',').map(str::trim).collect();
    let col = |name: &str| header.iter().position(|h| h.eq_ignore_ascii_case(name));
    let need = |name: &str| col(name).ok_or_else(|| format!("colonne « {name} » absente : ce n'est pas un export de tour Garage 61"));
    let (c_speed, c_pct, c_thr, c_brk) = (need("Speed")?, need("LapDistPct")?, need("Throttle")?, need("Brake")?);
    let (c_lat, c_lon, c_rpm, c_steer, c_gear, c_yaw, c_pos) =
        (col("Lat"), col("Lon"), col("RPM"), col("SteeringWheelAngle"), col("Gear"), col("YawRate"), col("PositionType"));

    let rows: Vec<Vec<&str>> = lines.map(|l| l.split(',').map(str::trim).collect()).collect();
    let num = |r: &Vec<&str>, c: usize| -> Option<f64> {
        r.get(c).and_then(|v| match *v {
            "true" => Some(1.0),
            "false" => Some(0.0),
            v => v.parse::<f64>().ok(),
        })
    };
    // Garde le tour proprement dit : on retire les échantillons d'avant la ligne (pct proche de 1
    // au début) et ceux d'après le franchissement final (pct revenu près de 0 à la fin).
    let pcts: Vec<f64> = rows.iter().map(|r| num(r, c_pct).unwrap_or(f64::NAN)).collect();
    let mut a = 0;
    while a < pcts.len() && !(pcts[a] >= 0.0 && pcts[a] < 0.5) {
        a += 1;
    }
    let mut b = pcts.len();
    while b > a + 1 && !(pcts[b - 1] >= 0.5 || (b - 1 > a && pcts[b - 1] >= pcts[b - 2])) {
        b -= 1;
    }
    for i in a + 1..b {
        if pcts[i] + 0.5 < pcts[i - 1] {
            b = i;
            break;
        }
    }
    if b - a < 100 {
        return Err("tour trop court ou incomplet dans le fichier".into());
    }
    let n = b - a;
    let len = track_length_m as f64;
    // Pas de temps : la durée du tour (nom du fichier) répartie sur les échantillons du tour ;
    // proche de 1/60 s, ce qui sert de contrôle de cohérence.
    let span = pcts[b - 1] - pcts[a];
    let speed0 = num(&rows[a], c_speed).unwrap_or(0.0).max(1.0);
    let speed1 = num(&rows[b - 1], c_speed).unwrap_or(0.0).max(1.0);
    let t0 = pcts[a] * len / speed0;
    let t_end_gap = (1.0 - pcts[b - 1]) * len / speed1;
    let dt = ((lap_time_s - t0 - t_end_gap) / (n - 1) as f64).clamp(1.0 / 70.0, 1.0 / 50.0);
    if span.is_nan() || span <= 0.95 {
        return Err("le fichier ne contient pas un tour complet".into());
    }

    let col_f = |c: Option<usize>| -> Option<Vec<f32>> { c.map(|c| (a..b).map(|i| num(&rows[i], c).unwrap_or(0.0) as f32).collect()) };
    let (pos_x, pos_y) = match (c_lat, c_lon) {
        (Some(la), Some(lo)) => {
            let pts: Vec<(f64, f64)> = (a..b).map(|i| (num(&rows[i], la).unwrap_or(0.0), num(&rows[i], lo).unwrap_or(0.0))).collect();
            match pts.iter().find(|p| p.0 != 0.0 || p.1 != 0.0) {
                Some(&first) => {
                    let origin = super::grid_origin(first.0, first.1);
                    let mut cur = first;
                    let (x, y): (Vec<f32>, Vec<f32>) = pts
                        .iter()
                        .map(|p| {
                            if p.0 != 0.0 || p.1 != 0.0 {
                                cur = *p;
                            }
                            super::latlon_to_xy(cur.0, cur.1, origin)
                        })
                        .collect::<Vec<_>>()
                        .into_iter()
                        .unzip();
                    (Some(x), Some(y))
                }
                None => (None, None),
            }
        }
        _ => (None, None),
    };
    let surface = col_f(c_pos);
    let off_track = surface.as_ref().is_some_and(|s| s.contains(&0.0));
    let mut extra = BTreeMap::new();
    if let Some(s) = surface {
        extra.insert(crate::analysis::offtrack::TRACK_SURFACE.to_string(), s);
    }
    let trace = LapTrace {
        time_s: (0..n).map(|i| (t0 + i as f64 * dt) as f32).collect(),
        dist_m: (a..b).map(|i| (pcts[i].max(0.0) * len) as f32).collect(),
        speed_mps: col_f(Some(c_speed)).unwrap(),
        throttle: col_f(Some(c_thr)).unwrap(),
        brake: col_f(Some(c_brk)).unwrap(),
        gear: col_f(c_gear),
        rpm: col_f(c_rpm),
        steer_rad: col_f(c_steer),
        pos_x,
        pos_y,
        fuel_l: None,
        yaw_rate: col_f(c_yaw),
        extra,
    };
    trace.validate().map_err(|e| format!("tour invalide : {e}"))?;
    Ok(G61Lap { trace, off_track })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_name() {
        let n = parse_file_name("3c0389b5-Garage 61 - Luke Fleming - Dallara F3 - Circuit of the Americas Grand Prix - 01.56.507 - 01M5F68611EC2BYG5Q5WTAAJ52.csv").unwrap();
        assert_eq!(n.driver, "Luke Fleming");
        assert_eq!(n.car, "Dallara F3");
        assert_eq!(n.track, "Circuit of the Americas Grand Prix");
        assert!((n.lap_time_s - 116.507).abs() < 1e-9);
        assert!(parse_file_name("tour.csv").is_none());
    }

    #[test]
    fn real_garage61_names_match_iracing_sessions() {
        // Formats réels relevés sur des exports Garage 61 (octobre 2026).
        let cota = parse_file_name("Garage 61 - Luke Fleming - Dallara F3 - Circuit of the Americas Grand Prix - 01.56.507 - X.csv").unwrap();
        let miami = parse_file_name("Garage 61 - Yohann Enette - Super Formula Lights - Miami International Autodrome (Grand Prix) - 01.38.123 - Y.csv").unwrap();
        assert_eq!(miami.track, "Miami International Autodrome (Grand Prix)");
        assert!(track_match_score(&cota.track, "Circuit of the Americas", Some("Grand Prix")).is_some());
        assert!(track_match_score(&miami.track, "Miami International Autodrome", Some("Grand Prix")).is_some());
        assert!(track_match_score(&miami.track, "Miami International Autodrome", None).is_some());
        assert!(track_match_score("Autódromo José Carlos Pace (Grand Prix)", "Autodromo Jose Carlos Pace", Some("Grand Prix")).is_some());
        // Circuit à tracé unique : Garage 61 ne précise pas de configuration.
        assert!(track_match_score("Okayama International Circuit", "Okayama International Circuit", Some("Full Course")).is_some());
        // Autre configuration ou autre circuit : refus.
        assert!(track_match_score(&cota.track, "Circuit of the Americas", Some("East")).is_none());
        assert!(track_match_score(&miami.track, "Circuit of the Americas", Some("Grand Prix")).is_none());
        // La configuration exacte l'emporte sur le nom seul.
        assert!(track_match_score(&cota.track, "Circuit of the Americas", Some("Grand Prix")) > track_match_score(&cota.track, "Circuit of the Americas", None));
        assert!(car_matches(&miami.car, "Super Formula Lights 324"));
        assert!(car_matches(&cota.car, "Dallara F312 F3"));
        assert!(!car_matches(&cota.car, "Super Formula Lights 324"));
    }

    #[test]
    fn csv_lap() {
        // Tour synthétique de 2000 m à 50 m/s (40 s), un échantillon avant la ligne et un après.
        let mut s = String::from("Speed,LapDistPct,Lat,Lon,Brake,Throttle,RPM,SteeringWheelAngle,Gear,YawRate,PositionType\n");
        s += "50,0.9999,30.1,-97.6,0,1,6000,0,4,0,3\n";
        for i in 0..2400 {
            let pct = 0.0004 + i as f64 * (50.0 / 60.0) / 2000.0;
            if pct >= 1.0 {
                break;
            }
            s += &format!("50,{pct},30.13{:04},-97.64,0,1,6000,0.1,4,0,{}\n", i, if i == 500 { 0 } else { 3 });
        }
        s += "50,0.0003,30.1,-97.6,0,1,6000,0,4,0,3\n";
        let lap = parse_csv(&s, 2000.0, 40.0).unwrap();
        let t = &lap.trace;
        assert!(t.dist_m[0] < 1.0 && *t.dist_m.last().unwrap() > 1990.0);
        assert!((t.time_s[1] - t.time_s[0] - 1.0 / 60.0).abs() < 1e-3, "pas de temps {}", t.time_s[1] - t.time_s[0]);
        assert!(lap.off_track);
        assert!(t.pos_x.is_some() && t.steer_rad.is_some());
    }
}
