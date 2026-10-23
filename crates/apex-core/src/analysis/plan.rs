//! Programme d'entraînement : 2 ou 3 objectifs pour la prochaine séance sur un circuit et une voiture,
//! tirés de tes propres données, dans l'ordre d'un ingénieur : rester en piste, puis gagner du temps
//! là où tu en perds à chaque tour, puis consolider (régularité ou tours propres).
//! Chaque objectif est mesurable et évalué sur la séance suivante (`goals::evaluate`).

use super::goals::{GoalMetric, GoalSuggestion};
use super::progress::ProgressZone;
use super::risk::{CornerRisk, RiskVerdict};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanObjective {
    pub title: String,
    /// Pourquoi cet objectif (constat chiffré).
    pub why: String,
    pub metric: GoalMetric,
    /// Virage concerné (le coach vocal le privilégie comme focus).
    pub apex_m: Option<f32>,
    /// Phrase courte pour la radio.
    pub spoken: String,
}

/// Données de la dernière séance utiles au programme.
pub struct PlanInput<'a> {
    pub risk: &'a [CornerRisk],
    pub zones: &'a [ProgressZone],
    pub timed_laps: usize,
    pub clean_laps: usize,
    pub stddev_s: Option<f64>,
}

fn apex_of(m: &GoalMetric) -> Option<f32> {
    match *m {
        GoalMetric::BrakePoint { apex_m, .. } | GoalMetric::MinSpeed { apex_m, .. } | GoalMetric::ExitSpeed { apex_m, .. } | GoalMetric::ThrottleOn { apex_m, .. } | GoalMetric::CornerClean { apex_m, .. } => Some(apex_m),
        _ => None,
    }
}

/// Construit le programme (au plus 3 objectifs, jamais deux sur le même virage).
pub fn build_plan(p: &PlanInput) -> Vec<PlanObjective> {
    let mut out: Vec<PlanObjective> = vec![];
    let near = |out: &Vec<PlanObjective>, a: Option<f32>| a.is_some_and(|a| out.iter().any(|o| o.apex_m.is_some_and(|b| (a - b).abs() < 60.0)));

    // 1. Sécurité : le virage où tu sors le plus, s'il coûte plus qu'il ne rapporte.
    if let Some(r) = p.risk.iter().find(|r| matches!(r.verdict, RiskVerdict::Secure | RiskVerdict::Cuts)) {
        out.push(PlanObjective {
            title: format!("{} : 9 passages sur 10 sans sortie", r.name),
            why: r.message.clone(),
            metric: GoalMetric::CornerClean { apex_m: r.apex_m, target_rate: 0.9 },
            apex_m: Some(r.apex_m),
            spoken: format!("{}, zéro sortie", r.name),
        });
    }
    // 2. Vitesse : la perte la plus répétée de tes tours propres face à ton meilleur tour.
    for z in p.zones.iter().filter(|z| z.occurrences >= 2) {
        let Some(g) = z.example.goal.clone() else { continue };
        if near(&out, apex_of(&g.metric)) {
            continue;
        }
        out.push(objective_from(g, zone_why(z)));
        break;
    }
    // 3. Consolider : rester en piste, puis être régulier.
    let incident_heavy = p.timed_laps >= 3 && p.clean_laps * 2 < p.timed_laps;
    if out.len() < 3 && incident_heavy {
        let target = (p.clean_laps as u32 + 2).clamp(3, 8);
        out.push(PlanObjective {
            title: format!("Enchaîner {target} tours propres"),
            why: format!("{} tours propres sur {} lancés à la dernière séance : un tour propre fait plus progresser qu'un tour rapide raté.", p.clean_laps, p.timed_laps),
            metric: GoalMetric::CleanLaps { target },
            apex_m: None,
            spoken: format!("{target} tours propres"),
        });
    } else if out.len() < 3 {
        if let Some(sd) = p.stddev_s.filter(|s| *s > 0.35) {
            let target = ((sd * 0.8 * 20.0).round() / 20.0).max(0.3) as f32;
            out.push(PlanObjective {
                title: format!("Régularité : écart-type ≤ {target:.2} s"),
                why: format!("Tes tours propres varient de ±{sd:.2} s : les mêmes repères à chaque tour."),
                metric: GoalMetric::Consistency { target_sd_s: target },
                apex_m: None,
                spoken: "régularité, mêmes repères à chaque tour".into(),
            });
        }
    }
    // Encore de la place : deuxième zone de vitesse.
    for z in p.zones.iter().filter(|z| z.occurrences >= 2) {
        if out.len() >= 3 {
            break;
        }
        let Some(g) = z.example.goal.clone() else { continue };
        if near(&out, apex_of(&g.metric)) || out.iter().any(|o| o.title == g.title) {
            continue;
        }
        out.push(objective_from(g, zone_why(z)));
    }
    if out.is_empty() {
        out.push(PlanObjective {
            title: "Enchaîner 5 tours propres".into(),
            why: "Pas encore assez de tours pour cibler un virage : commence par une base propre.".into(),
            metric: GoalMetric::CleanLaps { target: 5 },
            apex_m: None,
            spoken: "5 tours propres".into(),
        });
    }
    out
}

/// Constat d'une zone : le conseil représentatif (celui dont vient l'objectif) et sa récurrence.
fn zone_why(z: &ProgressZone) -> String {
    format!("{} : perte sur {}/{} tours propres, ≈{:.2} s à chaque fois.", z.example.title, z.occurrences, z.laps_compared, z.avg_loss_s)
}

fn objective_from(g: GoalSuggestion, why: String) -> PlanObjective {
    let apex = apex_of(&g.metric);
    let corner = g.title.split(" : ").next().unwrap_or(&g.title).to_string();
    // Version radio : pas de distances en mètres, une consigne qu'on retient en roulant.
    let spoken = match g.metric {
        GoalMetric::BrakePoint { .. } => format!("{corner}, freine plus tard"),
        GoalMetric::MinSpeed { target_kmh, .. } => format!("{corner}, {} à la corde", target_kmh.round() as i32),
        GoalMetric::ExitSpeed { .. } => format!("{corner}, meilleure sortie"),
        GoalMetric::ThrottleOn { .. } => format!("{corner}, gaz plus tôt"),
        GoalMetric::CornerClean { .. } => format!("{corner}, zéro sortie"),
        _ => g.title.replace(" : ", ", "),
    };
    PlanObjective { title: g.title, why, metric: g.metric, apex_m: apex, spoken }
}

/// Ajoute un objectif choisi par le pilote (depuis un conseil) : il remplace l'objectif du même
/// virage, sinon le dernier si le programme est plein. La sécurité (zéro sortie) reste en tête.
pub fn add_objective(mut objs: Vec<PlanObjective>, g: GoalSuggestion, why: String) -> Vec<PlanObjective> {
    let new = objective_from(g, why);
    objs.retain(|o| !(new.apex_m.is_some() && o.apex_m.is_some_and(|a| (a - new.apex_m.unwrap()).abs() < 60.0) && !matches!(o.metric, GoalMetric::CornerClean { .. })) && o.title != new.title);
    if objs.len() >= 3 {
        objs.pop();
    }
    let at = objs.iter().take_while(|o| matches!(o.metric, GoalMetric::CornerClean { .. })).count();
    objs.insert(at, new);
    objs
}

/// Texte radio du programme : « Plan : un, …. Deux, …. ».
pub fn spoken_plan(objs: &[PlanObjective]) -> String {
    const N: [&str; 3] = ["Un", "Deux", "Trois"];
    if objs.is_empty() {
        return "Pas de plan pour cette séance.".into();
    }
    let parts: Vec<String> = objs.iter().zip(N).map(|(o, n)| format!("{n}, {}.", o.spoken)).collect();
    format!("Plan du jour. {}", parts.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::insights::{Confidence, Insight, Priority};

    fn risk(name: &str, apex: f32, verdict: RiskVerdict, offs: usize, passes: usize) -> CornerRisk {
        CornerRisk {
            name: name.into(),
            apex_m: apex,
            entry_m: apex - 80.0,
            exit_m: apex + 80.0,
            passes,
            offs,
            off_rate: offs as f32 / passes as f32,
            mean_clean_s: None,
            best_clean_s: None,
            mean_off_s: None,
            off_cost_s: None,
            gain_available_s: None,
            expected_cost_s: 0.0,
            verdict,
            sessions: 2,
            message: "Freine un repère plus tôt.".into(),
        }
    }

    fn zone(name: &str, apex: f32, occ: usize) -> ProgressZone {
        let goal = GoalSuggestion { title: format!("{name} : commencer à freiner à {:.0} m ou plus loin", apex - 120.0), metric: GoalMetric::BrakePoint { apex_m: apex, target_m: apex - 120.0 } };
        ProgressZone {
            segment: name.into(),
            range_m: [apex - 100.0, apex + 100.0],
            occurrences: occ,
            laps_compared: 6,
            avg_loss_s: 0.21,
            max_loss_s: 0.4,
            typical_issue: format!("{name} : freinage trop précoce"),
            example: Insight {
                id: "x".into(),
                title: format!("{name} : freinage trop précoce"),
                priority: Priority::Important,
                est_gain_s: Some([0.1, 0.2]),
                segment: Some(name.into()),
                segment_range_m: None,
                explanation: String::new(),
                evidence: vec![],
                confidence: Confidence::High,
                confidence_notes: vec![],
                recommendation: String::new(),
                goal: Some(goal),
            },
        }
    }

    #[test]
    fn safety_then_speed_then_consolidation() {
        let r = [risk("Descida do Lago", 1379.0, RiskVerdict::Secure, 9, 39)];
        let z = [zone("Descida do Lago", 1379.0, 4), zone("Junção", 3194.0, 5), zone("Bico de Pato", 2696.0, 3)];
        let plan = build_plan(&PlanInput { risk: &r, zones: &z, timed_laps: 12, clean_laps: 9, stddev_s: Some(0.6) });
        let titles: Vec<&str> = plan.iter().map(|o| o.title.as_str()).collect();
        assert_eq!(plan.len(), 3, "{titles:?}");
        assert!(titles[0].starts_with("Descida do Lago : 9 passages sur 10"));
        assert!(titles[1].starts_with("Junção"), "même virage que la sécurité : zone suivante");
        assert!(titles[2].starts_with("Régularité"), "{titles:?}");
        assert!(matches!(plan[2].metric, GoalMetric::Consistency { target_sd_s } if (target_sd_s - 0.5).abs() < 1e-6));
        assert_eq!(spoken_plan(&plan), "Plan du jour. Un, Descida do Lago, zéro sortie. Deux, Junção, freine plus tard. Trois, régularité, mêmes repères à chaque tour.");
    }

    #[test]
    fn chosen_advice_joins_the_plan() {
        let r = [risk("Descida do Lago", 1379.0, RiskVerdict::Secure, 9, 39)];
        let z = [zone("Junção", 3194.0, 5)];
        let plan = build_plan(&PlanInput { risk: &r, zones: &z, timed_laps: 12, clean_laps: 9, stddev_s: Some(0.6) });
        let g = GoalSuggestion { title: "Pinheirinho : remettre les gaz avant 2440 m".into(), metric: GoalMetric::ThrottleOn { apex_m: 2415.0, target_m: 2440.0 } };
        let p = add_objective(plan, g, "choisi".into());
        assert_eq!(p.len(), 3);
        assert!(p[0].title.starts_with("Descida do Lago"), "la sécurité reste en tête");
        assert!(p[1].title.starts_with("Pinheirinho"));
        assert_eq!(p[1].spoken, "Pinheirinho, gaz plus tôt");
    }

    #[test]
    fn many_incidents_means_clean_laps_first() {
        let plan = build_plan(&PlanInput { risk: &[], zones: &[], timed_laps: 14, clean_laps: 3, stddev_s: Some(0.2) });
        assert_eq!(plan.len(), 1);
        assert!(matches!(plan[0].metric, GoalMetric::CleanLaps { target: 5 }));
        let empty = build_plan(&PlanInput { risk: &[], zones: &[], timed_laps: 0, clean_laps: 0, stddev_s: None });
        assert_eq!(empty[0].title, "Enchaîner 5 tours propres");
    }
}
