//! Connecteur Le Mans Ultimate.
//!
//! Source : mémoire partagée native `LMU_Data` (voir `lmu_shm`), publiée par le jeu quand
//! *Réglages → Gameplay → Enable Plugins* est activé (redémarrage du jeu requis).
//!
//! LMU n'écrit pas de fichier de télémétrie exploitable : pendant que le jeu tourne, Apex lit la
//! mémoire partagée dans son propre fil (~50 Hz), alimente le coach en direct et enregistre la séance
//! dans un fichier `.apexlmu` (dossier de données d'Apex), importé ensuite comme un `.ibt` iRacing.

use super::lmu_shm::Frame;
use super::*;
use crate::ingest::lmu::{self as rec, LiveState, Recorder, Recording};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct LmuConnector {
    /// Dossier des enregistrements (dossier de données d'Apex).
    pub recordings_dir: Option<PathBuf>,
}

pub struct LmuImporter {
    dir: Option<PathBuf>,
}

impl SessionImporter for LmuImporter {
    fn extensions(&self) -> &'static [&'static str] {
        &[rec::EXT]
    }
    fn default_dir(&self) -> Option<PathBuf> {
        self.dir.clone()
    }
    fn import_file(&self, path: &Path) -> Result<ImportedSession, ConnectorError> {
        rec::import_lmu_file(path).map_err(ConnectorError::Import)
    }
}

impl SimulatorConnector for LmuConnector {
    fn id(&self) -> SimId {
        SimId::Lmu
    }
    fn process_names(&self) -> &'static [&'static str] {
        &["Le Mans Ultimate.exe", "LeMansUltimate.exe"]
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            live_telemetry: Support::Unverified,
            post_session_import: Support::Unverified,
            position_xy: Support::Unverified,
            tyres: Support::Planned,
            damage: Support::Planned,
            weather: Support::Unverified,
            replay: Support::Unavailable,
            data_source: "Mémoire partagée du jeu (LMU_Data), enregistrée par Apex pendant que tu roules",
            notes: vec![
                "Dans LMU : Réglages → Gameplay → « Enable Plugins », puis redémarre le jeu.",
                "Apex doit être ouvert pendant que tu roules : il enregistre lui-même la séance (LMU n'écrit pas de fichier lisible).",
                "La séance est importée à sa fin : changement de séance, retour au menu ou fermeture du jeu.",
            ],
        }
    }
    fn detect_installation(&self, cfg: &ConnectorConfig) -> InstallStatus {
        install_from_path(cfg, "Le Mans Ultimate.exe")
    }
    fn importer(&self) -> Option<&dyn SessionImporter> {
        static NONE: LmuImporter = LmuImporter { dir: None };
        // Le dossier est fourni par le service (dossier de données) : l'import d'un fichier n'en a pas besoin.
        Some(&NONE)
    }
    #[cfg(windows)]
    fn open_live(&self, cfg: &ConnectorConfig) -> Result<Box<dyn LiveSource>, ConnectorError> {
        let dir = cfg.telemetry_dir.clone().or_else(|| self.recordings_dir.clone()).ok_or(ConnectorError::NotSupported("dossier d'enregistrement LMU"))?;
        let mut map = super::lmu_live::Mapping::open()?;
        let hz = if cfg.sample_rate_hz >= 20 { cfg.sample_rate_hz.min(60) as f32 } else { 50.0 };
        Ok(Box::new(ThreadedLive::spawn(move || map.read(), dir, hz)?))
    }
}

// --- Fichiers d'enregistrement --------------------------------------------------------------------

fn safe(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' }).collect();
    t.trim_matches('_').chars().take(40).collect()
}

/// Nom de fichier d'une séance : `lmu_<circuit>_<date>_<séance>`.
pub fn file_stem(r: &Recording) -> String {
    let when = chrono::DateTime::from_timestamp(r.header.started_at_unix, 0)
        .map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d_%H-%M-%S").to_string())
        .unwrap_or_else(|| r.header.started_at_unix.to_string());
    format!("lmu_{}_{}_{}{}", safe(&r.header.track), when, rec::session_label(r.header.session).to_lowercase(), r.header.session)
}

/// Écrit la séance : `.apexlmu-part` pendant qu'elle se déroule, `.apexlmu` à la fin (écriture
/// atomique par renommage ; le fichier provisoire est alors supprimé).
pub fn write_recording(dir: &Path, r: &Recording, done: bool) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let stem = file_stem(r);
    let part = dir.join(format!("{stem}.{}", rec::PART_EXT));
    let bytes = rec::encode(r);
    if !done {
        let tmp = dir.join(format!("{stem}.tmp"));
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &part)?;
        return Ok(part);
    }
    let final_path = dir.join(format!("{stem}.{}", rec::EXT));
    let tmp = dir.join(format!("{stem}.tmp"));
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &final_path)?;
    let _ = std::fs::remove_file(part);
    Ok(final_path)
}

/// Enregistrements provisoires abandonnés (Apex fermé brutalement) : promus en séances terminées
/// quand ils n'ont plus bougé depuis `min_age`.
pub fn promote_stale_parts(dir: &Path, min_age: Duration) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    let mut n = 0;
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some(rec::PART_EXT) {
            continue;
        }
        let old = e.metadata().ok().and_then(|m| m.modified().ok()).and_then(|m| m.elapsed().ok()).is_some_and(|a| a >= min_age);
        if old && std::fs::rename(&p, p.with_extension(rec::EXT)).is_ok() {
            n += 1;
        }
    }
    n
}

// --- Pilotage : une image → coach + enregistrement ------------------------------------------------

/// Transforme les images de la mémoire partagée en échantillons pour le coach et en fichiers de séance.
pub struct LmuDriver {
    dir: PathBuf,
    recorder: Recorder,
    live: LiveState,
    yaml_key: Option<String>,
    last_part: Instant,
    tick: i64,
    /// Depuis quand le pilote n'est plus dans la voiture (garage, menus).
    out_since: Option<Instant>,
    /// Hors de la voiture plus longtemps que ça : la séance est écrite (comme iRacing, un fichier
    /// par passage en piste) et importée sans attendre la fin de la séance.
    pub idle_after: Duration,
    /// Fichiers terminés écrits par ce pilote (pour les tests et le journal).
    pub written: Vec<PathBuf>,
}

impl LmuDriver {
    pub fn new(dir: PathBuf, hz: f32) -> Self {
        Self { dir, recorder: Recorder::new(hz), live: LiveState::default(), yaml_key: None, last_part: Instant::now(), tick: 0, out_since: None, idle_after: Duration::from_secs(30), written: vec![] }
    }

    fn save(&mut self, r: &Recording, done: bool) {
        match write_recording(&self.dir, r, done) {
            Ok(p) if done => {
                log::info!("séance LMU enregistrée : {}", p.display());
                self.written.push(p);
            }
            Ok(_) => {}
            Err(e) => log::warn!("enregistrement LMU : {e}"),
        }
    }

    pub fn step(&mut self, f: &Frame, now_unix: i64) -> LiveSnapshot {
        if let Some(done) = self.recorder.push(f, now_unix) {
            self.save(&done, true);
            self.live = LiveState::default();
        }
        let in_car = f.info.mInRealtime != 0 && f.telem.is_some();
        if in_car {
            self.out_since = None;
        } else if self.recorder.current().is_some() {
            let since = *self.out_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= self.idle_after {
                self.out_since = None;
                self.yaml_key = None;
                if let Some(r) = self.recorder.finish() {
                    self.save(&r, true);
                }
            }
        }
        // Sauvegarde provisoire toutes les 2 minutes : rien de perdu si Apex est fermé brutalement.
        if self.last_part.elapsed() >= Duration::from_secs(120) {
            self.last_part = Instant::now();
            if let Some(r) = self.recorder.current().filter(|r| r.ticks.len() > 500).cloned() {
                self.save(&r, false);
            }
        }
        self.tick += 1;
        let row = self.live.row(f);
        let session_info = row.and_then(|_| {
            let y = rec::live_yaml(f)?;
            let key = format!("{}|{}|{}", rec::track_name(f), f.info.mSession, f.telem.as_ref().map(rec::car_name).unwrap_or_default());
            (self.yaml_key.as_deref() != Some(key.as_str())).then(|| {
                self.yaml_key = Some(key);
                y
            })
        });
        let t = f.telem.as_ref();
        LiveSnapshot {
            connected: true,
            on_track: row.is_some(),
            session_time_s: t.map(|t| t.mElapsedTime).unwrap_or(0.0),
            lap: t.map(|t| t.mLapNumber).unwrap_or(0),
            lap_dist_pct: row.map(|r| r[2] as f32).unwrap_or(0.0),
            speed_kmh: t.map(|t| rec::speed_mps(t) * 3.6).unwrap_or(0.0),
            gear: t.map(|t| t.mGear).unwrap_or(0),
            throttle: t.map(|t| t.mUnfilteredThrottle as f32).unwrap_or(0.0),
            brake: t.map(|t| t.mUnfilteredBrake as f32).unwrap_or(0.0),
            tick: self.tick,
            // Apex enregistre lui-même : pas de réglage d'enregistrement à surveiller côté jeu.
            disk_logging: None,
            channels: row,
            session_info,
        }
    }

    /// Fin de lecture (jeu fermé, Apex quitté) : la séance en cours est écrite.
    pub fn finish(&mut self) {
        if let Some(r) = self.recorder.finish() {
            self.save(&r, true);
        }
    }
}

// --- Fil de lecture -------------------------------------------------------------------------------

/// Lecture dans un fil dédié à cadence fixe : l'enregistrement ne dépend pas de la boucle de
/// surveillance (imports, synchro) ; le coach reçoit chaque échantillon par une file.
pub struct ThreadedLive {
    rx: Receiver<LiveSnapshot>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl ThreadedLive {
    /// `read` : image courante de la mémoire partagée (`None` = lecture incohérente ou impossible).
    pub fn spawn(mut read: impl FnMut() -> Option<Frame> + Send + 'static, dir: PathBuf, hz: f32) -> Result<Self, ConnectorError> {
        let (tx, rx): (SyncSender<LiveSnapshot>, Receiver<LiveSnapshot>) = std::sync::mpsc::sync_channel(2048);
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let period = Duration::from_secs_f32(1.0 / hz.clamp(10.0, 100.0));
        let handle = std::thread::Builder::new()
            .name("apex-lmu".into())
            .spawn(move || {
                let mut driver = LmuDriver::new(dir, hz);
                while !stop2.load(Ordering::Relaxed) {
                    let t0 = Instant::now();
                    if let Some(f) = read() {
                        let snap = driver.step(&f, chrono::Utc::now().timestamp());
                        // File pleine (coach arrêté) : l'échantillon est perdu pour le coach, pas pour l'enregistrement.
                        let _ = tx.try_send(snap);
                    }
                    if let Some(rest) = period.checked_sub(t0.elapsed()) {
                        std::thread::sleep(rest);
                    }
                }
                driver.finish();
            })
            .map_err(ConnectorError::Io)?;
        Ok(Self { rx, stop, handle: Some(handle) })
    }
}

impl LiveSource for ThreadedLive {
    fn poll(&mut self, timeout_ms: u32) -> Result<Option<LiveSnapshot>, ConnectorError> {
        match self.rx.recv_timeout(Duration::from_millis(timeout_ms as u64)) {
            Ok(s) => Ok(Some(s)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(ConnectorError::Import("lecture LMU interrompue".into())),
        }
    }
}

impl Drop for ThreadedLive {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::lmu_shm::{zeroed, ScoringInfo, VehicleScoring, VehicleTelemetry};

    fn frame(et: f64, session: i32) -> Frame {
        let mut info: ScoringInfo = zeroed();
        info.mTrackName[..6].copy_from_slice(b"Fuji 1");
        info.mLapDist = 4563.0;
        info.mSession = session;
        info.mInRealtime = 1;
        info.mCurrentET = (et * 5.0).floor() / 5.0;
        let mut s: VehicleScoring = zeroed();
        s.mIsPlayer = 1;
        s.mLapDist = (et * 60.0) % 4563.0;
        let mut t: VehicleTelemetry = zeroed();
        t.mElapsedTime = et;
        t.mLocalVel.z = 60.0;
        t.mVehicleModel[..9].copy_from_slice(b"Ferrari 4");
        Frame { game_version: 14000, info, scoring: Some(s), telem: Some(t) }
    }

    #[test]
    fn driver_writes_one_file_per_session_and_feeds_the_coach() {
        let dir = tempfile::tempdir().unwrap();
        let mut d = LmuDriver::new(dir.path().to_path_buf(), 50.0);
        let mut yaml = 0;
        let mut et = 0.0f64;
        while et < 40.0 {
            let s = d.step(&frame(et, 1), 1_790_000_000);
            assert!(s.channels.is_some() && s.on_track);
            yaml += s.session_info.is_some() as u32;
            et += 0.02;
        }
        assert_eq!(yaml, 1, "YAML envoyé une fois par séance");
        // Nouvelle séance : la précédente est écrite.
        let s = d.step(&frame(0.5, 5), 1_790_000_100);
        assert!(s.session_info.is_some());
        assert_eq!(d.written.len(), 1);
        let p = &d.written[0];
        assert_eq!(p.extension().unwrap(), rec::EXT);
        assert!(p.file_name().unwrap().to_string_lossy().starts_with("lmu_Fuji_1_"));
        let imported = rec::import_lmu_file(p).unwrap();
        assert_eq!(imported.meta.car, "Ferrari 4");
        // Séance de qualification trop courte (< 30 s en mouvement) : rien d'écrit à la fin.
        d.finish();
        assert_eq!(d.written.len(), 1);
    }

    #[test]
    fn leaving_the_car_closes_the_run() {
        let dir = tempfile::tempdir().unwrap();
        let mut d = LmuDriver::new(dir.path().to_path_buf(), 50.0);
        d.idle_after = Duration::ZERO;
        let mut et = 0.0f64;
        while et < 40.0 {
            d.step(&frame(et, 1), 0);
            et += 0.02;
        }
        let mut garage = frame(et, 1);
        garage.info.mInRealtime = 0;
        d.step(&garage, 0);
        assert_eq!(d.written.len(), 1);
        // Retour en piste dans la même séance : nouveau passage, nouveau YAML pour le coach.
        assert!(d.step(&frame(et + 60.0, 1), 0).session_info.is_some());
    }

    #[test]
    fn stale_part_files_are_promoted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("x.apexlmu-part"), b"x").unwrap();
        assert_eq!(promote_stale_parts(dir.path(), Duration::from_secs(3600)), 0);
        assert_eq!(promote_stale_parts(dir.path(), Duration::ZERO), 1);
        assert!(dir.path().join("x.apexlmu").exists());
    }

    #[test]
    fn threaded_source_delivers_samples_and_finishes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut et = 0.0f64;
        let mut src = ThreadedLive::spawn(
            move || {
                et += 0.5; // temps de jeu accéléré : 25 s de séance par seconde réelle
                Some(frame(et, 1))
            },
            dir.path().to_path_buf(),
            50.0,
        )
        .unwrap();
        let mut got = 0;
        let until = Instant::now() + Duration::from_millis(2500);
        while Instant::now() < until {
            if src.poll(50).unwrap().is_some() {
                got += 1;
            }
        }
        assert!(got > 50, "{got}");
        drop(src);
        let files: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == rec::EXT)).collect();
        assert_eq!(files.len(), 1, "{files:?}");
    }
}
