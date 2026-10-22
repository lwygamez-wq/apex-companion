//! Client de l'API officielle de Garage 61 (https://garage61.net/api/v1), avec un jeton d'accès
//! personnel créé par le pilote sur garage61.net (page « Developer »).
//!
//! Utilisé uniquement si le pilote a saisi un jeton : c'est alors la seule communication de l'app
//! avec un service tiers. On n'envoie que des critères de recherche (circuit, voiture) et on reçoit
//! des tours ; aucune donnée de conduite du pilote n'est transmise.
//!
//! Limite imposée par Garage 61 (confidentialité) : la recherche ne couvre que tes tours, ceux de
//! tes coéquipiers et ceux des pilotes que tu suis (« following »). Pas de recherche globale.

use crate::ingest::garage61::G61Name;
use serde::Deserialize;
use std::time::Duration;

const BASE: &str = "https://garage61.net/api/v1";

#[derive(Debug, Clone, Deserialize)]
pub struct G61User {
    pub slug: String,
    #[serde(rename = "firstName")]
    pub first_name: Option<String>,
    #[serde(rename = "lastName")]
    pub last_name: Option<String>,
    #[serde(rename = "subscriptionPlan", default)]
    pub subscription_plan: Option<String>,
    #[serde(default)]
    pub teams: Vec<G61Team>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct G61Team {
    pub slug: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct G61Car {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub platform: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct G61Track {
    pub id: i64,
    pub name: String,
    pub variant: Option<String>,
    #[serde(default)]
    pub platform: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct G61Lap {
    pub id: String,
    pub driver: Option<G61Driver>,
    pub car: G61Car,
    pub track: G61Track,
    #[serde(rename = "lapTime")]
    pub lap_time: f64,
    #[serde(default)]
    pub clean: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct G61Driver {
    pub slug: String,
    #[serde(rename = "firstName")]
    pub first_name: Option<String>,
    #[serde(rename = "lastName")]
    pub last_name: Option<String>,
}

impl G61Driver {
    pub fn display(&self) -> String {
        let n = format!("{} {}", self.first_name.clone().unwrap_or_default(), self.last_name.clone().unwrap_or_default());
        if n.trim().is_empty() { self.slug.clone() } else { n.trim().to_string() }
    }
}

#[derive(Deserialize)]
struct Items<T> {
    items: Vec<T>,
}

pub struct G61Api {
    token: String,
    agent: ureq::Agent,
}

fn http_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(401) => "jeton Garage 61 refusé (expiré ou invalide) : recrée-le sur garage61.net".into(),
        ureq::Error::StatusCode(403) => "accès refusé par Garage 61 (droit « driving data » du jeton, ou abonnement requis)".into(),
        ureq::Error::StatusCode(429) => "trop de requêtes vers Garage 61 : réessaie dans quelques minutes".into(),
        ureq::Error::StatusCode(c) => format!("Garage 61 a répondu {c}"),
        e => format!("connexion à Garage 61 impossible ({e})"),
    }
}

impl G61Api {
    pub fn new(token: &str) -> Self {
        let agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(45))).build().into();
        Self { token: token.trim().to_string(), agent }
    }

    fn get(&self, path: &str, query: &[(&str, String)]) -> Result<String, String> {
        let mut req = self
            .agent
            .get(format!("{BASE}{path}"))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("User-Agent", "apex-companion");
        for (k, v) in query {
            req = req.query(*k, v);
        }
        let mut resp = req.call().map_err(http_err)?;
        resp.body_mut().with_config().limit(50 * 1024 * 1024).read_to_string().map_err(|e| format!("réponse Garage 61 illisible : {e}"))
    }

    fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str, query: &[(&str, String)]) -> Result<T, String> {
        let text = self.get(path, query)?;
        serde_json::from_str(&text).map_err(|e| format!("réponse Garage 61 inattendue ({path}) : {e}"))
    }

    pub fn me(&self) -> Result<G61User, String> {
        self.get_json("/me", &[])
    }
    pub fn cars(&self) -> Result<Vec<G61Car>, String> {
        Ok(self.get_json::<Items<G61Car>>("/cars", &[])?.items)
    }
    pub fn tracks(&self) -> Result<Vec<G61Track>, String> {
        Ok(self.get_json::<Items<G61Track>>("/tracks", &[])?.items)
    }

    /// Meilleur tour propre de chaque pilote suivi ou coéquipier, sur ce circuit et cette voiture.
    pub fn best_laps(&self, track_id: i64, car_id: i64, teams: &[String]) -> Result<Vec<G61Lap>, String> {
        let mut q = vec![
            ("tracks", track_id.to_string()),
            ("cars", car_id.to_string()),
            ("drivers", "following".to_string()),
            ("group", "driver".to_string()),
            ("lapTypes", "1".to_string()),
            ("limit", "100".to_string()),
        ];
        if !teams.is_empty() {
            q.push(("teams", teams.join(",")));
        }
        Ok(self.get_json::<Items<G61Lap>>("/laps", &q)?.items)
    }

    /// Recherche brute pour le diagnostic : `drivers` = "following", "me" ou vide (toi + coéquipiers).
    pub fn count_laps(&self, track_id: i64, car_id: Option<i64>, drivers: Option<&str>) -> Result<usize, String> {
        let mut q = vec![("tracks", track_id.to_string()), ("group", "driver".to_string()), ("limit", "100".to_string())];
        if let Some(c) = car_id {
            q.push(("cars", c.to_string()));
        }
        if let Some(d) = drivers {
            q.push(("drivers", d.to_string()));
        }
        Ok(self.get_json::<Items<G61Lap>>("/laps", &q)?.items.len())
    }

    pub fn lap_csv(&self, lap_id: &str) -> Result<String, String> {
        self.get(&format!("/laps/{lap_id}/csv"), &[])
    }
}

/// Correspondance des noms Apex (issus d'iRacing) avec le catalogue Garage 61.
pub fn match_track<'a>(tracks: &'a [G61Track], name: &str, config: Option<&str>) -> Option<&'a G61Track> {
    let f = crate::tracks::fold_name;
    let (n, c) = (f(name), f(config.unwrap_or("")));
    let ir = tracks.iter().filter(|t| t.platform.is_empty() || t.platform.eq_ignore_ascii_case("iracing"));
    let cands: Vec<&G61Track> = ir.filter(|t| f(&t.name) == n).collect();
    cands
        .iter()
        .find(|t| f(t.variant.as_deref().unwrap_or("")) == c)
        .or_else(|| cands.iter().find(|t| c.is_empty() && t.variant.as_deref().is_none_or(str::is_empty)))
        .or_else(|| (cands.len() == 1).then(|| &cands[0]))
        .copied()
}

pub fn match_car<'a>(cars: &'a [G61Car], name: &str) -> Option<&'a G61Car> {
    let tokens = |s: &str| crate::tracks::fold_name(s).split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()).map(String::from).collect::<Vec<_>>();
    let mine = tokens(name);
    let ir: Vec<&G61Car> = cars.iter().filter(|c| c.platform.is_empty() || c.platform.eq_ignore_ascii_case("iracing")).collect();
    ir.iter()
        .find(|c| tokens(&c.name) == mine)
        .or_else(|| ir.iter().filter(|c| { let t = tokens(&c.name); t.iter().all(|x| mine.contains(x)) || mine.iter().all(|x| t.contains(x)) }).max_by_key(|c| tokens(&c.name).len()))
        .copied()
}

/// Nom équivalent au fichier exporté depuis le site (même chaîne de traitement que l'import manuel).
pub fn lap_name(l: &G61Lap) -> G61Name {
    G61Name {
        driver: l.driver.as_ref().map(G61Driver::display).unwrap_or_else(|| "Pilote Garage 61".into()),
        car: l.car.name.clone(),
        track: format!("{} {}", l.track.name, l.track.variant.clone().unwrap_or_default()).trim().to_string(),
        lap_time_s: l.lap_time,
        id: l.id.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_documented_payloads_and_matches_names() {
        let tracks: Items<G61Track> = serde_json::from_str(r#"{"items":[
            {"id":1,"name":"Circuit of the Americas","variant":"Grand Prix","platform":"iracing"},
            {"id":2,"name":"Circuit of the Americas","variant":"East","platform":"iracing"},
            {"id":3,"name":"Spa","variant":null,"platform":"acc"}],"total":3}"#).unwrap();
        assert_eq!(match_track(&tracks.items, "Circuit of the Americas", Some("Grand Prix")).unwrap().id, 1);
        assert!(match_track(&tracks.items, "Monza", Some("Grand Prix")).is_none());
        let cars: Items<G61Car> = serde_json::from_str(r#"{"items":[
            {"id":10,"name":"Dallara F3","platform":"iracing"},
            {"id":11,"name":"Dallara IR18","platform":"iracing"}],"total":2}"#).unwrap();
        assert_eq!(match_car(&cars.items, "Dallara F312 F3").unwrap().id, 10);
        let laps: Items<G61Lap> = serde_json::from_str(r#"{"items":[{"id":"01M5","driver":{"slug":"luke-fleming","firstName":"Luke","lastName":"Fleming"},
            "car":{"id":10,"name":"Dallara F3","platform":"iracing"},"track":{"id":1,"name":"Circuit of the Americas","variant":"Grand Prix","platform":"iracing"},
            "lapTime":116.507,"startTime":"2026-10-01T10:00:00Z","clean":true}],"total":1}"#).unwrap();
        let n = lap_name(&laps.items[0]);
        assert_eq!((n.driver.as_str(), n.track.as_str()), ("Luke Fleming", "Circuit of the Americas Grand Prix"));
    }
}
