//! Persistance SQLite locale.
//!
//! - Mode WAL + `synchronous=NORMAL` : pas de corruption en cas de crash de l'appli.
//! - Chaque import est une transaction unique : tout ou rien.
//! - Anti-doublon : contrainte `UNIQUE(source_fingerprint)`.
//! - Traces de tour en BLOB colonnaire compressé (voir `codec`).

use crate::analysis::session::{best_clean_lap, consistency};
use crate::codec;
use crate::model::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("base de données : {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("trace : {0}")]
    Codec(#[from] codec::CodecError),
    #[error("JSON : {0}")]
    Json(#[from] serde_json::Error),
    #[error("introuvable : {0}")]
    NotFound(String),
    #[error("données invalides : {0}")]
    Invalid(String),
}

const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE sessions (
  id INTEGER PRIMARY KEY,
  sim TEXT NOT NULL,
  track TEXT NOT NULL,
  track_config TEXT,
  track_length_m REAL,
  car TEXT NOT NULL,
  kind TEXT NOT NULL,
  started_at INTEGER NOT NULL,
  weather_json TEXT NOT NULL,
  source_fingerprint TEXT NOT NULL UNIQUE,
  source_path TEXT,
  sample_rate_hz REAL,
  imported_at INTEGER NOT NULL
);
CREATE INDEX idx_sessions_combo ON sessions(sim, track, car, started_at);
CREATE TABLE laps (
  id INTEGER PRIMARY KEY,
  session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  number INTEGER NOT NULL,
  kind TEXT NOT NULL,
  lap_time_s REAL,
  validity TEXT NOT NULL,
  validity_source TEXT NOT NULL,
  flags_json TEXT NOT NULL,
  sectors_json TEXT NOT NULL,
  fuel_start_l REAL,
  fuel_used_l REAL,
  is_reference INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_laps_session ON laps(session_id);
CREATE TABLE lap_traces (
  lap_id INTEGER PRIMARY KEY REFERENCES laps(id) ON DELETE CASCADE,
  codec INTEGER NOT NULL,
  n_samples INTEGER NOT NULL,
  data BLOB NOT NULL
);
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
"#, r#"
-- v2 : fichiers déjà traités par la synchro automatique (évite de reparser à chaque passage).
CREATE TABLE imported_files (
  path TEXT PRIMARY KEY,
  size INTEGER NOT NULL,
  mtime INTEGER NOT NULL,
  session_id INTEGER,
  error TEXT,
  checked_at INTEGER NOT NULL
);
"#, r#"
-- v3 : noms de virages par circuit (combo simulateur / circuit / configuration).
CREATE TABLE corner_names (
  track_key TEXT NOT NULL,
  apex_m REAL NOT NULL,
  name TEXT NOT NULL,
  PRIMARY KEY (track_key, apex_m)
);
"#, r#"
-- v4 : objectif de session (un seul actif à la fois).
CREATE TABLE goals (
  id INTEGER PRIMARY KEY,
  track_key TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  title TEXT NOT NULL,
  metric_json TEXT NOT NULL,
  archived INTEGER NOT NULL DEFAULT 0
);
"#, r#"
-- v5 : version du décodeur ayant produit les tours (permet de les recalculer après une correction).
ALTER TABLE sessions ADD COLUMN parser_version INTEGER NOT NULL DEFAULT 1;
"#, r#"
-- v6 : tours de référence d'autres pilotes (Garage 61…), tenus à l'écart des statistiques du pilote.
ALTER TABLE sessions ADD COLUMN external_driver TEXT;
"#];

/// Version courante du décodage des fichiers sources. À incrémenter quand une correction
/// change les tours produits : les sessions plus anciennes sont recalculées depuis leur fichier.
/// v2 : franchissement de ligne robuste, noms Windows-1252, origine GPS commune aux tours.
/// v3 : un « reset » près de la ligne ne crée plus de faux tour ; positions GPS nulles ignorées ;
///      surface sous la voiture conservée (localisation des sorties de piste).
pub const PARSER_VERSION: i64 = 3;

/// (id, clé circuit, date de création, titre, métrique JSON).
pub type GoalRow = (i64, String, i64, String, String);

pub struct Store {
    conn: Connection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "status", content = "session_id", rename_all = "snake_case")]
pub enum InsertOutcome {
    Inserted(i64),
    Duplicate(i64),
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionSummary {
    pub id: i64,
    pub sim: SimId,
    pub sim_name: String,
    pub track: String,
    pub track_config: Option<String>,
    pub car: String,
    pub kind: SessionKind,
    pub started_at_unix: i64,
    pub weather: Weather,
    pub lap_count: usize,
    pub clean_lap_count: usize,
    pub best_lap_s: Option<f64>,
    pub mean_clean_s: Option<f64>,
    pub stddev_clean_s: Option<f64>,
    pub incident_laps: usize,
    pub fuel_per_lap_l: Option<f32>,
    /// Pilote d'un tour de référence externe ; `None` pour les sessions du pilote.
    pub external_driver: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LapRow {
    pub id: i64,
    pub session_id: i64,
    pub is_reference: bool,
    pub clean: bool,
    #[serde(flatten)]
    pub meta: LapMeta,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct SessionFilter {
    pub sim: Option<SimId>,
    pub track: Option<String>,
    pub car: Option<String>,
    pub kind: Option<SessionKind>,
    pub from_unix: Option<i64>,
    pub to_unix: Option<i64>,
    /// `Some(true)` : uniquement les références externes. Par défaut : uniquement les sessions du pilote.
    pub external: Option<bool>,
}

fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, StoreError> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let mut s = Store { conn };
        s.migrate()?;
        Ok(s)
    }

    fn migrate(&mut self) -> Result<(), StoreError> {
        let v: i64 = self.conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(v as usize) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn insert_session(&mut self, s: &ImportedSession) -> Result<InsertOutcome, StoreError> {
        if let Some(id) = self
            .conn
            .query_row("SELECT id FROM sessions WHERE source_fingerprint = ?1", [&s.meta.source_fingerprint], |r| r.get(0))
            .optional()?
        {
            return Ok(InsertOutcome::Duplicate(id));
        }
        for l in &s.laps {
            l.trace.validate().map_err(|e| StoreError::Invalid(format!("tour {} : {e}", l.meta.number)))?;
        }
        let tx = self.conn.transaction()?;
        let m = &s.meta;
        tx.execute(
            "INSERT INTO sessions (sim, track, track_config, track_length_m, car, kind, started_at, weather_json, source_fingerprint, source_path, sample_rate_hz, imported_at, parser_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                m.sim.as_str(), m.track, m.track_config, m.track_length_m, m.car, m.kind.as_str(), m.started_at_unix,
                serde_json::to_string(&m.weather)?, m.source_fingerprint, m.source_path, s.sample_rate_hz, now_unix(), PARSER_VERSION
            ],
        )?;
        let sid = tx.last_insert_rowid();
        Self::insert_laps(&tx, sid, s)?;
        tx.commit()?;
        Ok(InsertOutcome::Inserted(sid))
    }

    fn insert_laps(tx: &rusqlite::Transaction, sid: i64, s: &ImportedSession) -> Result<(), StoreError> {
        for l in &s.laps {
            let lm = &l.meta;
            tx.execute(
                "INSERT INTO laps (session_id, number, kind, lap_time_s, validity, validity_source, flags_json, sectors_json, fuel_start_l, fuel_used_l)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    sid, lm.number, lm.kind.as_str(), lm.lap_time_s, lm.validity.as_str(),
                    serde_json::to_string(&lm.validity_source)?, serde_json::to_string(&lm.flags)?,
                    serde_json::to_string(&lm.sector_times_s)?, lm.fuel_start_l, lm.fuel_used_l
                ],
            )?;
            let lid = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO lap_traces (lap_id, codec, n_samples, data) VALUES (?1, ?2, ?3, ?4)",
                params![lid, codec::CODEC_ID, l.trace.len() as i64, codec::encode(&l.trace)],
            )?;
        }
        Ok(())
    }

    /// Marque une session comme tour de référence d'un autre pilote.
    pub fn set_external_driver(&self, id: i64, driver: &str) -> Result<(), StoreError> {
        self.conn.execute("UPDATE sessions SET external_driver = ?1 WHERE id = ?2", params![driver, id])?;
        Ok(())
    }

    /// Pilote de la session si c'est une référence externe.
    pub fn external_driver(&self, id: i64) -> Result<Option<String>, StoreError> {
        Ok(self.conn.query_row("SELECT external_driver FROM sessions WHERE id = ?1", [id], |r| r.get(0)).optional()?.flatten())
    }

    /// Sessions iRacing décodées par une version antérieure du décodeur : (id, fichier source).
    pub fn outdated_sessions(&self) -> Result<Vec<(i64, String)>, StoreError> {
        let mut st = self.conn.prepare(
            "SELECT id, source_path FROM sessions WHERE parser_version < ?1 AND sim = 'iracing' AND source_path IS NOT NULL AND external_driver IS NULL ORDER BY id",
        )?;
        let rows = st.query_map([PARSER_VERSION], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Marque une session comme à jour sans la recalculer (fichier source disparu).
    pub fn mark_parsed(&self, id: i64) -> Result<(), StoreError> {
        self.conn.execute("UPDATE sessions SET parser_version = ?1 WHERE id = ?2", params![PARSER_VERSION, id])?;
        Ok(())
    }

    /// Remplace les tours d'une session par un nouveau décodage du même fichier.
    /// L'identifiant de session est conservé ; la référence personnelle est reportée par numéro de tour.
    pub fn replace_laps(&mut self, id: i64, s: &ImportedSession) -> Result<(), StoreError> {
        for l in &s.laps {
            l.trace.validate().map_err(|e| StoreError::Invalid(format!("tour {} : {e}", l.meta.number)))?;
        }
        let refs: Vec<u32> = {
            let mut st = self.conn.prepare("SELECT number FROM laps WHERE session_id = ?1 AND is_reference = 1")?;
            let v = st.query_map([id], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?;
            v
        };
        let tx = self.conn.transaction()?;
        let m = &s.meta;
        tx.execute(
            "UPDATE sessions SET track = ?1, track_config = ?2, car = ?3, track_length_m = ?4, parser_version = ?5 WHERE id = ?6",
            params![m.track, m.track_config, m.car, m.track_length_m, PARSER_VERSION, id],
        )?;
        tx.execute("DELETE FROM laps WHERE session_id = ?1", [id])?;
        Self::insert_laps(&tx, id, s)?;
        for n in refs {
            tx.execute("UPDATE laps SET is_reference = 1 WHERE session_id = ?1 AND number = ?2 AND kind = 'flying' AND lap_time_s IS NOT NULL", params![id, n])?;
        }
        tx.commit()?;
        Ok(())
    }

    fn row_to_meta(r: &rusqlite::Row) -> rusqlite::Result<SessionMeta> {
        let weather: String = r.get("weather_json")?;
        let sim: String = r.get("sim")?;
        let kind: String = r.get("kind")?;
        Ok(SessionMeta {
            sim: SimId::parse(&sim).unwrap_or(SimId::Demo),
            track: r.get("track")?,
            track_config: r.get("track_config")?,
            track_length_m: r.get("track_length_m")?,
            car: r.get("car")?,
            kind: SessionKind::parse(&kind),
            started_at_unix: r.get("started_at")?,
            weather: serde_json::from_str(&weather).unwrap_or_default(),
            source_fingerprint: r.get("source_fingerprint")?,
            source_path: r.get("source_path")?,
        })
    }

    pub fn session_meta(&self, id: i64) -> Result<SessionMeta, StoreError> {
        self.conn
            .query_row("SELECT * FROM sessions WHERE id = ?1", [id], Self::row_to_meta)
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("session {id}")))
    }

    fn row_to_lap(r: &rusqlite::Row) -> rusqlite::Result<LapRow> {
        let flags: String = r.get("flags_json")?;
        let sectors: String = r.get("sectors_json")?;
        let vs: String = r.get("validity_source")?;
        let kind: String = r.get("kind")?;
        let validity: String = r.get("validity")?;
        let meta = LapMeta {
            number: r.get("number")?,
            kind: LapKind::parse(&kind),
            lap_time_s: r.get("lap_time_s")?,
            sector_times_s: serde_json::from_str(&sectors).unwrap_or_default(),
            flags: serde_json::from_str(&flags).unwrap_or_default(),
            validity: Validity::parse(&validity),
            validity_source: serde_json::from_str(&vs).unwrap_or(ValiditySource::Derived),
            fuel_start_l: r.get("fuel_start_l")?,
            fuel_used_l: r.get("fuel_used_l")?,
        };
        Ok(LapRow {
            id: r.get("id")?,
            session_id: r.get("session_id")?,
            is_reference: r.get::<_, i64>("is_reference")? != 0,
            clean: meta.is_clean(),
            meta,
        })
    }

    pub fn laps(&self, session_id: i64) -> Result<Vec<LapRow>, StoreError> {
        let mut st = self.conn.prepare("SELECT * FROM laps WHERE session_id = ?1 ORDER BY id")?;
        let rows = st.query_map([session_id], Self::row_to_lap)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn lap(&self, lap_id: i64) -> Result<LapRow, StoreError> {
        self.conn
            .query_row("SELECT * FROM laps WHERE id = ?1", [lap_id], Self::row_to_lap)
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("tour {lap_id}")))
    }

    pub fn trace(&self, lap_id: i64) -> Result<LapTrace, StoreError> {
        let blob: Vec<u8> = self
            .conn
            .query_row("SELECT data FROM lap_traces WHERE lap_id = ?1", [lap_id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("trace du tour {lap_id}")))?;
        Ok(codec::decode(&blob)?)
    }

    pub fn list_sessions(&self, f: &SessionFilter) -> Result<Vec<SessionSummary>, StoreError> {
        let mut sql = String::from(if f.external == Some(true) { "SELECT * FROM sessions WHERE external_driver IS NOT NULL" } else { "SELECT * FROM sessions WHERE external_driver IS NULL" });
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![];
        if let Some(s) = f.sim {
            sql += " AND sim = ?";
            args.push(Box::new(s.as_str()));
        }
        if let Some(t) = &f.track {
            sql += " AND track = ?";
            args.push(Box::new(t.clone()));
        }
        if let Some(c) = &f.car {
            sql += " AND car = ?";
            args.push(Box::new(c.clone()));
        }
        if let Some(k) = f.kind {
            sql += " AND kind = ?";
            args.push(Box::new(k.as_str()));
        }
        if let Some(a) = f.from_unix {
            sql += " AND started_at >= ?";
            args.push(Box::new(a));
        }
        if let Some(b) = f.to_unix {
            sql += " AND started_at <= ?";
            args.push(Box::new(b));
        }
        sql += " ORDER BY started_at DESC";
        let mut st = self.conn.prepare(&sql)?;
        let metas: Vec<(i64, SessionMeta, Option<String>)> = st
            .query_map(rusqlite::params_from_iter(args.iter().map(|b| b.as_ref())), |r| Ok((r.get("id")?, Self::row_to_meta(r)?, r.get("external_driver")?)))?
            .collect::<Result<_, _>>()?;
        metas
            .into_iter()
            .map(|(id, m, external_driver)| {
                let laps = self.laps(id)?;
                let metas: Vec<LapMeta> = laps.iter().map(|l| l.meta.clone()).collect();
                let best = best_clean_lap(&metas).and_then(|i| metas[i].lap_time_s);
                let cons = consistency(&metas);
                let fuel: Vec<f32> = metas.iter().filter(|l| l.is_clean()).filter_map(|l| l.fuel_used_l).collect();
                Ok(SessionSummary {
                    id,
                    sim: m.sim,
                    sim_name: m.sim.display_name().into(),
                    track: m.track,
                    track_config: m.track_config,
                    car: m.car,
                    kind: m.kind,
                    started_at_unix: m.started_at_unix,
                    weather: m.weather,
                    lap_count: metas.len(),
                    clean_lap_count: metas.iter().filter(|l| l.is_clean()).count(),
                    best_lap_s: best,
                    mean_clean_s: cons.as_ref().map(|c| c.mean_s),
                    stddev_clean_s: cons.as_ref().map(|c| c.stddev_s),
                    incident_laps: metas.iter().filter(|l| l.flags.incident || l.flags.off_track).count(),
                    fuel_per_lap_l: (!fuel.is_empty()).then(|| fuel.iter().sum::<f32>() / fuel.len() as f32),
                    external_driver,
                })
            })
            .collect()
    }

    /// Une seule référence personnelle par (simulateur, circuit, config, voiture).
    pub fn set_reference(&mut self, lap_id: i64) -> Result<(), StoreError> {
        let lap = self.lap(lap_id)?;
        let m = self.session_meta(lap.session_id)?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE laps SET is_reference = 0 WHERE session_id IN (
               SELECT id FROM sessions WHERE sim = ?1 AND track = ?2 AND IFNULL(track_config,'') = IFNULL(?3,'') AND car = ?4)",
            params![m.sim.as_str(), m.track, m.track_config, m.car],
        )?;
        tx.execute("UPDATE laps SET is_reference = 1 WHERE id = ?1", [lap_id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_session(&mut self, id: i64) -> Result<(), StoreError> {
        self.conn.execute("DELETE FROM sessions WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Suppression totale des données (droit à l'effacement).
    pub fn delete_all(&mut self) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute_batch("DELETE FROM lap_traces; DELETE FROM laps; DELETE FROM sessions; DELETE FROM imported_files; DELETE FROM corner_names; DELETE FROM goals;")?;
        tx.commit()?;
        self.conn.execute_batch("VACUUM;")?;
        Ok(())
    }

    /// Taille et date du fichier lors de son dernier traitement par la synchro.
    pub fn file_state(&self, path: &str) -> Result<Option<(i64, i64)>, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT size, mtime FROM imported_files WHERE path = ?1", [path], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?)
    }

    pub fn record_file(&self, path: &str, size: i64, mtime: i64, session_id: Option<i64>, error: Option<&str>) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO imported_files (path, size, mtime, session_id, error, checked_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(path) DO UPDATE SET size = excluded.size, mtime = excluded.mtime, session_id = excluded.session_id,
               error = excluded.error, checked_at = excluded.checked_at",
            params![path, size, mtime, session_id, error, now_unix()],
        )?;
        Ok(())
    }

    pub fn corner_names(&self, track_key: &str) -> Result<Vec<(f32, String)>, StoreError> {
        let mut st = self.conn.prepare("SELECT apex_m, name FROM corner_names WHERE track_key = ?1 ORDER BY apex_m")?;
        let v = st.query_map([track_key], |r| Ok((r.get::<_, f64>(0)? as f32, r.get(1)?)))?.collect::<Result<_, _>>()?;
        Ok(v)
    }

    /// Nomme le virage dont l'apex est à `apex_m` (remplace un nom existant à moins de 60 m ; nom vide = suppression).
    pub fn set_corner_name(&mut self, track_key: &str, apex_m: f32, name: &str) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM corner_names WHERE track_key = ?1 AND ABS(apex_m - ?2) <= 60.0",
            params![track_key, apex_m as f64],
        )?;
        let name = name.trim();
        if !name.is_empty() {
            tx.execute("INSERT INTO corner_names (track_key, apex_m, name) VALUES (?1, ?2, ?3)", params![track_key, apex_m as f64, name])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Crée l'objectif actif (archive le précédent : un seul point travaillé à la fois).
    pub fn create_goal(&mut self, track_key: &str, title: &str, metric_json: &str) -> Result<i64, StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute("UPDATE goals SET archived = 1 WHERE archived = 0", [])?;
        tx.execute(
            "INSERT INTO goals (track_key, created_at, title, metric_json) VALUES (?1, ?2, ?3, ?4)",
            params![track_key, now_unix(), title, metric_json],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    /// (id, clé circuit, date de création, titre, métrique JSON) de l'objectif actif.
    pub fn active_goal(&self) -> Result<Option<GoalRow>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, track_key, created_at, title, metric_json FROM goals WHERE archived = 0 ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?)
    }

    pub fn archive_goal(&self, id: i64) -> Result<(), StoreError> {
        self.conn.execute("UPDATE goals SET archived = 1 WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, StoreError> {
        Ok(self.conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::demo_session;

    #[test]
    fn outdated_session_is_replaced_in_place() {
        let mut st = Store::open_in_memory().unwrap();
        let mut s = demo_session(7, 1_760_000_000);
        s.meta.sim = SimId::Iracing;
        s.meta.source_path = Some("C:/x.ibt".into());
        let InsertOutcome::Inserted(id) = st.insert_session(&s).unwrap() else { panic!() };
        assert!(st.outdated_sessions().unwrap().is_empty());
        st.conn.execute("UPDATE sessions SET parser_version = 1", []).unwrap();
        let laps = st.laps(id).unwrap();
        st.set_reference(laps[2].id).unwrap();
        assert_eq!(st.outdated_sessions().unwrap(), vec![(id, "C:/x.ibt".to_string())]);
        let mut s2 = s.clone();
        s2.meta.track = "Autódromo".into();
        s2.laps.truncate(3);
        st.replace_laps(id, &s2).unwrap();
        assert!(st.outdated_sessions().unwrap().is_empty());
        let laps = st.laps(id).unwrap();
        assert_eq!(laps.len(), 3);
        assert!(laps[2].is_reference, "référence reportée par numéro de tour");
        assert_eq!(st.session_meta(id).unwrap().track, "Autódromo");
    }

    #[test]
    fn insert_is_idempotent() {
        let mut st = Store::open_in_memory().unwrap();
        let s = demo_session(42, 1_760_000_000);
        let a = st.insert_session(&s).unwrap();
        let b = st.insert_session(&s).unwrap();
        let InsertOutcome::Inserted(id) = a else { panic!() };
        assert_eq!(b, InsertOutcome::Duplicate(id));
        assert_eq!(st.list_sessions(&SessionFilter::default()).unwrap().len(), 1);
    }

    #[test]
    fn roundtrip_laps_and_traces() {
        let mut st = Store::open_in_memory().unwrap();
        let s = demo_session(7, 1_760_000_000);
        let InsertOutcome::Inserted(id) = st.insert_session(&s).unwrap() else { panic!() };
        let laps = st.laps(id).unwrap();
        assert_eq!(laps.len(), s.laps.len());
        let tr = st.trace(laps[2].id).unwrap();
        assert_eq!(tr, s.laps[2].trace);
        let sum = &st.list_sessions(&SessionFilter::default()).unwrap()[0];
        assert!(sum.best_lap_s.is_some());
        assert_eq!(sum.incident_laps, 1);
        assert_eq!(sum.clean_lap_count, 4);
    }

    #[test]
    fn reference_is_unique_per_combo() {
        let mut st = Store::open_in_memory().unwrap();
        let InsertOutcome::Inserted(id) = st.insert_session(&demo_session(1, 1)).unwrap() else { panic!() };
        let laps = st.laps(id).unwrap();
        st.set_reference(laps[1].id).unwrap();
        st.set_reference(laps[2].id).unwrap();
        let refs: Vec<_> = st.laps(id).unwrap().into_iter().filter(|l| l.is_reference).collect();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].id, laps[2].id);
    }

    #[test]
    fn filters_and_delete_all() {
        let mut st = Store::open_in_memory().unwrap();
        st.insert_session(&demo_session(1, 100)).unwrap();
        st.insert_session(&demo_session(2, 200)).unwrap();
        let f = SessionFilter { from_unix: Some(150), ..Default::default() };
        assert_eq!(st.list_sessions(&f).unwrap().len(), 1);
        let f = SessionFilter { sim: Some(SimId::Iracing), ..Default::default() };
        assert_eq!(st.list_sessions(&f).unwrap().len(), 0);
        st.delete_all().unwrap();
        assert!(st.list_sessions(&SessionFilter::default()).unwrap().is_empty());
    }
}
