//! Sonde setup LMU (lecture seule) : ce que le jeu expose vraiment pour un futur Setup Builder.
//!
//! Deux sources, jamais d'écriture dans le jeu :
//! - la mémoire partagée : taux de remplissage des champs pneus, freins, hauteurs et aéro pendant
//!   que le pilote roule (un champ toujours à 0 n'est pas fourni par le jeu) ;
//! - l'API locale du jeu (`http://127.0.0.1:6397`), en GET seulement : réglages du garage avec
//!   leurs crans mini et maxi, setups fournis par le jeu, conditions idéales des gommes. Les
//!   requêtes d'écriture de cette API sont connues pour faire planter LMU : elles ne sont jamais
//!   envoyées.
//!
//! Le rapport reste sur le PC (`sonde-lmu/` dans le dossier de données d'Apex).

use super::lmu_shm::{Frame, VehicleTelemetry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const API_BASE: &str = "http://127.0.0.1:6397";

/// Lectures de l'API (GET seulement) : nom du fichier, chemin.
pub const API_READS: [(&str, &str); 10] = [
    ("garage", "/rest/garage/getPlayerGarageData"),
    ("setup-overview", "/rest/garage/UIScreen/CarSetupOverview"),
    ("tyres", "/rest/garage/UIScreen/TireManagement"),
    ("swagger", "/swagger-schema.json"),
    // Stratégie et classement vus par le jeu (contenu à étudier : audit du 13 nov.).
    ("strategy-overall", "/rest/strategy/overall"),
    ("strategy-usage", "/rest/strategy/usage"),
    ("strategy-pitstop", "/rest/strategy/pitstop-estimate"),
    ("standings", "/rest/watch/standings"),
    ("session-info", "/rest/watch/sessionInfo"),
    ("event-sessions", "/rest/sessions/GetSessionsInfoForEvent"),
];

/// Ce que le jeu annonce pour la voiture et le circuit chargés, avant même le premier tour.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GameFacts {
    pub track: String,
    pub car: String,
    pub car_class: Option<String>,
    /// Numéro de séance du jeu (10 et plus : course).
    pub session: i32,
    pub at_unix: i64,
    /// Durée de la séance (min), si elle est au temps.
    pub minutes: Option<f32>,
    /// Énergie virtuelle par tour estimée par le jeu (%).
    pub energy_per_lap_pct: Option<f32>,
    /// Carburant par tour estimé par le jeu (l).
    pub fuel_per_lap_l: Option<f32>,
    /// Ratio carburant réglé dans le garage (l par 1 % d'énergie).
    pub fuel_ratio: Option<f32>,
}

/// Consommations estimées par le jeu (`TireManagement.expectedUsage`) et ratio réglé (`VM_FUEL_LEVEL`).
pub fn game_usage(overview: Option<&str>, tyres: Option<&str>) -> (Option<f32>, Option<f32>, Option<f32>) {
    let pos = |v: Option<f64>| v.filter(|x| x.is_finite() && *x > 0.0).map(|x| x as f32);
    let usage = tyres.and_then(|t| serde_json::from_str::<Value>(t).ok()).and_then(|t| find_key(&t, "expectedUsage").cloned());
    let energy = pos(usage.as_ref().and_then(|u| u.get("virtualEnergyFractionPerLap")).and_then(Value::as_f64)).map(|f| f * 100.0);
    let fuel = pos(usage.as_ref().and_then(|u| u.get("fuelConsumption")).and_then(Value::as_f64));
    // Hypercar et GT3 : le réglage « carburant » affiche le ratio (« 0.73 ») ; ailleurs des litres (« 75.0l »).
    let ratio = overview
        .and_then(|o| serde_json::from_str::<Value>(o).ok())
        .and_then(|o| find_key(&o, "VM_FUEL_LEVEL").and_then(|v| v.get("stringValue")).and_then(Value::as_str).map(str::to_string))
        .and_then(|s| s.trim().replace(',', ".").parse::<f32>().ok())
        .filter(|r| energy.is_some() && *r > 0.05 && *r < 3.0);
    (energy, fuel, ratio)
}

/// Lit dans le jeu ce qu'il annonce pour la séance chargée (deux GET), et le garde dans `jeu.json`.
pub fn read_game_facts(base: &str, dir: &Path, mut facts: GameFacts) -> Option<GameFacts> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(4))).proxy(None).build().into();
    let get = |path: &str| -> Option<String> {
        let mut resp = agent.get(format!("{base}{path}")).call().ok()?;
        let b = resp.body_mut().with_config().limit(20 * 1024 * 1024).read_to_vec().ok()?;
        Some(String::from_utf8(b.clone()).unwrap_or_else(|_| b.iter().map(|&c| c as char).collect()))
    };
    let tyres = get("/rest/garage/UIScreen/TireManagement")?;
    let overview = get("/rest/garage/UIScreen/CarSetupOverview");
    (facts.energy_per_lap_pct, facts.fuel_per_lap_l, facts.fuel_ratio) = game_usage(overview.as_deref(), Some(&tyres));
    std::fs::create_dir_all(dir).ok()?;
    std::fs::write(dir.join("jeu.json"), serde_json::to_string_pretty(&facts).ok()?).ok()?;
    Some(facts)
}

/// Dernière séance vue dans le jeu (`jeu.json`).
pub fn load_game_facts(dir: &Path) -> Option<GameFacts> {
    serde_json::from_str(&std::fs::read_to_string(dir.join("jeu.json")).ok()?).ok()
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct FieldStat {
    /// Images où le champ est non nul (et fini).
    pub filled: u64,
    pub min: f64,
    pub max: f64,
    pub sum: f64,
}

impl FieldStat {
    fn add(&mut self, v: f64) {
        if !v.is_finite() || v == 0.0 {
            return;
        }
        if self.filled == 0 {
            self.min = v;
            self.max = v;
        } else {
            self.min = self.min.min(v);
            self.max = self.max.max(v);
        }
        self.filled += 1;
        self.sum += v;
    }
}

/// Accumulateur de la mémoire partagée, image par image, voiture en piste.
#[derive(Debug, Clone, Default)]
pub struct ShmStats {
    pub frames: u64,
    pub fields: BTreeMap<&'static str, FieldStat>,
}

const K: f64 = 273.15;

impl ShmStats {
    pub fn add(&mut self, t: &VehicleTelemetry) {
        self.frames += 1;
        let mut put = |name: &'static str, v: f64| self.fields.entry(name).or_default().add(v);
        for w in t.mWheels.iter() {
            // Températures en kelvin dans le jeu : converties en °C (0 K reste « vide »).
            let c = |k: f64| if k > 0.0 { k - K } else { 0.0 };
            put("pneu_surface_c", c((w.mTemperature[0] + w.mTemperature[1] + w.mTemperature[2]) / 3.0));
            put("pneu_ecart_int_ext_c", if w.mTemperature[0] > 0.0 { w.mTemperature[0] - w.mTemperature[2] } else { 0.0 });
            put("pneu_couche_interne_c", c((w.mTireInnerLayerTemperature[0] + w.mTireInnerLayerTemperature[1] + w.mTireInnerLayerTemperature[2]) / 3.0));
            put("pneu_carcasse_c", c(w.mTireCarcassTemperature));
            put("pneu_temperature_optimale", w.mOptimalTemp as f64);
            put("pneu_pression_kpa", w.mPressure);
            put("pneu_usure", w.mWear);
            put("pneu_charge_n", w.mTireLoad);
            put("pneu_adherence_fraction", w.mGripFract);
            put("pneu_index_gomme", w.mCompoundIndex as f64 + 1.0);
            put("frein_temperature_c", c(w.mBrakeTemp));
            put("roue_hauteur_m", w.mRideHeight);
            put("roue_debattement_m", w.mSuspensionDeflection);
            put("roue_carrossage_rad", w.mCamber);
            put("roue_pincement", w.mToe);
        }
        put("caisse_hauteur_av_m", t.mFrontRideHeight);
        put("caisse_hauteur_ar_m", t.mRearRideHeight);
        put("aileron_av_hauteur_m", t.mFrontWingHeight);
        put("appui_av_n", t.mFrontDownforce);
        put("appui_ar_n", t.mRearDownforce);
        put("trainee_n", t.mDrag);
        put("troisieme_element_av_m", t.mFront3rdDeflection);
        put("troisieme_element_ar_m", t.mRear3rdDeflection);
        put("repartition_frein_ar", t.mRearBrakeBias);
        put("barre_av_cran", t.mFrontAntiSway as f64);
        put("barre_av_max", t.mFrontAntiSwayMax as f64);
        put("barre_ar_cran", t.mRearAntiSway as f64);
        put("barre_ar_max", t.mRearAntiSwayMax as f64);
        put("tc_max", t.mTCMax as f64);
        put("abs_max", t.mABSMax as f64);
        put("carto_moteur_max", t.mMotorMapMax as f64);
        put("migration_max", t.mMigrationMax as f64);
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FieldReport {
    pub name: String,
    /// Part des images (voiture en piste) où le jeu remplit le champ, en %.
    pub filled_pct: f64,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiRead {
    pub name: String,
    pub path: String,
    pub ok: bool,
    pub bytes: usize,
    pub error: Option<String>,
}

/// Ce que l'API dit des réglages : de quoi trancher les conditions du Setup Builder.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GarageSummary {
    /// Réglages `VM_*` trouvés.
    pub settings: usize,
    /// … dont ceux qui ont un cran mini et maxi.
    pub with_range: usize,
    /// Setups fournis par le jeu (`carPresetSetups`).
    pub presets: Option<usize>,
    /// Conditions idéales des gommes (`optimalCompoundConditions`) présentes et non vides.
    pub optimal_conditions: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProbeReport {
    pub stem: String,
    pub car: String,
    pub track: String,
    pub game_version: i32,
    pub frames: u64,
    pub fields: Vec<FieldReport>,
    pub api: Vec<ApiRead>,
    pub garage: GarageSummary,
}

impl ProbeReport {
    pub fn new(stem: &str, f: &Frame, stats: &ShmStats) -> Self {
        let fields = stats
            .fields
            .iter()
            .map(|(name, s)| FieldReport {
                name: name.to_string(),
                // 4 roues par image pour les champs de roue.
                filled_pct: {
                    let per = if ["pneu_", "frein_", "roue_"].iter().any(|p| name.starts_with(p)) { 4 } else { 1 };
                    if stats.frames == 0 { 0.0 } else { (s.filled as f64 / (stats.frames * per) as f64 * 1000.0).round() / 10.0 }
                },
                min: s.min,
                max: s.max,
                mean: if s.filled > 0 { s.sum / s.filled as f64 } else { 0.0 },
            })
            .collect();
        Self {
            stem: stem.into(),
            car: f.telem.as_ref().map(crate::ingest::lmu::car_name).unwrap_or_default(),
            track: crate::ingest::lmu::track_name(f),
            game_version: f.game_version,
            frames: stats.frames,
            fields,
            ..Default::default()
        }
    }
}

/// Parcourt la réponse de `getPlayerGarageData` : objets dont la clé commence par `VM_`.
fn walk_vm(v: &Value, out: &mut GarageSummary) {
    match v {
        Value::Object(o) => {
            for (k, x) in o {
                if k.starts_with("VM_") && x.is_object() {
                    out.settings += 1;
                    if x.get("minValue").is_some_and(|m| m.is_number()) && x.get("maxValue").is_some_and(|m| m.is_number()) {
                        out.with_range += 1;
                    }
                } else {
                    walk_vm(x, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| walk_vm(x, out)),
        _ => {}
    }
}

fn find_key<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    match v {
        Value::Object(o) => o.get(key).or_else(|| o.values().find_map(|x| find_key(x, key))),
        Value::Array(a) => a.iter().find_map(|x| find_key(x, key)),
        _ => None,
    }
}

pub fn summarize(garage: Option<&str>, overview: Option<&str>, tyres: Option<&str>) -> GarageSummary {
    let mut s = GarageSummary::default();
    if let Some(v) = garage.and_then(|t| serde_json::from_str::<Value>(t).ok()) {
        walk_vm(&v, &mut s);
    }
    if let Some(v) = overview.and_then(|t| serde_json::from_str::<Value>(t).ok()) {
        // `carPresetSetups` : `{ "presets": [...] }` dans LMU (12 nov. 2026), parfois une liste directe.
        s.presets = find_key(&v, "carPresetSetups").map(|p| match p {
            Value::Array(a) => a.len(),
            Value::Object(o) => o.get("presets").and_then(Value::as_array).map_or(o.len(), Vec::len),
            _ => 0,
        });
    }
    if let Some(v) = tyres.and_then(|t| serde_json::from_str::<Value>(t).ok()) {
        s.optimal_conditions = find_key(&v, "optimalCompoundConditions").map(|c| match c {
            Value::Null => false,
            Value::Array(a) => !a.is_empty(),
            Value::Object(o) => !o.is_empty(),
            Value::String(t) => !t.is_empty(),
            _ => true,
        });
    }
    s
}

/// Lit l'API du jeu (GET seulement) et enregistre chaque réponse brute dans `dir/<stem>/`.
/// Setup de la séance d'après `CarSetupOverview` (`carSetup.garageValues` : réglages `VM_*` de la
/// voiture et `WM_*-W_FL`… par roue) et `TireManagement` (température idéale par gomme).
pub fn setup_snapshot(overview: &str, tyres: Option<&str>) -> Option<crate::ingest::lmu::SetupSnapshot> {
    use crate::ingest::lmu::{SetupSnapshot, SetupValue};
    let v: Value = serde_json::from_str(overview).ok()?;
    let gv = find_key(&v, "garageValues")?.as_object()?;
    let int = |x: &Value, k: &str| x.get(k).and_then(Value::as_i64).map(|n| n as i32);
    let mut values: Vec<SetupValue> = gv
        .iter()
        .filter_map(|(key, x)| {
            Some(SetupValue { key: key.clone(), value: int(x, "value")?, display: x.get("stringValue").and_then(Value::as_str).unwrap_or_default().trim().to_string(), min: int(x, "minValue").unwrap_or(0), max: int(x, "maxValue").unwrap_or(0) })
        })
        .collect();
    values.sort_by(|a, b| a.key.cmp(&b.key));
    let presets = find_key(&v, "carPresetSetups")
        .and_then(|p| p.get("presets").or(Some(p)))
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|p| p.get("translated").or(p.get("unTranslated")).and_then(Value::as_str).map(str::to_string)).collect())
        .unwrap_or_default();
    let optimal_temp_c = tyres
        .and_then(|t| serde_json::from_str::<Value>(t).ok())
        .and_then(|t| find_key(&t, "optimalCompoundConditions").and_then(|c| c.get("compounds")).and_then(Value::as_array).cloned())
        .map(|a| a.iter().filter_map(|c| Some((c.get("type")?.as_str()?.to_string(), c.get("optimalTemperature")?.as_f64()? as f32))).collect())
        .unwrap_or_default();
    (!values.is_empty()).then_some(SetupSnapshot { values, presets, optimal_temp_c })
}

pub fn read_api(base: &str, dir: &Path, stem: &str) -> (Vec<ApiRead>, GarageSummary, Option<crate::ingest::lmu::SetupSnapshot>) {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(4))).proxy(None).build().into();
    let out_dir = dir.join(stem);
    let _ = std::fs::create_dir_all(&out_dir);
    let mut reads = vec![];
    let mut bodies: BTreeMap<&str, String> = BTreeMap::new();
    for (name, path) in API_READS {
        let mut r = ApiRead { name: name.into(), path: path.into(), ..Default::default() };
        match agent.get(format!("{base}{path}")).call() {
            Ok(mut resp) => match resp.body_mut().with_config().limit(20 * 1024 * 1024).read_to_vec() {
                Ok(b) => {
                    // Réponses en text/plain sans jeu de caractères : UTF-8, sinon Latin-1.
                    let text = String::from_utf8(b.clone()).unwrap_or_else(|_| b.iter().map(|&c| c as char).collect());
                    r.ok = true;
                    r.bytes = b.len();
                    let _ = std::fs::write(out_dir.join(format!("{name}.json")), &text);
                    bodies.insert(name, text);
                }
                Err(e) => r.error = Some(e.to_string()),
            },
            Err(e) => r.error = Some(e.to_string()),
        }
        reads.push(r);
    }
    let summary = summarize(bodies.get("garage").map(String::as_str), bodies.get("setup-overview").map(String::as_str), bodies.get("tyres").map(String::as_str));
    let setup = bodies.get("setup-overview").and_then(|o| setup_snapshot(o, bodies.get("tyres").map(String::as_str)));
    (reads, summary, setup)
}

pub fn write_report(dir: &Path, r: &ProbeReport) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let p = dir.join(format!("{}.json", r.stem));
    std::fs::write(&p, serde_json::to_string_pretty(r).unwrap_or_default())?;
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::lmu_shm::zeroed;

    #[test]
    fn counts_filled_fields_per_wheel() {
        let mut s = ShmStats::default();
        let mut t: VehicleTelemetry = zeroed();
        for f in 0..10 {
            for w in t.mWheels.iter_mut() {
                w.mTemperature = [K + 90.0, K + 85.0, K + 80.0];
                w.mPressure = 140.0;
                // Température optimale : vide (le jeu ne la remplit pas dans ce test).
            }
            t.mFrontDownforce = if f < 5 { 0.0 } else { 5000.0 };
            s.add(&t);
        }
        assert_eq!(s.frames, 10);
        let surf = &s.fields["pneu_surface_c"];
        assert_eq!(surf.filled, 40);
        assert!((surf.min - 85.0).abs() < 1e-9);
        assert!((s.fields["pneu_ecart_int_ext_c"].max - 10.0).abs() < 1e-9);
        assert_eq!(s.fields["pneu_temperature_optimale"].filled, 0);
        assert_eq!(s.fields["appui_av_n"].filled, 5);
        let mut f: Frame = Frame { game_version: 1, info: zeroed(), scoring: None, telem: Some(t) };
        f.game_version = 14000;
        let r = ProbeReport::new("x", &f, &s);
        let pct = |n: &str| r.fields.iter().find(|x| x.name == n).unwrap().filled_pct;
        assert_eq!(pct("pneu_surface_c"), 100.0);
        assert_eq!(pct("appui_av_n"), 50.0);
        assert_eq!(pct("pneu_temperature_optimale"), 0.0);
    }

    #[test]
    fn summarizes_garage_api_answers() {
        let garage = r#"{"VM_BRAKE_BALANCE":{"value":30,"stringValue":"49.5:50.5","minValue":0,"maxValue":60},"carSetup":{"VM_REAR_WING":{"value":18,"stringValue":"P19"}}}"#;
        let overview = r#"{"carSetup":{},"carPresetSetups":[{"name":"Safe"},{"name":"Fast"}]}"#;
        let tyres = r#"{"tireInventory":[],"optimalCompoundConditions":[]}"#;
        let s = summarize(Some(garage), Some(overview), Some(tyres));
        assert_eq!(s, GarageSummary { settings: 2, with_range: 1, presets: Some(2), optimal_conditions: Some(false) });
        assert_eq!(summarize(Some("pas du json"), None, None), GarageSummary::default());
    }

    #[test]
    fn api_absent_is_reported_not_fatal() {
        let d = tempfile::tempdir().unwrap();
        // Port fermé : chaque lecture échoue proprement.
        let (reads, s, setup) = read_api("http://127.0.0.1:9", d.path(), "x");
        assert!(setup.is_none());
        assert_eq!(reads.len(), API_READS.len());
        assert!(reads.iter().all(|r| !r.ok && r.error.is_some()));
        assert_eq!(s, GarageSummary::default());
    }

    #[test]
    fn reads_the_setup_of_the_session() {
        // Forme des réponses de LMU (12 nov. 2026), valeurs inventées.
        let overview = r#"{"carPresetSetups":{"presets":[{"translated":"fast","unTranslated":"fast"},{"translated":"safe"}]},"carSetup":{"garageValues":{"VM_REAR_WING":{"value":17,"stringValue":"P18","minValue":0,"maxValue":18,"available":true},"WM_CAMBER-W_FL":{"value":27,"stringValue":"-1.3 deg","minValue":0,"maxValue":31},"gearGraph":{"value":null}}}}"#;
        let tyres = r#"{"optimalCompoundConditions":{"compounds":[{"type":"Medium","optimalTemperature":92.0}]}}"#;
        let s = setup_snapshot(overview, Some(tyres)).unwrap();
        assert_eq!(s.values.len(), 2);
        assert_eq!((s.values[0].key.as_str(), s.values[0].value, s.values[0].max), ("VM_REAR_WING", 17, 18));
        assert_eq!((s.values[1].display.as_str(), s.values[1].max), ("-1.3 deg", 31));
        assert_eq!(s.presets, ["fast", "safe"]);
        assert_eq!(s.optimal_temp_c, [("Medium".to_string(), 92.0)]);
        assert_eq!(summarize(None, Some(overview), None).presets, Some(2));
        assert!(setup_snapshot("{}", None).is_none());
    }

    #[test]
    fn reads_what_the_game_expects_before_the_first_lap() {
        // Forme des réponses de LMU ; chiffres de la Cadillac à Bahreïn (12 nov. 2026).
        let tyres = r#"{"expectedUsage":{"fuelConsumption":4.6135,"fuelFractionPerLap":0.0419,"virtualEnergyFractionPerLap":0.0443}}"#;
        let overview = r#"{"carSetup":{"garageValues":{"VM_FUEL_LEVEL":{"value":60,"stringValue":"0.73"}}}}"#;
        let (e, f, r) = game_usage(Some(overview), Some(tyres));
        assert!((e.unwrap() - 4.43).abs() < 1e-3 && (f.unwrap() - 4.6135).abs() < 1e-4);
        assert_eq!(r, Some(0.73));
        // Voiture sans énergie virtuelle : le réglage affiche des litres, pas un ratio.
        let (e, _, r) = game_usage(Some(r#"{"VM_FUEL_LEVEL":{"stringValue":"75.0l"}}"#), Some(r#"{"expectedUsage":{"fuelConsumption":2.9,"virtualEnergyFractionPerLap":0.0}}"#));
        assert_eq!((e, r), (None, None));
        let d = tempfile::tempdir().unwrap();
        assert!(load_game_facts(d.path()).is_none());
        assert!(read_game_facts("http://127.0.0.1:9", d.path(), GameFacts::default()).is_none());
    }
}
