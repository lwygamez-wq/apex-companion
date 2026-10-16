//! Modèle de données unifié, indépendant du simulateur.
//!
//! Principe : toute donnée qui n'est pas fournie par la source est `None`,
//! jamais une valeur par défaut « plausible ». L'UI affiche « non disponible ».

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimId {
    Iracing,
    Lmu,
    AssettoCorsa,
    AcEvo,
    Acc,
    /// Données de démonstration générées localement.
    Demo,
}

impl SimId {
    pub const ALL: [SimId; 6] = [
        SimId::Iracing,
        SimId::Lmu,
        SimId::AssettoCorsa,
        SimId::AcEvo,
        SimId::Acc,
        SimId::Demo,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            SimId::Iracing => "iracing",
            SimId::Lmu => "lmu",
            SimId::AssettoCorsa => "assetto_corsa",
            SimId::AcEvo => "ac_evo",
            SimId::Acc => "acc",
            SimId::Demo => "demo",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        SimId::ALL.into_iter().find(|x| x.as_str() == s)
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            SimId::Iracing => "iRacing",
            SimId::Lmu => "Le Mans Ultimate",
            SimId::AssettoCorsa => "Assetto Corsa",
            SimId::AcEvo => "Assetto Corsa EVO",
            SimId::Acc => "Assetto Corsa Competizione",
            SimId::Demo => "Démo",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    Practice,
    Qualifying,
    Race,
    Hotlap,
    PrivateTesting,
    Unknown,
}

impl SessionKind {
    /// Conversion depuis un libellé brut de simulateur (ex. iRacing `SessionType`).
    pub fn from_label(label: &str) -> Self {
        let l = label.to_ascii_lowercase();
        if l.contains("testing") || l.contains("test") {
            SessionKind::PrivateTesting
        } else if l.contains("qualif") {
            SessionKind::Qualifying
        } else if l.contains("race") || l.contains("heat") {
            SessionKind::Race
        } else if l.contains("time trial") || l.contains("hotlap") || l.contains("hot lap") {
            SessionKind::Hotlap
        } else if l.contains("practice") || l.contains("warmup") || l.contains("warm up") {
            SessionKind::Practice
        } else {
            SessionKind::Unknown
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SessionKind::Practice => "practice",
            SessionKind::Qualifying => "qualifying",
            SessionKind::Race => "race",
            SessionKind::Hotlap => "hotlap",
            SessionKind::PrivateTesting => "private_testing",
            SessionKind::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "practice" => SessionKind::Practice,
            "qualifying" => SessionKind::Qualifying,
            "race" => SessionKind::Race,
            "hotlap" => SessionKind::Hotlap,
            "private_testing" => SessionKind::PrivateTesting,
            _ => SessionKind::Unknown,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Weather {
    pub air_temp_c: Option<f32>,
    pub track_temp_c: Option<f32>,
    /// Libellé brut fourni par le simulateur (ex. « Dry », « Partly Cloudy »).
    pub description: Option<String>,
    pub wet: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMeta {
    pub sim: SimId,
    pub track: String,
    pub track_config: Option<String>,
    pub track_length_m: Option<f32>,
    pub car: String,
    pub kind: SessionKind,
    /// Début de session, secondes Unix (UTC).
    pub started_at_unix: i64,
    pub weather: Weather,
    /// Empreinte stable de la source : clé d'anti-doublon à l'import.
    pub source_fingerprint: String,
    pub source_path: Option<String>,
}

/// Règles de coaching : tour de sortie / lancé / retour stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LapKind {
    OutLap,
    Flying,
    InLap,
    /// Tour non bouclé (début/fin d'enregistrement, reset, téléportation).
    Incomplete,
}

impl LapKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            LapKind::OutLap => "out_lap",
            LapKind::Flying => "flying",
            LapKind::InLap => "in_lap",
            LapKind::Incomplete => "incomplete",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "out_lap" => LapKind::OutLap,
            "flying" => LapKind::Flying,
            "in_lap" => LapKind::InLap,
            _ => LapKind::Incomplete,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Validity {
    Valid,
    Invalid,
    Unknown,
}

impl Validity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Validity::Valid => "valid",
            Validity::Invalid => "invalid",
            Validity::Unknown => "unknown",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "valid" => Validity::Valid,
            "invalid" => Validity::Invalid,
            _ => Validity::Unknown,
        }
    }
}

/// Origine de la validité : rapportée par le jeu ou déduite par nous.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValiditySource {
    SimReported,
    /// Déduite (hors-piste, incident…) — l'UI doit l'afficher comme « estimée ».
    Derived,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LapFlags {
    pub off_track: bool,
    pub incident: bool,
    pub pit: bool,
    pub yellow_flag: bool,
    /// Discontinuité détectée (reset, tow, saut de distance).
    pub discontinuity: bool,
}

impl LapFlags {
    /// Un tour « perturbé » ne doit pas servir de référence sans avertissement.
    pub fn is_disturbed(&self) -> bool {
        self.off_track || self.incident || self.pit || self.yellow_flag || self.discontinuity
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LapMeta {
    pub number: i32,
    pub kind: LapKind,
    pub lap_time_s: Option<f64>,
    pub sector_times_s: Vec<f64>,
    pub flags: LapFlags,
    pub validity: Validity,
    pub validity_source: ValiditySource,
    pub fuel_start_l: Option<f32>,
    pub fuel_used_l: Option<f32>,
}

impl LapMeta {
    /// Tour lancé, bouclé, sans perturbation et non invalide.
    pub fn is_clean(&self) -> bool {
        self.kind == LapKind::Flying
            && self.lap_time_s.is_some()
            && self.validity != Validity::Invalid
            && !self.flags.is_disturbed()
    }
}

/// Télémétrie d'un tour, stockée en colonnes (efficace pour l'analyse et la compression).
///
/// Canaux obligatoires : `time_s`, `dist_m`, `speed_mps`, `throttle`, `brake`.
/// Tous les autres sont optionnels : absents = non fournis par la source.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LapTrace {
    /// Temps depuis le franchissement de la ligne (s).
    pub time_s: Vec<f32>,
    /// Distance depuis la ligne (m).
    pub dist_m: Vec<f32>,
    pub speed_mps: Vec<f32>,
    /// 0..1
    pub throttle: Vec<f32>,
    /// 0..1 (pression normalisée)
    pub brake: Vec<f32>,
    pub gear: Option<Vec<f32>>,
    pub rpm: Option<Vec<f32>>,
    pub steer_rad: Option<Vec<f32>>,
    pub pos_x: Option<Vec<f32>>,
    pub pos_y: Option<Vec<f32>>,
    pub fuel_l: Option<Vec<f32>>,
    pub yaw_rate: Option<Vec<f32>>,
    /// Canaux additionnels (températures pneus, usure…) : clé = nom normalisé.
    pub extra: BTreeMap<String, Vec<f32>>,
}

impl LapTrace {
    pub fn len(&self) -> usize {
        self.time_s.len()
    }
    pub fn is_empty(&self) -> bool {
        self.time_s.is_empty()
    }

    /// Vérifie la cohérence des longueurs de canaux et l'absence de NaN sur les canaux requis.
    pub fn validate(&self) -> Result<(), String> {
        let n = self.time_s.len();
        let required = [
            ("dist_m", &self.dist_m),
            ("speed_mps", &self.speed_mps),
            ("throttle", &self.throttle),
            ("brake", &self.brake),
        ];
        for (name, ch) in required {
            if ch.len() != n {
                return Err(format!("canal {name}: {} échantillons au lieu de {n}", ch.len()));
            }
        }
        let optional = [
            ("gear", &self.gear),
            ("rpm", &self.rpm),
            ("steer_rad", &self.steer_rad),
            ("pos_x", &self.pos_x),
            ("pos_y", &self.pos_y),
            ("fuel_l", &self.fuel_l),
            ("yaw_rate", &self.yaw_rate),
        ];
        for (name, ch) in optional {
            if let Some(c) = ch {
                if c.len() != n {
                    return Err(format!("canal {name}: {} échantillons au lieu de {n}", c.len()));
                }
            }
        }
        for (name, ch) in &self.extra {
            if ch.len() != n {
                return Err(format!("canal {name}: {} échantillons au lieu de {n}", ch.len()));
            }
        }
        if self.time_s.iter().chain(&self.dist_m).chain(&self.speed_mps).any(|v| !v.is_finite()) {
            return Err("valeurs non finies dans les canaux requis".into());
        }
        Ok(())
    }

    pub fn max_dist(&self) -> f32 {
        self.dist_m.iter().copied().fold(0.0, f32::max)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportedLap {
    pub meta: LapMeta,
    pub trace: LapTrace,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportedSession {
    pub meta: SessionMeta,
    pub laps: Vec<ImportedLap>,
    /// Fréquence d'échantillonnage de la source (Hz), si connue.
    pub sample_rate_hz: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_kind_labels() {
        assert_eq!(SessionKind::from_label("Offline Testing"), SessionKind::PrivateTesting);
        assert_eq!(SessionKind::from_label("Lone Qualify"), SessionKind::Qualifying);
        assert_eq!(SessionKind::from_label("Race"), SessionKind::Race);
        assert_eq!(SessionKind::from_label("Open Practice"), SessionKind::Practice);
        assert_eq!(SessionKind::from_label("???"), SessionKind::Unknown);
    }

    #[test]
    fn sim_id_roundtrip() {
        for s in SimId::ALL {
            assert_eq!(SimId::parse(s.as_str()), Some(s));
        }
    }

    #[test]
    fn trace_validation_catches_length_mismatch() {
        let mut t = LapTrace {
            time_s: vec![0.0, 0.1],
            dist_m: vec![0.0, 5.0],
            speed_mps: vec![50.0, 50.0],
            throttle: vec![1.0, 1.0],
            brake: vec![0.0, 0.0],
            ..Default::default()
        };
        assert!(t.validate().is_ok());
        t.rpm = Some(vec![1.0]);
        assert!(t.validate().is_err());
    }
}
