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
        // Séance déjà terminée entre-temps : ne pas recréer de provisoire (il serait importé en double).
        if dir.join(format!("{stem}.{}", rec::EXT)).exists() {
            return Ok(part);
        }
        let tmp = dir.join(format!("{stem}.part.tmp"));
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
    /// Écritures sérialisées (provisoire en arrière-plan, définitive à la fin de séance).
    write_lock: Arc<std::sync::Mutex<()>>,
    part_busy: Arc<AtomicBool>,
    /// Sonde setup (lecture seule) : rapport par séance dans ce dossier ; `None` = désactivée.
    pub probe_dir: Option<PathBuf>,
    /// Adresse de l'API du jeu interrogée par la sonde (vide = API non lue).
    pub probe_api: String,
    probe: Option<Probe>,
    /// Séance du jeu dont les estimations ont déjà été demandées (circuit, séance, voiture).
    facts_key: Option<(String, i32, String)>,
}

type ApiResult = (Vec<super::lmu_probe::ApiRead>, super::lmu_probe::GarageSummary, Option<crate::ingest::lmu::SetupSnapshot>);

/// Sonde de la séance en cours : statistiques de la mémoire partagée et lecture de l'API du jeu.
struct Probe {
    stem: String,
    stats: super::lmu_probe::ShmStats,
    last: Option<Frame>,
    api: Option<std::thread::JoinHandle<ApiResult>>,
    /// Réponse de l'API, une fois la lecture terminée.
    api_done: Option<(Vec<super::lmu_probe::ApiRead>, super::lmu_probe::GarageSummary)>,
}

impl LmuDriver {
    pub fn new(dir: PathBuf, hz: f32) -> Self {
        Self { dir, recorder: Recorder::new(hz), live: LiveState::default(), yaml_key: None, last_part: Instant::now(), tick: 0, out_since: None, idle_after: Duration::from_secs(30), written: vec![], write_lock: Default::default(), part_busy: Default::default(), probe_dir: None, probe_api: super::lmu_probe::API_BASE.into(), probe: None, facts_key: None }
    }

    fn save(&mut self, r: &Recording, done: bool) {
        let _w = self.write_lock.lock().unwrap_or_else(|p| p.into_inner());
        match write_recording(&self.dir, r, done) {
            Ok(p) if done => {
                log::info!("séance LMU enregistrée : {}", p.display());
                self.written.push(p);
            }
            Ok(_) => {}
            Err(e) => log::warn!("enregistrement LMU : {e}"),
        }
    }

    /// Séance abandonnée (trop courte) : son fichier provisoire éventuel est supprimé, sinon il serait
    /// repris et importé comme une séance.
    fn drop_part(&self, stem: &str) {
        let _w = self.write_lock.lock().unwrap_or_else(|p| p.into_inner());
        let _ = std::fs::remove_file(self.dir.join(format!("{stem}.{}", rec::PART_EXT)));
    }

    /// Écrit le rapport de la sonde (dans un autre fil : la lecture de l'API peut durer quelques secondes).
    fn flush_probe(&mut self) {
        let (Some(p), Some(dir)) = (self.probe.take(), self.probe_dir.clone()) else { return };
        let Some(last) = p.last else { return };
        std::thread::spawn(move || {
            let mut r = super::lmu_probe::ProbeReport::new(&p.stem, &last, &p.stats);
            let done = p.api_done.or_else(|| p.api.and_then(|h| h.join().ok()).map(|(a, g, _)| (a, g)));
            if let Some((api, garage)) = done {
                r.api = api;
                r.garage = garage;
            }
            match super::lmu_probe::write_report(&dir, &r) {
                Ok(f) => log::info!("sonde setup LMU : {}", f.display()),
                Err(e) => log::warn!("sonde setup LMU : {e}"),
            }
        });
    }

    /// Alimente la sonde : une par séance, l'API du jeu lue une fois par séance, voiture en piste.
    fn feed_probe(&mut self, f: &Frame, on_track: bool) {
        let Some(dir) = self.probe_dir.clone() else { return };
        let stem = self.recorder.current().map(file_stem);
        if self.probe.as_ref().map(|p| &p.stem) != stem.as_ref() {
            self.flush_probe();
            self.probe = stem.map(|stem| Probe { stem, stats: Default::default(), last: None, api: None, api_done: None });
        }
        let Some(p) = self.probe.as_mut() else { return };
        // Lecture de l'API terminée : le setup de la séance rejoint l'enregistrement.
        if p.api.as_ref().is_some_and(|h| h.is_finished()) {
            if let Some((api, garage, setup)) = p.api.take().and_then(|h| h.join().ok()) {
                p.api_done = Some((api, garage));
                if let Some(s) = setup {
                    self.recorder.set_setup(s);
                }
            }
            return;
        }
        let Some(t) = f.telem.as_ref().filter(|_| on_track) else { return };
        p.stats.add(t);
        p.last = Some(*f);
        if p.api.is_none() && p.api_done.is_none() && !self.probe_api.is_empty() {
            let (base, stem) = (self.probe_api.clone(), p.stem.clone());
            p.api = Some(std::thread::spawn(move || super::lmu_probe::read_api(&base, &dir, &stem)));
        }
    }

    /// Termine la séance en cours : écrite si elle vaut la peine, sinon provisoire supprimé.
    fn close(&mut self) {
        self.flush_probe();
        let stem = self.recorder.current().map(file_stem);
        match self.recorder.finish() {
            Some(r) => self.save(&r, true),
            None => {
                if let Some(s) = stem {
                    self.drop_part(&s);
                }
            }
        }
    }

    /// Dès qu'une voiture est chargée dans le jeu (au garage aussi) : ce que le jeu annonce pour elle.
    fn read_facts(&mut self, f: &Frame, now_unix: i64) {
        let (Some(dir), Some(t)) = (self.probe_dir.clone(), f.telem.as_ref()) else { return };
        if self.probe_api.is_empty() {
            return;
        }
        let key = (crate::ingest::lmu::track_name(f), f.info.mSession, crate::ingest::lmu::car_name(t));
        if self.facts_key.as_ref() == Some(&key) {
            return;
        }
        self.facts_key = Some(key.clone());
        let class = f.scoring.as_ref().map(|s| crate::connectors::lmu_shm::cstr(&s.mVehicleClass)).filter(|c| !c.is_empty());
        let minutes = Some((f.info.mEndET / 60.0) as f32).filter(|m| m.is_finite() && *m > 1.0 && *m < 100_000.0);
        let facts = super::lmu_probe::GameFacts { track: key.0, car: key.2, car_class: class, session: key.1, at_unix: now_unix, minutes, ..Default::default() };
        let base = self.probe_api.clone();
        std::thread::spawn(move || {
            if super::lmu_probe::read_game_facts(&base, &dir, facts).is_none() {
                log::info!("jeu : estimations non lues (API du jeu injoignable)");
            }
        });
    }

    pub fn step(&mut self, f: &Frame, now_unix: i64) -> LiveSnapshot {
        self.read_facts(f, now_unix);
        let before = self.recorder.current().map(file_stem);
        let done = self.recorder.push(f, now_unix);
        let changed = before.is_some() && self.recorder.current().map(file_stem) != before;
        if let Some(done) = done {
            self.save(&done, true);
            self.live = LiveState::default();
        } else if changed {
            self.drop_part(before.as_deref().unwrap_or_default());
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
                self.close();
            }
        }
        // Sauvegarde provisoire toutes les 2 minutes : rien de perdu si Apex est fermé brutalement.
        // Compression et écriture dans un autre fil : sur une longue séance elles prennent plus d'une
        // seconde, et la lecture à 50 Hz ne doit pas s'interrompre (le tour serait perdu).
        if self.last_part.elapsed() >= Duration::from_secs(120) && !self.part_busy.load(Ordering::SeqCst) {
            self.last_part = Instant::now();
            if let Some(r) = self.recorder.current().filter(|r| r.ticks.len() > 500).cloned() {
                let (dir, lock, busy) = (self.dir.clone(), self.write_lock.clone(), self.part_busy.clone());
                busy.store(true, Ordering::SeqCst);
                std::thread::spawn(move || {
                    let _w = lock.lock().unwrap_or_else(|p| p.into_inner());
                    if let Err(e) = write_recording(&dir, &r, false) {
                        log::warn!("sauvegarde provisoire LMU : {e}");
                    }
                    busy.store(false, Ordering::SeqCst);
                });
            }
        }
        self.tick += 1;
        let row = self.live.row(f);
        self.feed_probe(f, row.is_some());
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
        self.close();
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
                let probe_dir = dir.parent().map(|p| p.join("sonde-lmu"));
                let mut driver = LmuDriver::new(dir, hz);
                driver.probe_dir = probe_dir;
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
    fn a_discarded_short_run_leaves_no_part_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut d = LmuDriver::new(dir.path().to_path_buf(), 50.0);
        let mut et = 0.0f64;
        while et < 10.0 {
            d.step(&frame(et, 1), 0);
            et += 0.02;
        }
        // Provisoire écrit pendant la séance (Apex aurait pu être fermé ici).
        let r = d.recorder.current().unwrap().clone();
        let part = write_recording(dir.path(), &r, false).unwrap();
        assert!(part.exists());
        d.finish();
        assert!(d.written.is_empty(), "10 s de roulage : rien à garder");
        assert!(!part.exists(), "provisoire d'une séance abandonnée laissé sur le disque");
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

    #[test]
    fn setup_probe_writes_one_report_per_session_without_the_game_api() {
        let dir = tempfile::tempdir().unwrap();
        let probe = dir.path().join("sonde-lmu");
        let mut d = LmuDriver::new(dir.path().join("enregistrements-lmu"), 50.0);
        d.probe_dir = Some(probe.clone());
        // API non lue ici (sous Windows, un port fermé met plusieurs secondes à répondre) : testée à part.
        d.probe_api = String::new();
        let mut et = 0.0f64;
        while et < 40.0 {
            let mut f = frame(et, 1);
            if let Some(t) = f.telem.as_mut() {
                t.mWheels[0].mTemperature = [363.15, 358.15, 353.15];
                t.mFrontDownforce = 4000.0;
            }
            d.step(&f, 1_790_000_000);
            et += 0.02;
        }
        d.finish();
        let mut report = None;
        for _ in 0..100 {
            if let Some(e) = std::fs::read_dir(&probe).ok().and_then(|mut r| r.find_map(|e| e.ok().filter(|e| e.path().extension().is_some_and(|x| x == "json")))) {
                report = Some(e.path());
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(report.expect("rapport de sonde")).unwrap()).unwrap();
        assert!(v["frames"].as_u64().unwrap() > 1000);
        let field = |n: &str| v["fields"].as_array().unwrap().iter().find(|x| x["name"] == n).unwrap()["filled_pct"].as_f64().unwrap();
        // Une roue sur quatre renseignée ; appui toujours fourni ; température optimale jamais.
        assert_eq!(field("pneu_surface_c"), 25.0);
        assert_eq!(field("appui_av_n"), 100.0);
        assert_eq!(field("pneu_temperature_optimale"), 0.0);
        assert!(v["api"].as_array().unwrap().is_empty());
    }

    /// Le setup lu dans l'API du jeu au départ de la séance est enregistré avec elle (API simulée).
    #[test]
    fn the_session_setup_is_saved_with_the_recording() {
        use std::io::{Read, Write};
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", server.local_addr().unwrap());
        std::thread::spawn(move || {
            for mut c in server.incoming().flatten().take(64) {
                let mut buf = [0u8; 2048];
                let n = c.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let body = if req.contains("CarSetupOverview") {
                    r#"{"carPresetSetups":{"presets":[{"translated":"fast"}]},"carSetup":{"garageValues":{"VM_REAR_WING":{"value":17,"stringValue":"P18","minValue":0,"maxValue":18}}}}"#
                } else if req.contains("TireManagement") {
                    r#"{"optimalCompoundConditions":{"compounds":[{"type":"Medium","optimalTemperature":92.0}]}}"#
                } else {
                    "{}"
                };
                let _ = write!(c, "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let rec_dir = dir.path().join("enregistrements-lmu");
        let mut d = LmuDriver::new(rec_dir.clone(), 50.0);
        d.probe_dir = Some(dir.path().join("sonde-lmu"));
        d.probe_api = base;
        let mut et = 0.0f64;
        while et < 60.0 {
            d.step(&frame(et, 1), 1_790_000_000);
            et += 0.02;
            // Laisse le temps à la lecture de l'API (fil séparé) sans bloquer la boucle.
            if (et * 50.0).round() as i64 % 500 == 0 {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        d.finish();
        let f = std::fs::read_dir(&rec_dir).unwrap().flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "apexlmu")).expect("séance écrite");
        let r = crate::ingest::lmu::decode(&std::fs::read(f).unwrap()).unwrap();
        let s = r.setup.expect("setup enregistré");
        assert_eq!((s.values[0].key.as_str(), s.values[0].display.as_str(), s.values[0].max), ("VM_REAR_WING", "P18", 18));
        assert_eq!(s.optimal_temp_c, [("Medium".to_string(), 92.0)]);
        assert!(!r.wheels.is_empty());
    }
}
