//! Ingénieur setup guidé (Setup Builder, étape 3) : UN essai à la fois, tiré du constat le plus net du
//! diagnostic, puis un verdict mesuré entre deux séances (avant / après).
//!
//! Règles :
//! - un essai seulement à partir d'un constat net et répété de `setup_diag` (même sens sur les deux roues
//!   d'un essieu), en crans, avec la mesure qui le justifie ;
//! - aucun gain chrono promis : le verdict dit si l'effet mesuré dépasse la dispersion, c'est tout ;
//! - aucune comparaison à l'idéal de 92 °C (échelle du jeu non confirmée) : les essais portent sur des
//!   écarts entre zones du pneu, pas sur une température cible ;
//! - le sens du réglage est donné dans les valeurs du jeu (« vers 0° ») : le cran qui y mène dépend de la
//!   voiture, le verdict vérifie dans le setup enregistré que le changement a bien été fait.

use super::setup_diag::{SetupDiagnosis, SetupRow, Stat, TyreDiag};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrialMetric {
    /// Intérieur moins extérieur, en appui (°C), moyenne des deux roues de l'essieu.
    InnerMinusOuter,
    /// Milieu moins moyenne des bords, en appui (°C).
    MiddleMinusEdges,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Trial {
    /// « avant » ou « arrière ».
    pub axle: &'static str,
    pub label: String,
    /// Réglages du jeu concernés (les deux roues de l'essieu) et leur état actuel.
    pub keys: Vec<String>,
    pub current: Vec<SetupRow>,
    /// Sens dans les valeurs affichées par le jeu, et nombre de crans.
    pub direction: String,
    pub steps: i32,
    pub metric: TrialMetric,
    /// Valeur mesurée qui justifie l'essai (moyenne de l'essieu, °C), dispersion et tours.
    pub measured: f32,
    pub spread: f32,
    pub laps: usize,
    /// Phrases prêtes à afficher.
    pub why: String,
    pub what: String,
    pub protocol: String,
    /// Réglage en butée d'un côté : le jeu peut refuser le sens demandé.
    pub at_limit: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TrialVerdict {
    /// Le réglage a bien changé dans le sens proposé (setup enregistré des deux séances).
    pub applied: bool,
    pub applied_text: String,
    /// Autres réglages changés entre les deux séances : l'effet peut leur être dû.
    pub others_changed: Vec<String>,
    pub before: Option<f32>,
    pub after: Option<f32>,
    pub effect: Option<f32>,
    /// Écart minimal pour parler d'effet : 2 erreurs types de la différence, et 1 °C au moins.
    pub threshold: Option<f32>,
    pub significant: bool,
    /// Médiane des tours utilisables avant / après (s), à titre indicatif.
    pub lap_before_s: Option<f64>,
    pub lap_after_s: Option<f64>,
    pub text: String,
}

const MIN_DELTA_C: f32 = 3.0;

fn axle_stat(tyres: &[TyreDiag], axle: usize, f: fn(&TyreDiag) -> Option<&Stat>) -> Option<(f32, f32, usize)> {
    let a = f(tyres.get(axle * 2)?)?;
    let b = f(tyres.get(axle * 2 + 1)?)?;
    (a.value.signum() == b.value.signum()).then(|| ((a.value + b.value) / 2.0, (a.spread + b.spread) / 2.0, a.laps.min(b.laps)))
}

fn fr1(v: f32) -> String {
    format!("{v:.1}").replace('.', ",")
}

/// Essai proposé à partir d'un diagnostic, ou `None` avec la raison.
pub fn propose(d: &SetupDiagnosis) -> Result<Trial, String> {
    if d.laps_used < 5 {
        return Err(format!("{} tours utilisables : il en faut 5 pour proposer un essai.", d.laps_used));
    }
    if d.setup.is_empty() {
        return Err("Setup de la séance non enregistré : impossible de dire quel cran changer.".into());
    }
    let mut cands: Vec<(f32, Trial)> = vec![];
    let mut skipped: Vec<String> = vec![];
    for (axle, name, short) in [(0usize, "avant", "AV"), (1, "arrière", "AR")] {
        let wheels = if axle == 0 { ["FL", "FR"] } else { ["RL", "RR"] };
        for (metric, stat_fn, key_base, label) in [
            (TrialMetric::InnerMinusOuter, (|t: &TyreDiag| t.inner_minus_outer.as_ref()) as fn(&TyreDiag) -> Option<&Stat>, "WM_CAMBER", "Carrossage"),
            (TrialMetric::MiddleMinusEdges, |t: &TyreDiag| t.middle_minus_edges.as_ref(), "WM_PRESSURE", "Pression à froid"),
        ] {
            let Some((v, spread, laps)) = axle_stat(&d.tyres, axle, stat_fn) else { continue };
            if v.abs() < MIN_DELTA_C {
                continue;
            }
            let keys: Vec<String> = wheels.iter().map(|w| format!("{key_base}-W_{w}")).collect();
            let current: Vec<SetupRow> = keys.iter().filter_map(|k| d.setup.iter().find(|r| &r.key == k).cloned()).collect();
            if current.len() != 2 {
                continue;
            }
            // Crans libres de chaque côté : le cran qui va « vers 0° » dépend de la voiture, on reste donc
            // dans la marge disponible des deux côtés ; en butée, pas d'essai sur ce réglage.
            let room = current.iter().map(|r| (r.step - r.min).min(r.max - r.step)).min().unwrap_or(0);
            let at_limit = room <= 0;
            if at_limit {
                skipped.push(format!("{label} {short} en butée (cran {} sur {}–{})", current[0].step, current[0].min, current[0].max));
                continue;
            }
            let (direction, what_dir, why) = match metric {
                TrialMetric::InnerMinusOuter if v > 0.0 => ("vers 0°", "moins négatif", format!("intérieur plus chaud que l'extérieur de {} °C", fr1(v))),
                TrialMetric::InnerMinusOuter => ("plus négatif", "plus négatif", format!("extérieur plus chaud que l'intérieur de {} °C", fr1(-v))),
                TrialMetric::MiddleMinusEdges if v > 0.0 => ("plus basse", "plus basse", format!("milieu plus chaud que les bords de {} °C", fr1(v))),
                TrialMetric::MiddleMinusEdges => ("plus haute", "plus haute", format!("bords plus chauds que le milieu de {} °C", fr1(-v))),
            };
            let steps = (if metric == TrialMetric::InnerMinusOuter { 2 } else { 1 }).min(room);
            let shown = current.iter().map(|r| r.display.clone()).collect::<Vec<_>>().join(" / ");
            let t = Trial {
                axle: name,
                label: format!("{label} {short}"),
                keys,
                current,
                direction: direction.into(),
                steps,
                metric,
                measured: v,
                spread,
                laps,
                why: format!("Pneus {name} : {why} en appui, sur {laps} tours (±{} d'un tour à l'autre).", fr1(spread / 2.0)),
                what: format!("{label} {short} {what_dir}, {steps} cran{} sur les deux roues (aujourd'hui {shown}). Ne change rien d'autre.", if steps > 1 { "s" } else { "" }),
                protocol: "Roule 5 tours propres avec le réglage actuel, puis change ce seul réglage et roule 5 tours propres, même circuit, même voiture. Apex compare les deux séances.".into(),
                at_limit,
            };
            cands.push((v.abs() / spread.max(0.2), t));
        }
    }
    cands.sort_by(|a, b| b.0.total_cmp(&a.0));
    cands.into_iter().next().map(|(_, t)| t).ok_or_else(|| {
        if skipped.is_empty() {
            "Pas d'écart net et répété sur un essieu entier : pas d'essai à proposer.".into()
        } else {
            format!("Écart net, mais réglage en butée : {}. Pas d'essai possible dans ce sens.", skipped.join(", "))
        }
    })
}

/// Verdict d'un essai entre la séance d'avant et celle d'après.
pub fn verdict(trial: &Trial, before: &SetupDiagnosis, after: &SetupDiagnosis, lap_before_s: Option<f64>, lap_after_s: Option<f64>) -> TrialVerdict {
    // Réglage changé, et dans le bon sens ? (valeurs affichées par le jeu)
    let num = |r: &SetupRow| crate::setup::number(&r.display);
    let mut changed_ok = 0;
    let mut changed_txt = vec![];
    for k in &trial.keys {
        let (Some(a), Some(b)) = (before.setup.iter().find(|r| &r.key == k), after.setup.iter().find(|r| &r.key == k)) else { continue };
        if a.step == b.step {
            continue;
        }
        changed_txt.push(format!("{} : {} → {} (cran {} → {})", b.label, a.display, b.display, a.step, b.step));
        let ok = match (num(a), num(b), trial.metric, trial.measured > 0.0) {
            (Some(x), Some(y), TrialMetric::InnerMinusOuter, true) => y.abs() < x.abs(),
            (Some(x), Some(y), TrialMetric::InnerMinusOuter, false) => y.abs() > x.abs(),
            (Some(x), Some(y), TrialMetric::MiddleMinusEdges, true) => y < x,
            (Some(x), Some(y), TrialMetric::MiddleMinusEdges, false) => y > x,
            _ => false,
        };
        if ok {
            changed_ok += 1;
        }
    }
    let applied = changed_ok == trial.keys.len();
    let applied_text = if before.setup.is_empty() || after.setup.is_empty() {
        "Setup non enregistré sur une des deux séances : impossible de vérifier le réglage.".to_string()
    } else if changed_txt.is_empty() {
        format!("{} n'a pas changé entre les deux séances.", trial.label)
    } else if applied {
        format!("Réglage fait : {}.", changed_txt.join(" ; "))
    } else {
        format!("Réglage changé, mais pas dans le sens de l'essai sur les deux roues : {}.", changed_txt.join(" ; "))
    };
    let others_changed: Vec<String> = after
        .setup
        .iter()
        .filter(|b| !trial.keys.contains(&b.key))
        .filter_map(|b| before.setup.iter().find(|a| a.key == b.key).filter(|a| a.step != b.step).map(|a| format!("{} : {} → {}", b.label, a.display, b.display)))
        .collect();
    let axle = if trial.axle == "avant" { 0 } else { 1 };
    let f: fn(&TyreDiag) -> Option<&Stat> = match trial.metric {
        TrialMetric::InnerMinusOuter => |t| t.inner_minus_outer.as_ref(),
        TrialMetric::MiddleMinusEdges => |t| t.middle_minus_edges.as_ref(),
    };
    let side = |d: &SetupDiagnosis| -> Option<(f32, f32, usize)> {
        let a = f(d.tyres.get(axle * 2)?)?;
        let b = f(d.tyres.get(axle * 2 + 1)?)?;
        Some(((a.value + b.value) / 2.0, (a.spread + b.spread) / 2.0, a.laps.min(b.laps)))
    };
    let (sb, sa) = (side(before), side(after));
    let (mut effect, mut threshold, mut significant) = (None, None, false);
    if let (Some((vb, spb, nb)), Some((va, spa, na))) = (sb, sa) {
        // Écart entre 1er et 9e déciles ≈ 2,56 écarts types ; erreur type de la différence des médianes.
        let se = ((spb / 2.56).powi(2) / nb.max(1) as f32 + (spa / 2.56).powi(2) / na.max(1) as f32).sqrt();
        let th = (2.0 * se).max(1.0);
        effect = Some(va - vb);
        threshold = Some(th);
        significant = (va - vb).abs() >= th;
    }
    let what = match trial.metric {
        TrialMetric::InnerMinusOuter => "écart intérieur / extérieur",
        TrialMetric::MiddleMinusEdges => "écart milieu / bords",
    };
    let mut text = match (sb, sa, effect, threshold) {
        (Some((vb, _, nb)), Some((va, _, na)), Some(e), Some(th)) => {
            let sens = if e < 0.0 { "baisse" } else { "monte" };
            if significant {
                format!("Pneus {} : {what} {} °C avant ({nb} tours), {} °C après ({na} tours). L'écart {sens} de {} °C, au-delà de la dispersion ({} °C) : effet mesuré.", trial.axle, fr1(vb), fr1(va), fr1(e.abs()), fr1(th))
            } else {
                format!("Pneus {} : {what} {} °C avant ({nb} tours), {} °C après ({na} tours). Écart de {} °C, sous la dispersion ({} °C) : pas d'effet mesurable.", trial.axle, fr1(vb), fr1(va), fr1(e.abs()), fr1(th))
            }
        }
        _ => "Mesure absente sur une des deux séances (pneus non enregistrés ou moins de 3 tours utilisables).".to_string(),
    };
    if !others_changed.is_empty() {
        text += &format!(" Attention, {} autre{} réglage{} changé{} : l'effet peut leur être dû.", others_changed.len(), if others_changed.len() > 1 { "s" } else { "" }, if others_changed.len() > 1 { "s" } else { "" }, if others_changed.len() > 1 { "s" } else { "" });
    }
    TrialVerdict { applied, applied_text, others_changed, before: sb.map(|x| x.0), after: sa.map(|x| x.0), effect, threshold, significant, lap_before_s, lap_after_s, text }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Diagnostics SYNTHÉTIQUES (pas des séances réelles).
    fn st(value: f32, spread: f32, laps: usize) -> Option<Stat> {
        Some(Stat { value, unit: "°C", laps, spread })
    }

    fn tyre(io: f32, me: f32) -> TyreDiag {
        TyreDiag { wheel: "x", outer: None, middle: None, inner: None, inner_minus_outer: st(io, 1.4, 8), middle_minus_edges: st(me, 0.6, 8), carcass: None, inner_layer: None, game_scale: None, pressure_hot: None, pressure_set: None }
    }

    fn row(key: &str, step: i32, display: &str, max: i32) -> SetupRow {
        SetupRow { key: key.into(), label: key.into(), group: "Pneus".into(), step, min: 0, max, display: display.into() }
    }

    fn diag(front_io: f32, camber_step: i32, camber_disp: &str, extra: Option<SetupRow>) -> SetupDiagnosis {
        let mut setup = vec![
            row("WM_CAMBER-W_FL", camber_step, camber_disp, 31),
            row("WM_CAMBER-W_FR", camber_step, camber_disp, 31),
            row("WM_PRESSURE-W_FL", 0, "131 kPa", 36),
            row("WM_PRESSURE-W_FR", 0, "131 kPa", 36),
        ];
        setup.extend(extra);
        SetupDiagnosis { laps_used: 8, tyres: vec![tyre(front_io, 1.0), tyre(front_io + 0.4, 1.2), tyre(2.0, 0.5), tyre(2.5, 0.4)], setup, ..Default::default() }
    }

    #[test]
    fn proposes_one_trial_from_the_clearest_axle_finding() {
        let t = propose(&diag(8.0, 20, "-2.5 deg", None)).unwrap();
        assert_eq!((t.label.as_str(), t.direction.as_str(), t.steps, t.metric), ("Carrossage AV", "vers 0°", 2, TrialMetric::InnerMinusOuter));
        assert!((t.measured - 8.2).abs() < 1e-4);
        assert_eq!(t.why, "Pneus avant : intérieur plus chaud que l'extérieur de 8,2 °C en appui, sur 8 tours (±0,7 d'un tour à l'autre).");
        assert_eq!(t.what, "Carrossage AV moins négatif, 2 crans sur les deux roues (aujourd'hui -2.5 deg / -2.5 deg). Ne change rien d'autre.");
        assert!(!t.at_limit);
        // Pression au cran 0 (comme à Monza) : en butée, écartée, même si l'écart milieu/bords est plus net.
        let mut d = diag(8.0, 20, "-2.5 deg", None);
        for t in &mut d.tyres {
            t.middle_minus_edges = st(6.0, 0.3, 8);
        }
        assert_eq!(propose(&d).unwrap().label, "Carrossage AV");
        // Carrossage à un cran de la butée : essai d'un seul cran.
        assert_eq!(propose(&diag(8.0, 30, "-1.0 deg", None)).unwrap().steps, 1);
        // Écart sous 3 °C partout : pas d'essai, et la raison.
        let none = propose(&diag(2.0, 20, "-2.5 deg", None)).unwrap_err();
        assert!(none.starts_with("Pas d'écart net"), "{none}");
        // Trop peu de tours.
        let few = SetupDiagnosis { laps_used: 3, ..diag(8.0, 20, "-2.5 deg", None) };
        assert!(propose(&few).unwrap_err().contains("il en faut 5"));
    }

    #[test]
    fn verdict_checks_the_change_and_measures_the_effect() {
        let before = diag(8.0, 20, "-2.5 deg", None);
        let t = propose(&before).unwrap();
        // Après : carrossage −2.3°, écart intérieur/extérieur 5,0 °C : baisse de 3,2 °C, au-delà de la dispersion.
        let after = diag(4.8, 22, "-2.3 deg", None);
        let v = verdict(&t, &before, &after, Some(97.6), Some(97.5));
        assert!(v.applied, "{}", v.applied_text);
        assert_eq!(v.applied_text, "Réglage fait : WM_CAMBER-W_FL : -2.5 deg → -2.3 deg (cran 20 → 22) ; WM_CAMBER-W_FR : -2.5 deg → -2.3 deg (cran 20 → 22).");
        assert!(v.significant);
        assert!((v.effect.unwrap() + 3.2).abs() < 1e-4);
        assert_eq!(v.text, "Pneus avant : écart intérieur / extérieur 8,2 °C avant (8 tours), 5,0 °C après (8 tours). L'écart baisse de 3,2 °C, au-delà de la dispersion (1,0 °C) : effet mesuré.");
        // Petit écart : pas d'effet mesurable. Autre réglage changé : signalé.
        let after2 = diag(7.6, 22, "-2.3 deg", Some(row("VM_REAR_WING", 5, "P6", 12)));
        let before2 = diag(8.0, 20, "-2.5 deg", Some(row("VM_REAR_WING", 3, "P4", 12)));
        let v2 = verdict(&t, &before2, &after2, None, None);
        assert!(!v2.significant);
        assert!(v2.text.contains("pas d'effet mesurable") && v2.text.ends_with("Attention, 1 autre réglage changé : l'effet peut leur être dû."), "{}", v2.text);
        assert_eq!(v2.others_changed, vec!["VM_REAR_WING : P4 → P6".to_string()]);
        // Réglage dans le mauvais sens (plus négatif) : pas appliqué.
        let wrong = diag(9.0, 18, "-2.7 deg", None);
        assert!(!verdict(&t, &before, &wrong, None, None).applied);
        // Réglage inchangé.
        assert_eq!(verdict(&t, &before, &before, None, None).applied_text, "WM_CAMBER-W_FL n'a pas changé entre les deux séances.".replace("WM_CAMBER-W_FL", &t.label));
    }
}
