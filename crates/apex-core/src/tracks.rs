//! Noms officiels des virages par circuit, positionnés par la distance de l'apex
//! depuis la ligne (mesurée sur des sessions réelles). Les noms donnés par le pilote
//! dans l'app restent prioritaires.
//!
//! Ajouter un circuit : relever les apex détectés sur une vraie session (export),
//! vérifier l'ordre et le sens des virages sur le tracé, puis compléter `TRACKS`.

use crate::model::{SessionMeta, SimId};

struct TrackCorners {
    sim: SimId,
    /// Fragments du nom du circuit tel qu'écrit par le simulateur (sans accents).
    name_contains: &'static [&'static str],
    /// Fragment de la configuration (`None` = toutes).
    config_contains: Option<&'static str>,
    corners: &'static [(f32, &'static str)],
}

const TRACKS: &[TrackCorners] = &[
    // Relevé : session iRacing du 17/10/2026, Super Formula Lights 324, apex détectés automatiquement.
    // Sens vérifié sur le tracé GPS (G, G, D, D, G, D, G). Curva do Sol et Mergulho, à fond, ne sont pas détectés.
    TrackCorners {
        sim: SimId::Iracing,
        name_contains: &["carlos pace", "interlagos"],
        config_contains: Some("grand prix"),
        corners: &[
            (326.0, "S do Senna"),
            (1379.0, "Descida do Lago"),
            (2103.0, "Ferradura"),
            (2292.0, "Laranjinha"),
            (2415.0, "Pinheirinho"),
            (2696.0, "Bico de Pato"),
            (3194.0, "Junção"),
        ],
    },
];

fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'î' | 'ï' => 'i',
            'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

/// Noms intégrés pour le circuit de cette session (vide si circuit inconnu).
pub fn builtin_corner_names(m: &SessionMeta) -> Vec<(f32, String)> {
    let track = fold(&m.track);
    let config = fold(m.track_config.as_deref().unwrap_or(""));
    TRACKS
        .iter()
        .find(|t| {
            t.sim == m.sim
                && t.name_contains.iter().any(|n| track.contains(n))
                && t.config_contains.is_none_or(|c| config.contains(c))
        })
        .map(|t| t.corners.iter().map(|(d, n)| (*d, n.to_string())).collect())
        .unwrap_or_default()
}

/// Fusionne noms intégrés et noms du pilote : un nom du pilote à moins de 60 m remplace le nom intégré.
pub fn merge_names(builtin: Vec<(f32, String)>, user: Vec<(f32, String)>) -> Vec<(f32, String)> {
    let mut out: Vec<(f32, String)> = builtin.into_iter().filter(|(d, _)| !user.iter().any(|(u, _)| (u - d).abs() <= 60.0)).collect();
    out.extend(user);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn meta(track: &str, config: Option<&str>) -> SessionMeta {
        SessionMeta {
            sim: SimId::Iracing,
            track: track.into(),
            track_config: config.map(String::from),
            track_length_m: Some(4222.6),
            car: "Super Formula Lights 324".into(),
            kind: SessionKind::Race,
            started_at_unix: 0,
            weather: Weather::default(),
            source_fingerprint: "x".into(),
            source_path: None,
        }
    }

    #[test]
    fn interlagos_names_with_accents() {
        let n = builtin_corner_names(&meta("Autódromo José Carlos Pace", Some("Grand Prix")));
        assert_eq!(n.len(), 7);
        assert_eq!(n[0].1, "S do Senna");
        assert!(builtin_corner_names(&meta("Spa-Francorchamps", Some("Grand Prix"))).is_empty());
    }

    #[test]
    fn user_name_overrides_builtin() {
        let b = vec![(326.0, "S do Senna".to_string()), (1379.0, "Descida do Lago".to_string())];
        let m = merge_names(b, vec![(340.0, "Le S".to_string())]);
        assert_eq!(m.len(), 2);
        assert!(m.iter().any(|(_, n)| n == "Le S") && !m.iter().any(|(_, n)| n == "S do Senna"));
    }
}
