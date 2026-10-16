//! Couche de service applicative : toute la logique des commandes UI,
//! indépendante de Tauri (donc testable sans WebView).

use crate::analysis::compare::{compare_laps, Comparison, LapInput};
use crate::analysis::progress::{progress_zones, ProgressZone, SessionLap};
use crate::analysis::session::{best_clean_lap, consistency, theoretical_best, Consistency};
use crate::connectors::{registry, Capabilities, ConnectorConfig, InstallStatus, SimulatorConnector};
use crate::mock::demo_session;
use crate::model::*;
use crate::storage::{InsertOutcome, LapRow, SessionFilter, SessionSummary, Store, StoreError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    User(String),
}

impl Serialize for ServiceError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ServiceError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    pub iracing_telemetry_dir: Option<String>,
    pub units: String,
    pub live_enabled: bool,
    pub sample_rate_hz: u32,
    pub auto_import: bool,
    pub coach_in_race: bool,
    pub coach_audio: bool,
    pub coach_min_interval_s: u32,
    pub log_level: String,
    /// Toujours false dans cette version : aucune fonctionnalité cloud n'existe.
    pub cloud_enabled: bool,
    /// Mode débutant : 3 priorités maximum, régularité avant vitesse pure.
    pub beginner_mode: bool,
    /// Vérifie au démarrage si une nouvelle version existe (seule requête réseau de l'app).
    pub check_updates: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            iracing_telemetry_dir: None,
            units: "metric".into(),
            live_enabled: true,
            sample_rate_hz: 60,
            auto_import: true,
            coach_in_race: false,
            coach_audio: false,
            coach_min_interval_s: 8,
            log_level: "info".into(),
            cloud_enabled: false,
            beginner_mode: true,
            check_updates: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectorInfo {
    pub id: SimId,
    pub name: String,
    pub process_names: Vec<String>,
    pub capabilities: Capabilities,
    pub install: InstallStatus,
    pub import_extensions: Vec<String>,
    pub default_import_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    pub path: String,
    pub outcome: Option<InsertOutcome>,
    pub laps: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct SyncReport {
    pub imported: usize,
    pub duplicates: usize,
    /// Fichiers encore en cours d'écriture (session en cours).
    pub pending: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionDetail {
    pub id: i64,
    pub meta: SessionMeta,
    pub laps: Vec<LapRow>,
    pub best_lap_id: Option<i64>,
    pub theoretical_best_s: Option<f64>,
    pub consistency: Option<Consistency>,
    /// Référence personnelle du même combo (sim/circuit/config/voiture), si elle existe.
    pub personal_reference_lap_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Dashboard {
    pub last_session: Option<SessionSummary>,
    pub theoretical_best_s: Option<f64>,
    pub progress_zones: Vec<ProgressZone>,
    /// (début de session, écart-type des tours propres) pour les sessions du même combo.
    pub consistency_trend: Vec<(i64, f64)>,
    pub next_goal: Option<String>,
    pub total_sessions: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostics {
    pub app_version: String,
    pub os: String,
    pub arch: String,
    pub sessions: usize,
    pub connectors: Vec<(String, Capabilities)>,
    pub note: String,
}

pub struct AppService {
    store: Mutex<Store>,
    connectors: Vec<Box<dyn SimulatorConnector>>,
}

const SETTINGS_KEY: &str = "app_settings";

/// 66.052 → « 1:06.052 »
pub fn fmt_lap(s: f64) -> String {
    let ms = (s * 1000.0).round() as i64;
    format!("{}:{:06.3}", ms / 60_000, (ms % 60_000) as f64 / 1000.0)
}

impl AppService {
    pub fn open(db: &Path) -> Result<Self> {
        if let Some(dir) = db.parent() {
            std::fs::create_dir_all(dir).map_err(|e| ServiceError::User(format!("dossier de données : {e}")))?;
        }
        Ok(Self { store: Mutex::new(Store::open(db)?), connectors: registry() })
    }

    pub fn in_memory() -> Result<Self> {
        Ok(Self { store: Mutex::new(Store::open_in_memory()?), connectors: registry() })
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Store> {
        // Un panic pendant une requête ne doit pas bloquer l'appli : on récupère le verrou.
        self.store.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn connectors(&self) -> &[Box<dyn SimulatorConnector>] {
        &self.connectors
    }

    pub fn settings(&self) -> AppSettings {
        self.db()
            .get_setting(SETTINGS_KEY)
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, mut s: AppSettings) -> Result<AppSettings> {
        if !(10..=120).contains(&s.sample_rate_hz) {
            return Err(ServiceError::User("fréquence d'échantillonnage : 10 à 120 Hz".into()));
        }
        if !matches!(s.units.as_str(), "metric" | "imperial") {
            return Err(ServiceError::User("unités : metric ou imperial".into()));
        }
        s.cloud_enabled = false;
        s.coach_min_interval_s = s.coach_min_interval_s.clamp(3, 60);
        self.db().set_setting(SETTINGS_KEY, &serde_json::to_string(&s).unwrap())?;
        Ok(s)
    }

    pub fn connector_config(&self, id: SimId) -> ConnectorConfig {
        let s = self.settings();
        ConnectorConfig {
            telemetry_dir: match id {
                SimId::Iracing => s.iracing_telemetry_dir.map(PathBuf::from),
                _ => None,
            },
            live_enabled: s.live_enabled,
            sample_rate_hz: s.sample_rate_hz,
            ..Default::default()
        }
    }

    pub fn connectors_info(&self) -> Vec<ConnectorInfo> {
        self.connectors
            .iter()
            .map(|c| {
                let cfg = self.connector_config(c.id());
                ConnectorInfo {
                    id: c.id(),
                    name: c.display_name().into(),
                    process_names: c.process_names().iter().map(|s| s.to_string()).collect(),
                    capabilities: c.capabilities(),
                    install: c.detect_installation(&cfg),
                    import_extensions: c.importer().map(|i| i.extensions().iter().map(|s| s.to_string()).collect()).unwrap_or_default(),
                    default_import_dir: c
                        .importer()
                        .and_then(|i| cfg.telemetry_dir.clone().or_else(|| i.default_dir()))
                        .map(|p| p.to_string_lossy().into_owned()),
                }
            })
            .collect()
    }

    pub fn list_sessions(&self, f: &SessionFilter) -> Result<Vec<SessionSummary>> {
        Ok(self.db().list_sessions(f)?)
    }

    pub fn import_paths(&self, paths: &[PathBuf]) -> Vec<ImportReport> {
        paths
            .iter()
            .map(|p| {
                let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
                let importer = self.connectors.iter().filter_map(|c| c.importer()).find(|i| i.extensions().contains(&ext.as_str()));
                let path = p.to_string_lossy().into_owned();
                let Some(imp) = importer else {
                    return ImportReport { path, outcome: None, laps: 0, error: Some(format!("format .{ext} non pris en charge")) };
                };
                match imp.import_file(p) {
                    Ok(s) => match self.db().insert_session(&s) {
                        Ok(o) => ImportReport { path, outcome: Some(o), laps: s.laps.len(), error: None },
                        Err(e) => ImportReport { path, outcome: None, laps: 0, error: Some(e.to_string()) },
                    },
                    Err(e) => ImportReport { path, outcome: None, laps: 0, error: Some(e.to_string()) },
                }
            })
            .collect()
    }

    /// Dossier de télémétrie iRacing (réglage, sinon emplacement par défaut).
    pub fn iracing_dir(&self) -> Result<PathBuf> {
        let c = self.connectors.iter().find(|c| c.id() == SimId::Iracing).expect("connecteur iRacing");
        let imp = c.importer().expect("import iRacing");
        let dir = self
            .connector_config(SimId::Iracing)
            .telemetry_dir
            .or_else(|| imp.default_dir())
            .ok_or_else(|| ServiceError::User("dossier de télémétrie iRacing non configuré".into()))?;
        if !dir.exists() {
            return Err(ServiceError::User(format!("dossier introuvable : {}", dir.display())));
        }
        Ok(dir)
    }

    fn ibt_files(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        let c = self.connectors.iter().find(|c| c.id() == SimId::Iracing).expect("connecteur iRacing");
        c.importer().expect("import iRacing").scan(dir).map_err(|e| ServiceError::User(e.to_string()))
    }

    /// Import manuel de tout le dossier iRacing (les doublons sont ignorés).
    pub fn scan_iracing(&self) -> Result<Vec<ImportReport>> {
        let dir = self.iracing_dir()?;
        let files = self.ibt_files(&dir)?;
        Ok(self.import_paths(&files))
    }

    /// Synchro automatique : importe chaque `.ibt` nouveau ou modifié, une fois terminé.
    ///
    /// Un fichier est considéré terminé quand il n'a pas été modifié depuis `min_age`
    /// (iRacing l'écrit en continu pendant la session). Les fichiers déjà traités
    /// (même taille, même date) ne sont jamais reparsés, y compris ceux en erreur.
    pub fn sync_iracing_folder(&self, min_age: std::time::Duration) -> Result<SyncReport> {
        let dir = self.iracing_dir()?;
        let now = std::time::SystemTime::now();
        let mut rep = SyncReport::default();
        for p in self.ibt_files(&dir)? {
            let Ok(meta) = std::fs::metadata(&p) else { continue };
            let Ok(modified) = meta.modified() else { continue };
            let size = meta.len() as i64;
            let mtime = modified.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
            let key = p.to_string_lossy().into_owned();
            if now.duration_since(modified).unwrap_or_default() < min_age {
                rep.pending += 1;
                continue;
            }
            if self.db().file_state(&key)? == Some((size, mtime)) {
                continue;
            }
            let r = self.import_paths(std::slice::from_ref(&p)).remove(0);
            let sid = match r.outcome {
                Some(InsertOutcome::Inserted(id)) => {
                    rep.imported += 1;
                    Some(id)
                }
                Some(InsertOutcome::Duplicate(id)) => {
                    rep.duplicates += 1;
                    Some(id)
                }
                None => None,
            };
            if let Some(e) = &r.error {
                rep.errors.push(format!("{} : {e}", p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
            }
            self.db().record_file(&key, size, mtime, sid, r.error.as_deref())?;
        }
        Ok(rep)
    }

    pub fn load_demo(&self) -> Result<InsertOutcome> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        let n = self.db().list_sessions(&SessionFilter { sim: Some(SimId::Demo), ..Default::default() })?.len() as u64;
        Ok(self.db().insert_session(&demo_session(1000 + n, now - 3600 * 24 * (3 - n.min(3) as i64)))?)
    }

    pub fn session_detail(&self, id: i64) -> Result<SessionDetail> {
        let db = self.db();
        let meta = db.session_meta(id)?;
        let laps = db.laps(id)?;
        let metas: Vec<LapMeta> = laps.iter().map(|l| l.meta.clone()).collect();
        let best = best_clean_lap(&metas);
        let traces: Vec<(LapMeta, LapTrace)> = laps
            .iter()
            .filter(|l| l.clean)
            .filter_map(|l| db.trace(l.id).ok().map(|t| (l.meta.clone(), t)))
            .collect();
        let pairs: Vec<(&LapMeta, &LapTrace)> = traces.iter().map(|(m, t)| (m, t)).collect();
        let length = meta.track_length_m.or_else(|| traces.first().map(|(_, t)| t.max_dist()));
        let theo = length.and_then(|l| theoretical_best(&pairs, l, 2.0)).map(|(t, _)| t);
        let personal = Self::personal_reference(&db, &meta)?;
        Ok(SessionDetail {
            id,
            best_lap_id: best.map(|i| laps[i].id),
            theoretical_best_s: theo,
            consistency: consistency(&metas),
            personal_reference_lap_id: personal,
            meta,
            laps,
        })
    }

    fn personal_reference(db: &Store, meta: &SessionMeta) -> Result<Option<i64>> {
        let f = SessionFilter { sim: Some(meta.sim), track: Some(meta.track.clone()), car: Some(meta.car.clone()), ..Default::default() };
        for s in db.list_sessions(&f)? {
            if s.track_config != meta.track_config {
                continue;
            }
            if let Some(l) = db.laps(s.id)?.into_iter().find(|l| l.is_reference) {
                return Ok(Some(l.id));
            }
        }
        Ok(None)
    }

    pub fn compare(&self, ref_lap: i64, cmp_lap: i64, step_m: f32) -> Result<Comparison> {
        if !(0.5..=10.0).contains(&step_m) {
            return Err(ServiceError::User("pas de comparaison : 0,5 à 10 m".into()));
        }
        let db = self.db();
        let (lr, lc) = (db.lap(ref_lap)?, db.lap(cmp_lap)?);
        let (mr, mc) = (db.session_meta(lr.session_id)?, db.session_meta(lc.session_id)?);
        let (tr, tc) = (db.trace(ref_lap)?, db.trace(cmp_lap)?);
        compare_laps(
            &LapInput { lap_id: ref_lap, session: &mr, lap: &lr.meta, trace: &tr },
            &LapInput { lap_id: cmp_lap, session: &mc, lap: &lc.meta, trace: &tc },
            step_m,
        )
        .map_err(|e| ServiceError::User(format!("comparaison impossible : {e}")))
    }

    pub fn set_reference(&self, lap_id: i64) -> Result<()> {
        Ok(self.db().set_reference(lap_id)?)
    }

    pub fn delete_session(&self, id: i64) -> Result<()> {
        Ok(self.db().delete_session(id)?)
    }

    pub fn delete_all(&self) -> Result<()> {
        Ok(self.db().delete_all()?)
    }

    pub fn dashboard(&self) -> Result<Dashboard> {
        let all = self.list_sessions(&SessionFilter::default())?;
        let Some(last) = all.first().cloned() else {
            return Ok(Dashboard { last_session: None, theoretical_best_s: None, progress_zones: vec![], consistency_trend: vec![], next_goal: None, total_sessions: 0 });
        };
        let detail = self.session_detail(last.id)?;
        let zones = {
            let db = self.db();
            let traces: Vec<(i64, LapMeta, LapTrace)> = detail
                .laps
                .iter()
                .filter(|l| l.clean)
                .filter_map(|l| db.trace(l.id).ok().map(|t| (l.id, l.meta.clone(), t)))
                .collect();
            drop(db);
            let laps: Vec<SessionLap> = traces.iter().map(|(id, m, t)| SessionLap { lap_id: *id, meta: m, trace: t }).collect();
            match detail.best_lap_id.and_then(|b| laps.iter().position(|l| l.lap_id == b)) {
                Some(bi) => progress_zones(&detail.meta, &laps[bi], &laps, 2.0),
                None => vec![],
            }
        };
        let mut trend: Vec<(i64, f64)> = all
            .iter()
            .filter(|s| s.sim == last.sim && s.track == last.track && s.car == last.car)
            .filter_map(|s| s.stddev_clean_s.map(|d| (s.started_at_unix, d)))
            .collect();
        trend.sort_by_key(|t| t.0);
        let beginner = self.settings().beginner_mode;
        let flying = last.lap_count.saturating_sub(2).max(1);
        // Mode débutant : rester sur la piste, puis être régulier, avant de chercher les dixièmes.
        let beginner_goal = if !beginner {
            None
        } else if last.incident_laps * 3 >= flying && last.incident_laps >= 2 {
            Some(format!(
                "Priorité : rester sur la piste. {} tour(s) sur {} ont eu une sortie ou un incident. Roule à 95 % de ton rythme jusqu'à enchaîner 5 tours propres : un tour propre et régulier fait plus progresser qu'un tour rapide raté.",
                last.incident_laps, last.lap_count
            ))
        } else if let (Some(sd), Some(best)) = (last.stddev_clean_s, last.best_lap_s) {
            (sd > 0.5).then(|| format!(
                "Priorité : la régularité. Tes tours propres varient de ±{:.1} s. Objectif : 5 tours d'affilée à moins de 0,5 s de ton meilleur tour ({}), avec les mêmes points de freinage à chaque tour.",
                sd, fmt_lap(best)
            ))
        } else {
            None
        };
        let next_goal = beginner_goal.or(match (zones.first(), last.mean_clean_s) {
            (Some(z), Some(mean)) => {
                // Les pertes sont mesurées contre le meilleur tour : corriger ce défaut
                // fait baisser la moyenne, pas le meilleur tour lui-même.
                let target = mean - z.avg_loss_s as f64 * z.occurrences as f64 / z.laps_compared.max(1) as f64;
                Some(format!(
                    "{} : {} sur {}/{} tours propres (≈{:.2} s perdus à chaque fois). Corriger ce défaut ramènerait ta moyenne de {} à ≈{}.",
                    z.segment,
                    z.typical_issue.split(" : ").nth(1).unwrap_or(&z.typical_issue),
                    z.occurrences,
                    z.laps_compared,
                    z.avg_loss_s,
                    fmt_lap(mean),
                    fmt_lap(target)
                ))
            }
            (None, Some(_)) => detail.theoretical_best_s.map(|t| {
                format!("Aucune perte récurrente détectée. Prochaine étape : enchaîner tes meilleurs segments pour viser le théorique {}.", fmt_lap(t))
            }),
            _ => Some("Pas assez de tours propres pour identifier une zone de progrès : roule au moins 3 tours lancés sans incident.".into()),
        });
        Ok(Dashboard {
            theoretical_best_s: detail.theoretical_best_s,
            last_session: Some(last),
            progress_zones: zones,
            consistency_trend: trend,
            next_goal,
            total_sessions: all.len(),
        })
    }

    /// Export complet (métadonnées + traces) au format JSON.
    pub fn export_all(&self, path: &Path) -> Result<usize> {
        #[derive(Serialize)]
        struct ExportLap {
            #[serde(flatten)]
            row: LapRow,
            trace: LapTrace,
        }
        #[derive(Serialize)]
        struct ExportSession {
            meta: SessionMeta,
            laps: Vec<ExportLap>,
        }
        let db = self.db();
        let sessions = db.list_sessions(&SessionFilter::default())?;
        let mut out = Vec::with_capacity(sessions.len());
        for s in &sessions {
            let laps = db
                .laps(s.id)?
                .into_iter()
                .map(|row| Ok(ExportLap { trace: db.trace(row.id)?, row }))
                .collect::<std::result::Result<Vec<_>, StoreError>>()?;
            out.push(ExportSession { meta: db.session_meta(s.id)?, laps });
        }
        let f = std::fs::File::create(path).map_err(|e| ServiceError::User(format!("export : {e}")))?;
        serde_json::to_writer(std::io::BufWriter::new(f), &serde_json::json!({ "format": "apex-companion-export", "version": 1, "sessions": out }))
            .map_err(|e| ServiceError::User(format!("export : {e}")))?;
        Ok(sessions.len())
    }

    /// Diagnostic exportable sans données personnelles (ni chemins, ni noms, ni télémétrie).
    pub fn diagnostics(&self, app_version: &str) -> Result<Diagnostics> {
        Ok(Diagnostics {
            app_version: app_version.into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            sessions: self.list_sessions(&SessionFilter::default())?.len(),
            connectors: self.connectors.iter().map(|c| (c.display_name().to_string(), c.capabilities())).collect(),
            note: "Aucun chemin, nom de pilote ni donnée de télémétrie inclus.".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_flow_end_to_end() {
        let svc = AppService::in_memory().unwrap();
        let InsertOutcome::Inserted(sid) = svc.load_demo().unwrap() else { panic!() };
        let d = svc.session_detail(sid).unwrap();
        assert!(d.best_lap_id.is_some() && d.theoretical_best_s.is_some());
        let other = d.laps.iter().find(|l| l.clean && Some(l.id) != d.best_lap_id).unwrap();
        let c = svc.compare(d.best_lap_id.unwrap(), other.id, 2.0).unwrap();
        assert!(!c.delta_s.is_empty());
        svc.set_reference(d.best_lap_id.unwrap()).unwrap();
        assert_eq!(svc.session_detail(sid).unwrap().personal_reference_lap_id, d.best_lap_id);
        svc.save_settings(AppSettings { beginner_mode: false, ..Default::default() }).unwrap();
        let dash = svc.dashboard().unwrap();
        assert_eq!(dash.progress_zones[0].segment, "V1");
        assert!(dash.next_goal.unwrap().starts_with("V1"));
        // Démo : σ ≈ 0,3 s, une seule sortie → pas d'alerte débutant, on retombe sur la zone de progrès.
        svc.save_settings(AppSettings::default()).unwrap();
        assert!(svc.dashboard().unwrap().next_goal.unwrap().starts_with("V1"));
    }

    #[test]
    fn settings_validation_and_cloud_locked() {
        let svc = AppService::in_memory().unwrap();
        let s = AppSettings { cloud_enabled: true, ..Default::default() };
        assert!(!svc.save_settings(s).unwrap().cloud_enabled);
        let bad = AppSettings { sample_rate_hz: 1000, ..Default::default() };
        assert!(svc.save_settings(bad).is_err());
    }

    #[test]
    fn unsupported_file_is_reported_not_crashing() {
        let svc = AppService::in_memory().unwrap();
        let r = svc.import_paths(&[PathBuf::from("x.csv")]);
        assert!(r[0].error.as_ref().unwrap().contains("non pris en charge"));
    }

    #[test]
    fn folder_sync_imports_once_and_waits_for_finished_files() {
        let svc = AppService::in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        svc.save_settings(AppSettings { iracing_telemetry_dir: Some(dir.path().to_string_lossy().into()), ..Default::default() }).unwrap();
        std::fs::write(dir.path().join("session.ibt"), crate::ingest::ibt::writer::synthetic()).unwrap();
        std::fs::write(dir.path().join("casse.ibt"), b"pas un ibt").unwrap();
        // Fichier tout juste écrit : considéré en cours d'écriture.
        let r = svc.sync_iracing_folder(std::time::Duration::from_secs(3600)).unwrap();
        assert_eq!((r.imported, r.pending), (0, 2));
        let r = svc.sync_iracing_folder(std::time::Duration::ZERO).unwrap();
        assert_eq!(r.imported, 1);
        assert_eq!(r.errors.len(), 1, "{:?}", r.errors);
        // Deuxième passage : rien n'est reparsé, ni le bon fichier ni celui en erreur.
        let r = svc.sync_iracing_folder(std::time::Duration::ZERO).unwrap();
        assert_eq!(r, SyncReport::default());
        assert_eq!(svc.list_sessions(&SessionFilter::default()).unwrap().len(), 1);
    }

    #[test]
    fn export_and_diagnostics() {
        let svc = AppService::in_memory().unwrap();
        svc.load_demo().unwrap();
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(svc.export_all(&dir.path().join("e.json")).unwrap(), 1);
        let d = svc.diagnostics("0.1.0").unwrap();
        let json = serde_json::to_string(&d).unwrap();
        assert!(!json.contains("Circuit Démo"));
    }
}
