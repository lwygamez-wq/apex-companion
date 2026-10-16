//! Génération de conseils de pilotage à partir de métriques réelles.
//!
//! Chaque conseil est rattaché à un virage, à des mesures (preuves) et à un
//! niveau de confiance justifié. Aucun conseil générique.

use super::corners::{CornerDef, CornerMetrics};
use super::goals::{GoalMetric, GoalSuggestion};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Critical,
    Important,
    Improvement,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

impl Confidence {
    pub fn label_fr(&self) -> &'static str {
        match self {
            Confidence::Low => "faible",
            Confidence::Medium => "moyenne",
            Confidence::High => "élevée",
        }
    }
    fn downgrade(self) -> Self {
        match self {
            Confidence::High => Confidence::Medium,
            _ => Confidence::Low,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Evidence {
    pub label: String,
    pub reference: String,
    pub compared: String,
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Insight {
    pub id: String,
    pub title: String,
    pub priority: Priority,
    /// Fourchette prudente de gain (s) : [50 %, 100 %] du temps perdu mesuré.
    pub est_gain_s: Option<[f32; 2]>,
    pub segment: Option<String>,
    pub segment_range_m: Option<[f32; 2]>,
    pub explanation: String,
    pub evidence: Vec<Evidence>,
    pub confidence: Confidence,
    pub confidence_notes: Vec<String>,
    pub recommendation: String,
    /// Objectif mesurable proposé pour la prochaine session (`None` si non mesurable).
    pub goal: Option<GoalSuggestion>,
}

/// Qualité des données de la comparaison, calculée en amont.
#[derive(Debug, Clone, Default)]
pub struct DataQuality {
    pub both_clean: bool,
    pub min_coverage: f32,
    pub min_density_per_m: f32,
    pub has_steering: bool,
    pub conditions_comparable: bool,
    /// Nombre de tours propres montrant le même défaut (analyse multi-tours), 0 = inconnu.
    pub repeat_count: u32,
    pub notes: Vec<String>,
}

impl DataQuality {
    pub fn base_confidence(&self) -> (Confidence, Vec<String>) {
        let mut c = Confidence::High;
        let mut notes = self.notes.clone();
        if !self.both_clean {
            c = c.downgrade();
            notes.push("au moins un des deux tours est perturbé ou non validé".into());
        }
        if self.min_coverage < 0.97 {
            c = c.downgrade();
            notes.push(format!("couverture télémétrie incomplète ({:.0} %)", self.min_coverage * 100.0));
        }
        if self.min_density_per_m < 0.15 {
            c = c.downgrade();
            notes.push(format!(
                "échantillonnage faible ({:.2} échantillon/m) : points de freinage imprécis",
                self.min_density_per_m
            ));
        }
        if !self.conditions_comparable {
            c = c.downgrade();
            notes.push("conditions de roulage différentes (voiture, météo ou circuit)".into());
        }
        (c, notes)
    }
}

/// Seuils de détection (volontairement au-dessus du bruit de mesure).
#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    pub min_loss_s: f32,
    pub brake_m: f32,
    pub speed_kmh: f32,
    pub throttle_m: f32,
    pub extra_reversals: u32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self { min_loss_s: 0.05, brake_m: 8.0, speed_kmh: 3.0, throttle_m: 10.0, extra_reversals: 2 }
    }
}

fn priority_for(loss: f32) -> Priority {
    if loss >= 0.25 {
        Priority::Critical
    } else if loss >= 0.12 {
        Priority::Important
    } else {
        Priority::Improvement
    }
}

fn fmt_opt_m(v: Option<f32>) -> String {
    v.map_or("—".into(), |x| format!("{x:.0} m"))
}

/// Conseils pour un virage : compare le tour analysé (`cmp`) à la référence (`rf`).
pub fn corner_insight(
    corner: &CornerDef,
    rf: &CornerMetrics,
    cmp: &CornerMetrics,
    q: &DataQuality,
    th: &Thresholds,
) -> Option<Insight> {
    let loss = cmp.time_s - rf.time_s;
    if loss < th.min_loss_s {
        return None;
    }
    // Écarts signés (convention : positif = défavorable au tour analysé).
    let brake_early = match (rf.brake_on_m, cmp.brake_on_m) {
        (Some(a), Some(b)) => Some(a - b), // > 0 : freine plus tôt
        _ => None,
    };
    let vmin_deficit = rf.min_speed_kmh - cmp.min_speed_kmh; // > 0 : plus lent à l'apex
    let exit_deficit = rf.exit_speed_kmh - cmp.exit_speed_kmh;
    let throttle_late = match (rf.throttle_on_m, cmp.throttle_on_m) {
        (Some(a), Some(b)) => Some(b - a), // > 0 : remet les gaz plus tard
        _ => None,
    };
    let extra_rev = match (rf.steer_reversals, cmp.steer_reversals) {
        (Some(a), Some(b)) => Some(b as i64 - a as i64),
        _ => None,
    };
    let ramp_faster = match (rf.throttle_ramp_s, cmp.throttle_ramp_s) {
        (Some(a), Some(b)) => Some(a - b), // > 0 : montée plus brutale
        _ => None,
    };

    let v = |x: f32| format!("{x:.0} km/h");
    let seg = &corner.name;
    let (title, explanation, rec, mut sharp, goal) = if let Some(be) = brake_early.filter(|b| *b >= th.brake_m) {
        (
            format!("{seg} : freinage trop précoce"),
            format!(
                "Tu freines environ {be:.0} m plus tôt que la référence{}. Le temps perdu vient surtout de la zone de freinage, avant même le point de corde.",
                if vmin_deficit >= th.speed_kmh { format!(" et ta vitesse minimale est inférieure de {vmin_deficit:.0} km/h") } else { String::new() }
            ),
            format!(
                "Décale ton point de freinage d'environ {:.0} m plus loin (réf. : {}), en gardant la même attaque de pédale, puis relâche progressivement jusqu'à la corde pour viser {} au minimum.",
                (be * 0.7).max(5.0), fmt_opt_m(rf.brake_on_m), v(rf.min_speed_kmh)
            ),
            be >= th.brake_m * 1.5,
            rf.brake_on_m.map(|t| (format!("{seg} : commencer à freiner à {t:.0} m ou plus loin"), GoalMetric::BrakePoint { apex_m: corner.apex_m, target_m: t })),
        )
    } else if let Some(bl) = brake_early.filter(|b| -*b >= th.brake_m && (vmin_deficit >= th.speed_kmh || exit_deficit >= th.speed_kmh)) {
        let bl = -bl;
        (
            format!("{seg} : freinage trop tardif, entrée surchargée"),
            format!(
                "Tu freines {bl:.0} m plus tard que la référence mais tu sors moins vite ({} contre {}) : l'entrée trop engagée coûte la vitesse de sortie.",
                v(cmp.exit_speed_kmh), v(rf.exit_speed_kmh)
            ),
            format!(
                "Reviens vers le point de freinage de la référence ({}), relâche le frein plus tôt et privilégie une rotation propre pour ressortir à {}.",
                fmt_opt_m(rf.brake_on_m), v(rf.exit_speed_kmh)
            ),
            bl >= th.brake_m * 1.5,
            Some((format!("{seg} : sortir à {} au moins", v(rf.exit_speed_kmh)), GoalMetric::ExitSpeed { apex_m: corner.apex_m, target_kmh: rf.exit_speed_kmh })),
        )
    } else if vmin_deficit >= th.speed_kmh {
        (
            format!("{seg} : vitesse minimale insuffisante"),
            format!(
                "Ton point de freinage est proche de la référence mais ta vitesse minimale est inférieure de {vmin_deficit:.0} km/h ({} contre {}) : tu freines trop fort ou trop longtemps dans la phase de relâche.",
                v(cmp.min_speed_kmh), v(rf.min_speed_kmh)
            ),
            format!(
                "Relâche le frein plus progressivement pour porter davantage de vitesse jusqu'à la corde (réf. : {} vers {:.0} m).",
                v(rf.min_speed_kmh), rf.min_speed_at_m
            ),
            vmin_deficit >= th.speed_kmh * 2.0,
            Some((format!("{seg} : passer la corde à {} au moins", v(rf.min_speed_kmh)), GoalMetric::MinSpeed { apex_m: corner.apex_m, target_kmh: rf.min_speed_kmh })),
        )
    } else if let Some(tl) = throttle_late.filter(|t| *t >= th.throttle_m) {
        (
            format!("{seg} : remise des gaz tardive"),
            format!(
                "Vitesse minimale comparable, mais tu reprends les gaz {tl:.0} m plus tard que la référence, ce qui te coûte en sortie ({} contre {}).",
                v(cmp.exit_speed_kmh), v(rf.exit_speed_kmh)
            ),
            format!(
                "Reprends l'accélérateur dès que le volant commence à se redresser, vers {} comme sur la référence, en dosant progressivement.",
                fmt_opt_m(rf.throttle_on_m)
            ),
            tl >= th.throttle_m * 2.0,
            rf.throttle_on_m.map(|t| (format!("{seg} : remettre les gaz avant {t:.0} m"), GoalMetric::ThrottleOn { apex_m: corner.apex_m, target_m: t })),
        )
    } else if exit_deficit >= th.speed_kmh {
        (
            format!("{seg} : sortie de virage pénalisante"),
            format!(
                "Tu sors {exit_deficit:.0} km/h moins vite que la référence ({} contre {}) sans écart net au freinage ni à la corde : la perte se fait dans la phase de réaccélération.",
                v(cmp.exit_speed_kmh), v(rf.exit_speed_kmh)
            ),
            "Ouvre la trajectoire en sortie et vise une montée des gaz continue plutôt qu'une reprise hésitante.".into(),
            false,
            Some((format!("{seg} : sortir à {} au moins", v(rf.exit_speed_kmh)), GoalMetric::ExitSpeed { apex_m: corner.apex_m, target_kmh: rf.exit_speed_kmh })),
        )
    } else if let (Some(r), Some(f)) = (extra_rev.filter(|r| *r >= th.extra_reversals as i64), ramp_faster.filter(|f| *f > 0.15)) {
        (
            format!("{seg} : remise des gaz trop brutale"),
            format!(
                "Ta montée des gaz est {f:.2} s plus rapide que la référence et suivie de {r} corrections de volant supplémentaires : signe d'un train arrière déstabilisé."
            ),
            "Allonge la montée des gaz entre la corde et la sortie ; n'atteins 100 % qu'une fois le volant quasiment droit.".into(),
            false,
            None,
        )
    } else {
        // Perte mesurée mais pas de cause isolable avec les canaux disponibles : on le dit.
        (
            format!("{seg} : perte de temps sans cause isolée"),
            format!("Tu perds {loss:.2} s sur ce virage, mais freinage, vitesse minimale et remise des gaz restent dans les seuils de détection."),
            "Compare la trajectoire sur la carte : la perte vient probablement de la ligne (point de corde, largeur de piste utilisée), non mesurée directement.".into(),
            false,
            None,
        )
    };

    let mut evidence = vec![
        Evidence {
            label: "Temps dans le virage".into(),
            reference: format!("{:.2} s", rf.time_s),
            compared: format!("{:.2} s", cmp.time_s),
            delta: format!("{loss:+.2} s"),
        },
        Evidence {
            label: "Début de freinage".into(),
            reference: fmt_opt_m(rf.brake_on_m),
            compared: fmt_opt_m(cmp.brake_on_m),
            delta: brake_early.map_or("—".into(), |b| format!("{:+.0} m", -b)),
        },
        Evidence {
            label: "Vitesse minimale".into(),
            reference: v(rf.min_speed_kmh),
            compared: v(cmp.min_speed_kmh),
            delta: format!("{:+.1} km/h", -vmin_deficit),
        },
        Evidence {
            label: "Vitesse de sortie".into(),
            reference: v(rf.exit_speed_kmh),
            compared: v(cmp.exit_speed_kmh),
            delta: format!("{:+.1} km/h", -exit_deficit),
        },
        Evidence {
            label: "Remise des gaz".into(),
            reference: fmt_opt_m(rf.throttle_on_m),
            compared: fmt_opt_m(cmp.throttle_on_m),
            delta: throttle_late.map_or("—".into(), |t| format!("{t:+.0} m")),
        },
    ];
    if let (Some(a), Some(b)) = (rf.steer_reversals, cmp.steer_reversals) {
        evidence.push(Evidence {
            label: "Corrections de volant".into(),
            reference: a.to_string(),
            compared: b.to_string(),
            delta: format!("{:+}", b as i64 - a as i64),
        });
    }

    let (mut conf, mut notes) = q.base_confidence();
    if title.contains("sans cause isolée") {
        conf = Confidence::Low;
        notes.push("aucune métrique ne dépasse les seuils : cause non identifiable".into());
        sharp = false;
    }
    if !sharp && conf == Confidence::High {
        conf = Confidence::Medium;
        notes.push("écart mesuré proche des seuils de détection".into());
    }
    if q.repeat_count >= 3 && conf < Confidence::High && q.both_clean {
        conf = Confidence::High;
        notes.push(format!("défaut répété sur {} tours propres", q.repeat_count));
    }
    let _ = extra_rev;

    Some(Insight {
        id: format!("corner-{}", corner.id),
        title,
        priority: priority_for(loss),
        est_gain_s: Some([loss * 0.5, loss]),
        segment: Some(corner.name.clone()),
        segment_range_m: Some([corner.entry_m, corner.exit_m]),
        explanation: format!(
            "{} Perte mesurée : {:.2} s par rapport à la référence. Confiance : {}.",
            explanation, loss, conf.label_fr()
        ),
        evidence,
        confidence: conf,
        confidence_notes: notes,
        recommendation: rec,
        goal: goal.map(|(title, metric)| GoalSuggestion { title, metric }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(time: f32, brake: f32, vmin: f32, exit: f32, thr: f32) -> CornerMetrics {
        CornerMetrics {
            corner_id: 7,
            time_s: time,
            brake_on_m: Some(brake),
            brake_peak: 1.0,
            min_speed_kmh: vmin,
            min_speed_at_m: brake + 80.0,
            throttle_on_m: Some(thr),
            exit_speed_kmh: exit,
            throttle_ramp_s: Some(0.4),
            steer_reversals: Some(1),
        }
    }
    fn corner() -> CornerDef {
        CornerDef { id: 7, name: "V7".into(), search_start_m: 900.0, entry_m: 1000.0, apex_m: 1100.0, exit_m: 1300.0 }
    }
    fn good_q() -> DataQuality {
        DataQuality { both_clean: true, min_coverage: 1.0, min_density_per_m: 0.5, has_steering: true, conditions_comparable: true, ..Default::default() }
    }

    #[test]
    fn early_braking_example_from_spec() {
        // Spec : « freinage 12 m plus tôt et vitesse minimale inférieure de 5 km/h, perte 0,18 s »
        let rf = m(5.00, 1040.0, 95.0, 150.0, 1130.0);
        let cmp = m(5.18, 1028.0, 90.0, 147.0, 1132.0);
        let i = corner_insight(&corner(), &rf, &cmp, &good_q(), &Thresholds::default()).unwrap();
        assert!(i.title.contains("précoce"), "{}", i.title);
        assert_eq!(i.priority, Priority::Important);
        assert_eq!(i.confidence, Confidence::High);
        assert!(i.explanation.contains("12 m"));
        assert!(i.explanation.contains("5 km/h"));
        let g = i.est_gain_s.unwrap();
        assert!((g[1] - 0.18).abs() < 1e-3 && (g[0] - 0.09).abs() < 1e-3);
    }

    #[test]
    fn no_insight_below_loss_threshold() {
        let rf = m(5.00, 1040.0, 95.0, 150.0, 1130.0);
        let cmp = m(5.03, 1020.0, 80.0, 140.0, 1150.0);
        assert!(corner_insight(&corner(), &rf, &cmp, &good_q(), &Thresholds::default()).is_none());
    }

    #[test]
    fn poor_data_lowers_confidence() {
        let rf = m(5.00, 1040.0, 95.0, 150.0, 1130.0);
        let cmp = m(5.30, 1020.0, 88.0, 145.0, 1135.0);
        let q = DataQuality { both_clean: false, min_coverage: 0.9, ..good_q() };
        let i = corner_insight(&corner(), &rf, &cmp, &q, &Thresholds::default()).unwrap();
        assert_eq!(i.confidence, Confidence::Low);
        assert!(i.confidence_notes.len() >= 2);
    }

    #[test]
    fn unexplained_loss_is_flagged_low_confidence() {
        let rf = m(5.00, 1040.0, 95.0, 150.0, 1130.0);
        let cmp = m(5.15, 1039.0, 94.0, 149.0, 1131.0);
        let i = corner_insight(&corner(), &rf, &cmp, &good_q(), &Thresholds::default()).unwrap();
        assert_eq!(i.confidence, Confidence::Low);
        assert!(i.title.contains("sans cause isolée"));
    }
}
