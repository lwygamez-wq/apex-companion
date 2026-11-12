//! Ingénieur de course : suit la stratégie carburant / énergie pendant la course et la dit à la radio.
//!
//! À chaque passage de ligne, avec le niveau réel et ta consommation des derniers tours, il recalcule
//! le plan avec le même moteur que le calculateur (`strategy::target`) : nombre d'arrêts, cible par
//! tour, tour de l'arrêt, niveau à régler au stand. Il ne parle que quand c'est utile :
//! - début de relais : relais n sur m, tour de l'arrêt, cible ;
//! - changement de consigne : lift and coast, dans la cible, ou de la marge pour attaquer ;
//! - rappel du lift tous les 3 tours tant qu'il le faut ;
//! - appel aux stands deux tours avant, puis au tour de l'arrêt avec les réglages (énergie, ratio) ;
//! - carburant qui lâcherait avant l'énergie (ratio trop bas) ;
//! - changement de plan (un arrêt de plus ou de moins), avec hystérésis pour ne pas osciller.

use crate::analysis::strategy::{pit_fill, stops_needed, target, Unit, SAVE_MAX};
use serde::{Deserialize, Serialize};

/// Façon de choisir le nombre d'arrêts en course.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineerMode {
    /// Un arrêt de moins si l'économie demandée reste légère (≤ 4 %).
    #[default]
    Auto,
    /// Le moins d'arrêts possible sans économiser : on attaque.
    Attack,
    /// Un arrêt de moins tant que l'économie reste tenable (≤ 8 %).
    Save,
}

impl EngineerMode {
    /// Économie acceptée pour éviter un arrêt (part de la consommation).
    pub fn tolerance(self) -> f32 {
        match self {
            EngineerMode::Attack => 0.0,
            EngineerMode::Auto => 0.04,
            EngineerMode::Save => SAVE_MAX,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            EngineerMode::Auto => "plan auto",
            EngineerMode::Attack => "plan attaque",
            EngineerMode::Save => "plan économie",
        }
    }
}

/// Plan suivi en course pour un circuit et une voiture (choisi dans le calculateur).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineerPlan {
    pub mode: EngineerMode,
    /// Marge gardée en arrivant au stand et à l'arrivée, en tours de consommation.
    pub margin_laps: f32,
}

impl Default for EngineerPlan {
    fn default() -> Self {
        Self { mode: EngineerMode::Auto, margin_laps: 0.5 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Consommation au-dessus de la cible : lever le pied avant les freinages.
    Lift,
    OnTarget,
    /// Nettement sous la cible : de la marge pour attaquer.
    Push,
}

/// La course vue par l'ingénieur, au passage de la ligne.
#[derive(Debug, Clone, Copy)]
pub struct RaceState {
    pub unit: Unit,
    /// Niveau utile : énergie, ou son équivalent en carburant à bord s'il lâche avant.
    pub level: f32,
    /// Niveau après un arrêt : 100 % (moins si le réservoir de carburant limite), ou le plein en litres.
    pub cap: f32,
    /// Consommation moyenne récente.
    pub per_lap: f32,
    /// Tours à couvrir, celui qui commence compris.
    pub laps_left: u32,
    /// Tour qui vient de se terminer.
    pub lap: i32,
    /// Énergie : ratio carburant conseillé (l par 1 %).
    pub fuel_ratio: Option<f32>,
    /// Énergie : tours possibles avec le carburant à bord et avec l'énergie.
    pub fuel_laps: Option<f32>,
    pub energy_laps: Option<f32>,
}

/// Dernière consigne calculée (écran « Au volant », commande « stratégie »).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EngineerCall {
    pub mode: EngineerMode,
    pub stops_left: u32,
    /// Relais en cours et nombre de relais de la course (arrêts faits + restants + 1).
    pub stint: u32,
    pub stints: u32,
    pub target_per_lap: f32,
    pub per_lap: f32,
    pub verdict: Verdict,
    /// Dernier tour du relais (arrêt à la fin de ce tour), et tours d'ici là (celui qui commence compris).
    pub box_lap: Option<i32>,
    pub laps_to_box: Option<u32>,
    /// Au prochain arrêt : niveau à régler et, en énergie, carburant que met le ratio.
    pub fill: Option<f32>,
    pub fill_fuel_l: Option<f32>,
    pub fuel_ratio: Option<f32>,
    /// Le carburant lâchera avant l'énergie : ratio trop bas au dernier arrêt.
    pub fuel_limits: bool,
    /// Tours de marge à l'arrivée au rythme actuel (dernier relais).
    pub finish_spare_laps: Option<f32>,
    /// Tours restant à couvrir.
    pub laps_left: u32,
}

#[derive(Debug, Default)]
pub struct Engineer {
    plan: EngineerPlan,
    stops_done: u32,
    stops_left: Option<u32>,
    /// Ravitaillement : plus bas niveau depuis la dernière reprise de consommation, niveau précédent.
    low: f64,
    prev: f64,
    filling: bool,
    stint_announced: bool,
    last_verdict: Option<Verdict>,
    last_lift_lap: i32,
    box_calls: u8,
    /// Tour de l'arrêt annoncé (« box au prochain tour ») : figé, sauf s'il devient inatteignable.
    committed_box: Option<i32>,
    fuel_warned: bool,
    last: Option<EngineerCall>,
}

/// « 4,82 % », « 0,15 % », « 2,35 litres » : deux décimales, l'écart à la cible se joue au centième.
fn per(unit: Unit, v: f32) -> String {
    let n = format!("{v:.2}").replace('.', ",");
    match unit {
        Unit::Energy => format!("{n} %"),
        Unit::Litres => format!("{n} litre{}", if v >= 2.0 { "s" } else { "" }),
    }
}

/// Niveau à régler, arrondi au-dessus (sans que le bruit du calcul ajoute 1 : 100,000 01 → 100).
fn up(v: f32) -> f32 {
    (v - 0.05).ceil()
}

fn plural(n: u32, w: &str) -> String {
    format!("{n} {w}{}", if n > 1 { "s" } else { "" })
}

impl Engineer {
    pub fn new(plan: EngineerPlan) -> Self {
        Self { plan, low: f64::MAX, ..Default::default() }
    }

    pub fn plan(&self) -> EngineerPlan {
        self.plan
    }

    /// Nouveau plan choisi (calculateur) : le nombre d'arrêts est recalculé au prochain tour.
    pub fn set_plan(&mut self, plan: EngineerPlan) {
        if plan != self.plan {
            self.plan = plan;
            self.stops_left = None;
        }
    }

    /// Nouvelle séance : on repart de zéro (le plan choisi reste).
    pub fn reset(&mut self) {
        *self = Self::new(self.plan);
    }

    pub fn last(&self) -> Option<&EngineerCall> {
        self.last.as_ref()
    }

    /// Arrêts encore prévus (connu après le premier tour de course).
    pub fn stops_left(&self) -> Option<u32> {
        self.stops_left
    }

    pub fn stops_done(&self) -> u32 {
        self.stops_done
    }

    /// Échantillon : un niveau qui remonte de plus de 2 (% ou l) est un ravitaillement.
    pub fn observe(&mut self, level: f64) {
        if level <= 0.0 {
            return;
        }
        if self.filling {
            if level < self.prev - 1e-4 {
                self.filling = false;
                self.low = level;
            }
        } else {
            self.low = self.low.min(level);
            if level > self.low + 2.0 {
                self.filling = true;
                self.stops_done += 1;
                self.stops_left = self.stops_left.map(|s| s.saturating_sub(1));
                self.stint_announced = false;
                self.box_calls = 0;
                self.committed_box = None;
                self.fuel_warned = false;
            }
        }
        self.prev = level;
    }

    /// Passage de la ligne en course : met à jour la consigne et renvoie le message radio s'il y a lieu.
    pub fn on_lap(&mut self, s: &RaceState) -> Option<String> {
        if s.laps_left == 0 || s.per_lap <= 0.0 || s.cap <= 0.0 {
            return None;
        }
        let u = s.unit;
        let word = match u {
            Unit::Energy => "Énergie",
            Unit::Litres => "Carburant",
        };
        let tol = self.plan.mode.tolerance();
        let reserve = self.plan.margin_laps.max(0.0) * s.per_lap;
        let n = s.laps_left;
        let t = |r: u32| target(s.level, s.cap, reserve, n, r);
        let needed = stops_needed(s.level, s.cap, reserve, n, s.per_lap, tol)?;
        let mut parts: Vec<String> = vec![];
        let r = match self.stops_left {
            None => needed,
            // Il faudrait un arrêt de plus : on garde le plan tant qu'un peu plus d'économie suffit.
            Some(cur) if cur < needed => {
                if t(cur).is_some_and(|x| x.per_lap >= s.per_lap * (1.0 - tol - 0.02)) {
                    cur
                } else {
                    let save = t(cur).map(|x| (1.0 - x.per_lap / s.per_lap) * 100.0).unwrap_or(100.0);
                    parts.push(format!("Changement de plan : {} au lieu de {cur}, il faudrait économiser {:.0} % pour tenir.", plural(needed, "arrêt"), save.min(99.0)));
                    self.stint_announced = false;
                    needed
                }
            }
            // Un arrêt de moins devient possible : seulement si c'est net.
            Some(cur) if cur > needed => {
                if t(needed).is_some_and(|x| x.per_lap >= s.per_lap * (1.0 - (tol - 0.01).max(0.0))) {
                    parts.push(format!("Changement de plan : un arrêt de moins, {} à faire.", plural(needed, "arrêt")));
                    self.stint_announced = false;
                    needed
                } else {
                    cur
                }
            }
            Some(cur) => cur,
        };
        self.stops_left = Some(r);
        let tg = t(r)?;
        let ratio = tg.per_lap / s.per_lap;
        let verdict = if ratio < 0.995 {
            Verdict::Lift
        } else if ratio > 1.03 {
            Verdict::Push
        } else {
            Verdict::OnTarget
        };
        let mut laps_to_box = tg.box_after.filter(|_| r > 0).map(|b| b.max(1));
        // Arrêt déjà annoncé : on le garde tant que le niveau permet de l'atteindre au rythme actuel.
        match (self.committed_box, laps_to_box) {
            (Some(cb), Some(_)) => {
                let d = (cb - s.lap).max(1) as u32;
                if s.level - reserve * 0.5 >= d as f32 * s.per_lap {
                    laps_to_box = Some(d);
                } else {
                    self.committed_box = None;
                    self.box_calls = 0;
                }
            }
            (Some(_), None) => self.committed_box = None,
            _ => {}
        }
        if self.committed_box.is_none() {
            self.committed_box = laps_to_box.filter(|d| *d <= 2).map(|d| s.lap + d as i32);
        }
        let box_lap = laps_to_box.map(|b| s.lap + b as i32);
        // Niveau à régler : de quoi faire le relais suivant (relais restants équilibrés) à ta consommation
        // + 2 %, plus la marge. Avec de la marge dans le plan, on remplit moins : arrêt plus court.
        let fill = laps_to_box.map(|b| pit_fill(n.saturating_sub(b), r, s.per_lap, reserve, s.cap));
        let fill_fuel_l = fill.zip(s.fuel_ratio).filter(|_| u == Unit::Energy).map(|(f, r)| f * r);
        let fuel_limits = u == Unit::Energy && matches!((s.fuel_laps, s.energy_laps), (Some(f), Some(e)) if f + 0.3 < e);
        let finish_spare = (r == 0).then(|| (s.level - reserve) / s.per_lap - n as f32);
        let call = EngineerCall {
            mode: self.plan.mode,
            stops_left: r,
            stint: self.stops_done + 1,
            stints: self.stops_done + r + 1,
            target_per_lap: tg.per_lap,
            per_lap: s.per_lap,
            verdict,
            box_lap,
            laps_to_box,
            fill,
            fill_fuel_l,
            fuel_ratio: s.fuel_ratio.filter(|_| u == Unit::Energy),
            fuel_limits,
            finish_spare_laps: finish_spare,
            laps_left: n,
        };
        let tgt = per(u, tg.per_lap);
        let avg = per(u, s.per_lap);
        let verdict_txt = |v: Verdict| match v {
            Verdict::Lift => format!("Tu fais {avg} : lift and coast, {} de moins par tour.", per(u, s.per_lap - tg.per_lap)),
            Verdict::OnTarget => "Tu es dans la cible.".to_string(),
            Verdict::Push if r == 0 => format!("Tu fais {avg}, {} de marge à l'arrivée : pas besoin d'économiser, roule à ton rythme.", plural((finish_spare.unwrap_or(0.0).max(0.0) + 1e-3).floor() as u32, "tour")),
            // Pas « attaque » : l'énergie ne donne pas de puissance, ne pas économiser ne fait pas gagner de temps.
            Verdict::Push => format!("Tu fais {avg} : pas besoin d'économiser, roule à ton rythme."),
        };
        if !self.stint_announced {
            let head = if r == 0 && self.stops_done == 0 {
                format!("Course sans arrêt, {}. Cible {tgt} par tour.", plural(n, "tour"))
            } else if r == 0 {
                format!("Dernier relais, {}. Cible {tgt} par tour.", plural(n, "tour"))
            } else {
                format!("Relais {} sur {}. Box à la fin du tour {}. Cible {tgt} par tour.", call.stint, call.stints, box_lap.unwrap_or(s.lap + 1))
            };
            parts.push(format!("{head} {}", verdict_txt(verdict)));
            self.stint_announced = true;
            self.last_lift_lap = s.lap;
        } else if self.last_verdict != Some(verdict) {
            parts.push(match verdict {
                Verdict::Lift => format!("{word} : tu fais {avg}, cible {tgt}. Lift and coast, {} de moins par tour.", per(u, s.per_lap - tg.per_lap)),
                Verdict::OnTarget => format!("{word} : dans la cible, {tgt} par tour."),
                Verdict::Push => format!("{word} : cible {tgt}. {}", verdict_txt(Verdict::Push)),
            });
            self.last_lift_lap = s.lap;
        } else if verdict == Verdict::Lift && s.lap - self.last_lift_lap >= 3 {
            parts.push(format!("Toujours lift and coast : {avg} pour une cible de {tgt}."));
            self.last_lift_lap = s.lap;
        }
        self.last_verdict = Some(verdict);
        match laps_to_box {
            Some(2) if self.box_calls < 1 => {
                parts.push("Box à la fin du prochain tour.".into());
                self.box_calls = 1;
            }
            Some(1) if self.box_calls < 2 => {
                let set = match (u, fill, fill_fuel_l, call.fuel_ratio) {
                    (Unit::Energy, Some(f), Some(l), Some(rt)) => format!(" Règle l'énergie à {:.0} %, ratio {}, soit {:.0} litres.", up(f), format!("{rt:.2}").replace('.', ","), l.round()),
                    (Unit::Energy, Some(f), _, _) => format!(" Règle l'énergie à {:.0} %.", up(f)),
                    (Unit::Litres, Some(f), _, _) => format!(" Mets {:.0} litres.", up(f)),
                    _ => String::new(),
                };
                parts.push(format!("Box, box, ce tour.{set}"));
                self.box_calls = 2;
            }
            Some(b) if b > 2 => self.box_calls = 0,
            _ => {}
        }
        if fuel_limits && !self.fuel_warned {
            let (f, e) = (s.fuel_laps.unwrap_or(0.0), s.energy_laps.unwrap_or(0.0));
            let fix = call.fuel_ratio.map(|r| format!(" Au prochain arrêt, ratio {}.", format!("{r:.2}").replace('.', ","))).unwrap_or_default();
            parts.push(format!("Attention, le carburant lâchera avant l'énergie : {} de carburant contre {} d'énergie.{fix}", format!("{f:.1} tours").replace('.', ","), format!("{e:.1}").replace('.', ",")));
            self.fuel_warned = true;
        }
        self.last = Some(call);
        (!parts.is_empty()).then(|| parts.join(" "))
    }

    /// Rappel complet (commande « stratégie »).
    pub fn summary(&self, unit: Unit) -> String {
        let Some(c) = &self.last else {
            return "Stratégie annoncée au premier passage de la ligne en course.".into();
        };
        let mut t = format!("{}, {}. ", capitalize(c.mode.label()), plural(c.laps_left, "tour"));
        match (c.box_lap, c.laps_to_box) {
            (Some(b), Some(d)) => t += &format!("Relais {} sur {}, box à la fin du tour {b}, dans {}. ", c.stint, c.stints, plural(d, "tour")),
            _ => t += "Dernier relais. ",
        }
        let v = match c.verdict {
            Verdict::Lift => "lift and coast",
            Verdict::OnTarget => "dans la cible",
            Verdict::Push => "pas besoin d'économiser",
        };
        t += &format!("Cible {} par tour, tu fais {} : {v}.", per(unit, c.target_per_lap), per(unit, c.per_lap));
        match (unit, c.fill, c.fuel_ratio) {
            (Unit::Energy, Some(f), Some(r)) => t += &format!(" Au stand : énergie {:.0} %, ratio {}.", up(f), format!("{r:.2}").replace('.', ",")),
            (Unit::Energy, Some(f), None) => t += &format!(" Au stand : énergie {:.0} %.", up(f)),
            (Unit::Litres, Some(f), _) => t += &format!(" Au stand : {:.0} litres.", up(f)),
            _ => {}
        }
        if c.fuel_limits {
            t += " Attention, le carburant limite avant l'énergie.";
        }
        t
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(level: f32, per_lap: f32, laps_left: u32, lap: i32) -> RaceState {
        RaceState { unit: Unit::Energy, level, cap: 100.0, per_lap, laps_left, lap, fuel_ratio: Some(0.86), fuel_laps: None, energy_laps: None }
    }

    /// Course de 6 h à Spa, 4,8 % par tour : l'ingénieur annonce le relais, puis appelle au stand.
    #[test]
    fn announces_the_stint_then_calls_the_box() {
        let mut e = Engineer::new(EngineerPlan { mode: EngineerMode::Attack, margin_laps: 0.5 });
        let first = e.on_lap(&state(95.2, 4.8, 104, 1)).unwrap();
        assert!(first.starts_with("Relais 1 sur 6. Box à la fin du tour"), "{first}");
        assert!(first.contains("pas besoin d'économiser, roule à ton rythme"), "{first}");
        assert_eq!(e.last().unwrap().stops_left, 5);
        // Tours suivants à la même consommation : on ne parle plus avant l'appel au stand.
        let (mut level, mut lap) = (95.2f32, 1);
        let mut said = vec![];
        while lap < 40 && !said.last().is_some_and(|(_, m): &(i32, String)| m.contains("Box, box")) {
            lap += 1;
            level -= 4.8;
            if let Some(m) = e.on_lap(&state(level, 4.8, (105 - lap) as u32, lap)) {
                said.push((lap, m));
            }
        }
        let (bl, last) = said.last().unwrap().clone();
        // 5 arrêts sans économiser laissent de la marge : relais suivant d'environ 17 tours, on ne remplit
        // pas à 100 % (arrêt plus court).
        let fill = e.last().unwrap().fill.unwrap();
        assert!(fill > 17.0 * 4.8 && fill < 95.0, "{fill}");
        assert!(last.contains(&format!("Box, box, ce tour. Règle l'énergie à {:.0} %, ratio 0,86, soit {:.0} litres.", up(fill), (fill * 0.86).round())), "{said:?}");
        assert!(said.iter().any(|(l, m)| *l == bl - 1 && m.contains("Box à la fin du prochain tour")), "{said:?}");
        assert_eq!(said.len(), 2, "{said:?}");
        // On arrive au stand avec la marge : le tour de l'arrêt se boucle sans tomber à 0.
        assert!(level - 4.8 >= 2.4 - 1e-3, "{level}");
        // Ravitaillement au stand : le relais suivant est annoncé.
        let low = (level - 4.8) as f64;
        for k in 0..40 {
            e.observe(low + k as f64 * 2.5);
        }
        e.observe(99.0);
        assert_eq!(e.stops_done(), 1);
        let m = e.on_lap(&state(97.0, 4.8, (105 - bl - 2) as u32, bl + 2)).unwrap();
        assert!(m.starts_with("Relais 2 sur 6"), "{m}");
    }

    #[test]
    fn asks_to_lift_when_the_plan_needs_it_and_says_when_back_on_target() {
        // Plan économie : un arrêt de moins si l'on économise ~3 %.
        let mut e = Engineer::new(EngineerPlan { mode: EngineerMode::Save, margin_laps: 0.5 });
        let m = e.on_lap(&state(97.6, 4.8, 104, 0)).unwrap();
        assert!(m.contains("lift and coast"), "{m}");
        assert_eq!(e.last().unwrap().verdict, Verdict::Lift);
        assert_eq!(e.last().unwrap().stops_left, 4);
        // Le pilote économise : retour dans la cible, dit une fois.
        let t = e.last().unwrap().target_per_lap;
        let m = e.on_lap(&state(97.6 - t, t, 103, 1)).unwrap();
        assert!(m.starts_with("Énergie : dans la cible"), "{m}");
        assert!(e.on_lap(&state(97.6 - 2.0 * t, t, 102, 2)).is_none());
        // Mode attaque sur la même course : pas d'économie demandée.
        let mut a = Engineer::new(EngineerPlan { mode: EngineerMode::Attack, margin_laps: 0.5 });
        a.on_lap(&state(97.6, 4.8, 104, 0));
        assert_eq!(a.last().unwrap().stops_left, 5);
        assert_ne!(a.last().unwrap().verdict, Verdict::Lift);
    }

    #[test]
    fn last_stint_with_margin_says_push_and_low_ratio_is_flagged() {
        let mut e = Engineer::new(EngineerPlan::default());
        // 10 tours à faire, 60 % à 4,8 : 12,5 tours possibles.
        let mut s = state(60.0, 4.8, 10, 40);
        s.fuel_laps = Some(9.0);
        s.energy_laps = Some(12.5);
        let m = e.on_lap(&s).unwrap();
        assert!(m.starts_with("Course sans arrêt, 10 tours"), "{m}");
        assert!(m.contains("de marge à l'arrivée : pas besoin d'économiser"), "{m}");
        assert!(m.contains("le carburant lâchera avant l'énergie"), "{m}");
        assert!(e.last().unwrap().fuel_limits);
        assert!(e.summary(Unit::Energy).contains("carburant limite"));
    }

    /// Confiance : à consommation constante, l'ingénieur appelle au stand aux tours du calculateur et
    /// fait régler les mêmes niveaux (6 h à Spa, 4,8 % par tour ; plan attaque et plan économie).
    #[test]
    fn live_calls_match_the_calculator() {
        use crate::analysis::strategy::{plan, PlanKind, RaceLength, StrategyInput};
        let s = plan(&StrategyInput { length: RaceLength::Laps(104), unit: Unit::Energy, per_lap: 4.8, lap_time_s: 210.0, tank: Some(100.0), margin_laps: 0.5, formation_laps: 0.5, fuel_ratio: None, fuel_tank: None, top_class: false, pit_loss_s: None });
        for (mode, kind) in [(EngineerMode::Attack, PlanKind::Attack), (EngineerMode::Save, PlanKind::Save)] {
            let o = s.options.iter().find(|o| o.kind == kind).unwrap();
            let c = if kind == PlanKind::Save { o.target_per_lap } else { 4.8 };
            let expected: Vec<(i32, i32)> = o.stints.windows(2).map(|w| (w[0].to_lap as i32, up(w[1].amount) as i32)).collect();
            let mut e = Engineer::new(EngineerPlan { mode, margin_laps: 0.5 });
            let (mut level, mut lap, mut got) = (100.0 - 0.5 * 4.8, 0i32, vec![]);
            while lap < 104 {
                let mut st = state(level, c, (104 - lap) as u32, lap);
                // Le plan économie est calculé sur ta consommation habituelle : l'ingénieur la connaît.
                st.per_lap = c;
                e.on_lap(&st);
                let call = e.last().unwrap().clone();
                lap += 1;
                level -= c;
                if call.laps_to_box == Some(1) {
                    let fill = call.fill.unwrap();
                    got.push((lap, up(fill) as i32));
                    for k in 0..=40 {
                        e.observe((level + (fill - level) * k as f32 / 40.0) as f64);
                    }
                    level = fill;
                }
                assert!(level >= 0.0, "{mode:?} tour {lap} : {level}");
            }
            assert_eq!(got, expected, "{mode:?}");
        }
    }

    #[test]
    fn does_not_flip_flop_between_plans() {
        // Juste à la limite d'un arrêt de moins (mode auto, 4 %) : un tour un peu plus gourmand ne
        // fait pas changer le plan, il demande juste un peu plus de lift.
        let mut e = Engineer::new(EngineerPlan::default());
        e.on_lap(&state(97.6, 4.8, 104, 0));
        let r = e.last().unwrap().stops_left;
        e.on_lap(&state(92.6, 4.85, 103, 1));
        assert_eq!(e.last().unwrap().stops_left, r);
    }
}
