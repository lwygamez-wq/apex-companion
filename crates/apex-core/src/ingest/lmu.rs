//! Le Mans Ultimate : enregistrement des sessions depuis la mémoire partagée du jeu, et import.
//!
//! LMU n'écrit pas de fichier de télémétrie lisible en fin de session : Apex enregistre lui-même
//! la voiture du pilote pendant qu'il roule (`Recorder`, ~50 Hz), puis écrit un fichier `.apexlmu`
//! par séance. L'import découpe ces fichiers en tours avec le même code que les `.ibt` iRacing.
//!
//! Particularités de la source :
//! - la télémétrie (≈100 Hz) n'a pas de distance depuis la ligne ; seul le classement (≈5 Hz) la
//!   donne. La distance de chaque échantillon est reconstruite en intégrant la vitesse entre deux
//!   relevés du classement, recalée sur chacun d'eux ;
//! - pas de GPS : la trajectoire est en mètres dans le repère du jeu (x, z) ;
//! - temps au tour et secteurs officiels repris du classement, invalidation de tour (limites de
//!   piste) reprise du jeu.

use super::ibt::{split_laps, Channels, SessionInfo, LIVE_N};
use crate::connectors::lmu_shm::{cstr, Frame, VehicleScoring, VehicleTelemetry};
use crate::model::*;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

/// Extension des enregistrements terminés.
pub const EXT: &str = "apexlmu";
/// Extension pendant l'écriture (séance en cours).
pub const PART_EXT: &str = "apexlmu-part";
const MAGIC: &[u8; 4] = b"APXL";
const FORMAT_VERSION: u32 = 1;

/// Code de matière façon iRacing (irsdk_TrkSurf), pour réutiliser le relevé de piste.
const MAT_ASPHALT: u8 = 1;
const MAT_KERB: u8 = 11;
const MAT_OFF: u8 = 15;
/// Bit « jaune » des drapeaux iRacing, repris pour les neutralisations.
const FLAG_YELLOW: f64 = 8.0;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Header {
    pub version: u32,
    pub game_version: i32,
    pub started_at_unix: i64,
    pub track: String,
    pub track_length_m: f32,
    pub car: String,
    pub car_class: Option<String>,
    pub vehicle_name: String,
    /// Numéro de séance LMU (0 test, 1-4 essais, 5-8 qualifs, 9 warm-up, 10-13 course).
    pub session: i32,
    pub fixed_setup: Option<bool>,
    pub fuel_capacity_l: Option<f32>,
    pub redline_rpm: Option<f32>,
    pub gears: Option<u8>,
    pub est_lap_s: Option<f32>,
    pub air_temp_c: Option<f32>,
    pub track_temp_c: Option<f32>,
    pub raining: Option<f32>,
    pub wetness: Option<f32>,
    pub hz: f32,
    pub field_size: Option<u32>,
    pub start_position: Option<u32>,
}

/// Relevé du classement : distance depuis la ligne à un instant donné.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub et: f64,
    pub lap_dist: f32,
    pub total_laps: i32,
}

/// Tour bouclé, vu par le classement.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LapEvent {
    pub et: f64,
    pub total_laps: i32,
    /// Temps officiel (s), ≤ 0 si le jeu ne l'a pas chronométré.
    pub lap_time: f64,
    /// Secteur 1, puis secteurs 1+2 (s).
    pub s1: f64,
    pub s2: f64,
    pub place: u8,
}

/// Un échantillon de la voiture du pilote.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Tick {
    pub et: f64,
    pub lap: i32,
    pub speed: f32,
    pub throttle: f32,
    pub brake: f32,
    pub gear: f32,
    pub rpm: f32,
    pub steer_rad: f32,
    pub x: f32,
    pub z: f32,
    pub fuel: f32,
    pub yaw: f32,
    pub impacts: u32,
    pub pit: bool,
    pub off: bool,
    pub yellow: bool,
    pub invalid: bool,
    pub material: u8,
    pub place: u8,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recording {
    pub header: Header,
    pub ticks: Vec<Tick>,
    pub anchors: Vec<Anchor>,
    pub laps: Vec<LapEvent>,
}

#[derive(Serialize, Deserialize)]
struct Meta {
    header: Header,
    anchors: Vec<Anchor>,
    laps: Vec<LapEvent>,
}

// --- Conversion d'une image de la mémoire partagée ------------------------------------------------

pub fn speed_mps(t: &VehicleTelemetry) -> f32 {
    let v = t.mLocalVel;
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt() as f32
}

/// Matière sous la voiture (code iRacing) et hors-piste (quatre roues dehors).
pub fn surface(t: &VehicleTelemetry) -> (u8, bool) {
    let w = t.mWheels;
    let s = [w[0].mSurfaceType, w[1].mSurfaceType, w[2].mSurfaceType, w[3].mSurfaceType];
    let off = s.iter().filter(|x| matches!(x, 2..=4)).count();
    let mat = if off >= 3 {
        MAT_OFF
    } else if s.contains(&5) {
        MAT_KERB
    } else {
        MAT_ASPHALT
    };
    (mat, off == 4)
}

/// Angle volant (rad), signe iRacing (positif à gauche).
pub fn steer_rad(t: &VehicleTelemetry) -> f32 {
    let range = [t.mPhysicalSteeringWheelRange, t.mVisualSteeringWheelRange].into_iter().find(|r| *r > 1.0).unwrap_or(540.0);
    (-t.mUnfilteredSteering as f32) * (range / 2.0).to_radians()
}

pub fn car_name(t: &VehicleTelemetry) -> String {
    let model = cstr(&t.mVehicleModel);
    if !model.is_empty() {
        return model;
    }
    let name = cstr(&t.mVehicleName);
    if name.is_empty() { "Voiture inconnue".into() } else { name }
}

pub fn track_name(f: &Frame) -> String {
    let n = cstr(&f.info.mTrackName);
    if n.is_empty() { "Circuit inconnu".into() } else { n }
}

/// Libellé de séance façon iRacing (pour réutiliser `SessionKind::from_label`).
pub fn session_label(session: i32) -> &'static str {
    match session {
        0 => "Testing",
        1..=4 => "Practice",
        5..=8 => "Qualify",
        9 => "Warmup",
        10..=13 => "Race",
        _ => "",
    }
}

fn pos_f(v: f64) -> Option<f32> {
    (v.is_finite() && v > 0.0).then_some(v as f32)
}

fn header_from(f: &Frame, t: &VehicleTelemetry, s: Option<&VehicleScoring>, hz: f32, now_unix: i64) -> Header {
    let i = &f.info;
    let class = s.map(|s| cstr(&s.mVehicleClass)).filter(|c| !c.is_empty());
    Header {
        version: FORMAT_VERSION,
        game_version: f.game_version,
        started_at_unix: now_unix,
        track: track_name(f),
        track_length_m: i.mLapDist as f32,
        car: car_name(t),
        car_class: class,
        vehicle_name: cstr(&t.mVehicleName),
        session: i.mSession,
        fixed_setup: Some(i.mIsFixedSetup != 0),
        fuel_capacity_l: pos_f(t.mFuelCapacity),
        redline_rpm: pos_f(t.mEngineMaxRPM),
        gears: (t.mMaxGears > 0).then_some(t.mMaxGears),
        est_lap_s: s.and_then(|s| pos_f(s.mEstimatedLapTime)),
        air_temp_c: Some(i.mAmbientTemp as f32).filter(|v| v.is_finite() && *v > -60.0 && *v < 70.0),
        track_temp_c: Some(i.mTrackTemp as f32).filter(|v| v.is_finite() && *v > -60.0 && *v < 90.0),
        raining: Some(i.mRaining as f32).filter(|v| v.is_finite()),
        wetness: Some(i.mAvgPathWetness as f32).filter(|v| v.is_finite()),
        hz,
        field_size: (i.mNumVehicles > 0).then_some(i.mNumVehicles as u32),
        start_position: s.and_then(|s| (s.mQualification > 0).then_some(s.mQualification as u32)),
    }
}

fn tick_from(f: &Frame, t: &VehicleTelemetry, s: Option<&VehicleScoring>, impacts: u32) -> Tick {
    let (material, off) = surface(t);
    Tick {
        et: t.mElapsedTime,
        lap: t.mLapNumber,
        speed: speed_mps(t),
        throttle: t.mUnfilteredThrottle.clamp(0.0, 1.0) as f32,
        brake: t.mUnfilteredBrake.clamp(0.0, 1.0) as f32,
        gear: t.mGear as f32,
        rpm: t.mEngineRPM as f32,
        steer_rad: steer_rad(t),
        x: t.mPos.x as f32,
        z: t.mPos.z as f32,
        fuel: t.mFuel as f32,
        yaw: t.mLocalRot.y as f32,
        impacts,
        pit: t.mCurrentSector < 0 || s.is_some_and(|s| s.mInPits != 0),
        off,
        // Neutralisation générale seulement : les drapeaux jaunes locaux ne sont pas fiables ici.
        yellow: f.info.mGamePhase == 6,
        invalid: t.mLapInvalidated != 0,
        material,
        place: s.map(|s| s.mPlace).unwrap_or(0),
    }
}

// --- Enregistreur ---------------------------------------------------------------------------------

/// Empile les images de la mémoire partagée et rend une séance terminée quand une autre commence.
pub struct Recorder {
    hz: f32,
    rec: Option<Recording>,
    key: Option<(String, i32, String)>,
    last_et: f64,
    last_scoring_et: f64,
    last_total_laps: Option<i32>,
    last_impact_et: f64,
    impacts: u32,
}

impl Recorder {
    pub fn new(hz: f32) -> Self {
        Self { hz: hz.clamp(10.0, 100.0), rec: None, key: None, last_et: f64::NEG_INFINITY, last_scoring_et: f64::NEG_INFINITY, last_total_laps: None, last_impact_et: 0.0, impacts: 0 }
    }

    /// Séance en cours (pour l'affichage).
    pub fn current(&self) -> Option<&Recording> {
        self.rec.as_ref()
    }

    /// Ajoute une image. Renvoie la séance précédente quand celle-ci vient de se terminer
    /// (autre circuit, autre voiture, autre séance, ou redémarrage).
    pub fn push(&mut self, f: &Frame, now_unix: i64) -> Option<Recording> {
        let t = f.telem.as_ref()?;
        let s = f.scoring.as_ref();
        if f.info.mInRealtime == 0 || f.info.mLapDist <= 100.0 {
            return None;
        }
        let key = (track_name(f), f.info.mSession, car_name(t));
        let restarted = t.mElapsedTime + 1.0 < self.last_et;
        let mut done = None;
        if self.key.as_ref() != Some(&key) || restarted {
            done = self.finish();
            self.key = Some(key);
            self.rec = Some(Recording { header: header_from(f, t, s, self.hz, now_unix), ..Default::default() });
            self.last_et = f64::NEG_INFINITY;
            self.last_scoring_et = f64::NEG_INFINITY;
            self.last_total_laps = s.map(|s| s.mTotalLaps as i32);
            self.last_impact_et = t.mLastImpactET;
            self.impacts = 0;
        }
        let rec = self.rec.as_mut()?;
        if t.mLastImpactET != self.last_impact_et {
            if t.mLastImpactET > 0.0 {
                self.impacts += 1;
            }
            self.last_impact_et = t.mLastImpactET;
        }
        if let Some(s) = s {
            if f.info.mCurrentET != self.last_scoring_et {
                self.last_scoring_et = f.info.mCurrentET;
                rec.anchors.push(Anchor { et: f.info.mCurrentET, lap_dist: s.mLapDist as f32, total_laps: s.mTotalLaps as i32 });
                if rec.header.start_position.is_none() && s.mQualification > 0 {
                    rec.header.start_position = Some(s.mQualification as u32);
                }
            }
            let total = s.mTotalLaps as i32;
            if self.last_total_laps != Some(total) {
                if self.last_total_laps.is_some_and(|p| total > p) {
                    rec.laps.push(LapEvent { et: f.info.mCurrentET, total_laps: total, lap_time: s.mLastLapTime, s1: s.mLastSector1, s2: s.mLastSector2, place: s.mPlace });
                }
                self.last_total_laps = Some(total);
            }
        }
        // Cadence d'enregistrement bornée ; même instant (jeu en pause) ignoré.
        let min_dt = 1.0 / self.hz as f64 * 0.9;
        if t.mElapsedTime - self.last_et >= min_dt {
            self.last_et = t.mElapsedTime;
            rec.ticks.push(tick_from(f, t, s, self.impacts));
        }
        done
    }

    /// Termine la séance en cours (jeu fermé, application quittée). `None` si rien d'exploitable.
    pub fn finish(&mut self) -> Option<Recording> {
        self.key = None;
        let rec = self.rec.take()?;
        // Moins de 30 s en mouvement (voiture restée au garage, menus) : rien à garder.
        let moving: f64 = rec.ticks.windows(2).filter(|w| w[1].speed > 5.0).map(|w| (w[1].et - w[0].et).clamp(0.0, 1.0)).sum();
        (moving >= 30.0).then_some(rec)
    }
}

// --- Fichier .apexlmu -----------------------------------------------------------------------------

fn put_f32(out: &mut Vec<u8>, it: impl Iterator<Item = f32>) {
    for v in it {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

/// Fichier compressé : en-tête JSON, puis échantillons en colonnes.
pub fn encode(r: &Recording) -> Vec<u8> {
    let meta = serde_json::to_vec(&Meta { header: r.header.clone(), anchors: r.anchors.clone(), laps: r.laps.clone() }).unwrap_or_default();
    let n = r.ticks.len();
    let mut raw = Vec::with_capacity(16 + meta.len() + n * 70);
    raw.extend_from_slice(MAGIC);
    raw.extend_from_slice(&(meta.len() as u32).to_le_bytes());
    raw.extend_from_slice(&meta);
    raw.extend_from_slice(&(n as u32).to_le_bytes());
    for t in &r.ticks {
        raw.extend_from_slice(&t.et.to_le_bytes());
    }
    for t in &r.ticks {
        raw.extend_from_slice(&t.lap.to_le_bytes());
    }
    for t in &r.ticks {
        raw.extend_from_slice(&t.impacts.to_le_bytes());
    }
    let cols: [fn(&Tick) -> f32; 11] = [|t| t.speed, |t| t.throttle, |t| t.brake, |t| t.gear, |t| t.rpm, |t| t.steer_rad, |t| t.x, |t| t.z, |t| t.fuel, |t| t.yaw, |t| t.place as f32];
    for c in cols {
        put_f32(&mut raw, r.ticks.iter().map(c));
    }
    for t in &r.ticks {
        raw.push((t.pit as u8) | (t.off as u8) << 1 | (t.yellow as u8) << 2 | (t.invalid as u8) << 3);
    }
    for t in &r.ticks {
        raw.push(t.material);
    }
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = gz.write_all(&raw);
    gz.finish().unwrap_or_default()
}

pub fn decode(bytes: &[u8]) -> Result<Recording, String> {
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(bytes).read_to_end(&mut raw).map_err(|e| format!("fichier LMU illisible : {e}"))?;
    let mut o = 0usize;
    let take = |o: &mut usize, n: usize| -> Result<&[u8], String> {
        let s = raw.get(*o..*o + n).ok_or("fichier LMU tronqué")?;
        *o += n;
        Ok(s)
    };
    if take(&mut o, 4)? != MAGIC {
        return Err("pas un enregistrement Apex de Le Mans Ultimate".into());
    }
    let ml = u32::from_le_bytes(take(&mut o, 4)?.try_into().unwrap()) as usize;
    let meta: Meta = serde_json::from_slice(take(&mut o, ml)?).map_err(|e| format!("en-tête LMU : {e}"))?;
    let n = u32::from_le_bytes(take(&mut o, 4)?.try_into().unwrap()) as usize;
    let mut ticks = vec![Tick::default(); n];
    for (i, c) in take(&mut o, n * 8)?.chunks_exact(8).enumerate() {
        ticks[i].et = f64::from_le_bytes(c.try_into().unwrap());
    }
    for (i, c) in take(&mut o, n * 4)?.chunks_exact(4).enumerate() {
        ticks[i].lap = i32::from_le_bytes(c.try_into().unwrap());
    }
    for (i, c) in take(&mut o, n * 4)?.chunks_exact(4).enumerate() {
        ticks[i].impacts = u32::from_le_bytes(c.try_into().unwrap());
    }
    let sets: [fn(&mut Tick, f32); 11] = [
        |t, v| t.speed = v,
        |t, v| t.throttle = v,
        |t, v| t.brake = v,
        |t, v| t.gear = v,
        |t, v| t.rpm = v,
        |t, v| t.steer_rad = v,
        |t, v| t.x = v,
        |t, v| t.z = v,
        |t, v| t.fuel = v,
        |t, v| t.yaw = v,
        |t, v| t.place = v as u8,
    ];
    for set in sets {
        for (i, c) in take(&mut o, n * 4)?.chunks_exact(4).enumerate() {
            set(&mut ticks[i], f32::from_le_bytes(c.try_into().unwrap()));
        }
    }
    for (i, b) in take(&mut o, n)?.iter().enumerate() {
        ticks[i].pit = b & 1 != 0;
        ticks[i].off = b & 2 != 0;
        ticks[i].yellow = b & 4 != 0;
        ticks[i].invalid = b & 8 != 0;
    }
    for (i, b) in take(&mut o, n)?.iter().enumerate() {
        ticks[i].material = *b;
    }
    Ok(Recording { header: meta.header, ticks, anchors: meta.anchors, laps: meta.laps })
}

// --- Distance depuis la ligne ---------------------------------------------------------------------

/// Distance « déroulée » (tours × longueur + distance) de chaque échantillon : vitesse intégrée,
/// recalée sur les relevés du classement.
pub fn unwrapped_distance(ticks: &[Tick], anchors: &[Anchor], len: f64) -> Vec<f64> {
    let n = ticks.len();
    let mut s = vec![0.0f64; n];
    for i in 1..n {
        let dt = (ticks[i].et - ticks[i - 1].et).clamp(0.0, 0.5);
        s[i] = s[i - 1] + 0.5 * (ticks[i].speed + ticks[i - 1].speed) as f64 * dt;
    }
    if anchors.is_empty() || len <= 0.0 {
        return s;
    }
    // Relevés déroulés : un passage de ligne = la distance retombe de plus d'une demi-longueur.
    let mut a: Vec<(f64, f64)> = Vec::with_capacity(anchors.len());
    let mut wraps = 0.0;
    for (k, x) in anchors.iter().enumerate() {
        if k > 0 {
            let d = x.lap_dist as f64 - anchors[k - 1].lap_dist as f64;
            if d < -len / 2.0 {
                wraps += 1.0;
            } else if d > len / 2.0 {
                wraps -= 1.0;
            }
        }
        if a.last().is_none_or(|p| x.et > p.0) {
            a.push((x.et, x.lap_dist as f64 + wraps * len));
        }
    }
    // Vitesse intégrée à l'instant de chaque relevé (interpolée entre échantillons).
    let mut j = 0usize;
    let s_at: Vec<f64> = a
        .iter()
        .map(|(et, _)| {
            while j + 1 < n && ticks[j + 1].et <= *et {
                j += 1;
            }
            if j + 1 < n && ticks[j].et <= *et {
                let span = ticks[j + 1].et - ticks[j].et;
                let f = if span > 0.0 { (et - ticks[j].et) / span } else { 0.0 };
                s[j] + f * (s[j + 1] - s[j])
            } else if *et < ticks[0].et {
                s[0] - (ticks[0].et - et) * ticks[0].speed as f64
            } else {
                s[n - 1] + (et - ticks[n - 1].et) * ticks[n - 1].speed as f64
            }
        })
        .collect();
    let mut k = 0usize;
    (0..n)
        .map(|i| {
            let et = ticks[i].et;
            while k + 1 < a.len() && a[k + 1].0 <= et {
                k += 1;
            }
            if et < a[0].0 {
                return a[0].1 - (s_at[0] - s[i]);
            }
            if k + 1 >= a.len() {
                return a[k].1 + (s[i] - s_at[k]);
            }
            let (ds, dd) = (s_at[k + 1] - s_at[k], a[k + 1].1 - a[k].1);
            let r = if ds > 1.0 && dd > 1.0 { dd / ds } else { 1.0 };
            let r = if (0.7..1.3).contains(&r) { r } else { 1.0 };
            a[k].1 + (s[i] - s_at[k]) * r
        })
        .collect()
}

// --- Import ---------------------------------------------------------------------------------------

pub fn import_lmu_bytes(bytes: &[u8]) -> Result<ImportedSession, String> {
    let r = decode(bytes)?;
    import_recording(&r)
}

pub fn import_lmu_file(path: &std::path::Path) -> Result<ImportedSession, String> {
    let b = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut s = import_lmu_bytes(&b)?;
    s.meta.source_path = Some(path.to_string_lossy().into_owned());
    Ok(s)
}

pub fn import_recording(r: &Recording) -> Result<ImportedSession, String> {
    let h = &r.header;
    let n = r.ticks.len();
    if n < 50 {
        return Err(format!("trop peu d'échantillons ({n})"));
    }
    let len = h.track_length_m as f64;
    let d = unwrapped_distance(&r.ticks, &r.anchors, len);
    let lap_dist: Vec<f64> = d.iter().map(|x| if len > 0.0 { x.rem_euclid(len) } else { *x }).collect();
    let pct: Vec<f64> = lap_dist.iter().map(|x| if len > 0.0 { x / len } else { 0.0 }).collect();
    let col = |f: &dyn Fn(&Tick) -> f64| -> Vec<f64> { r.ticks.iter().map(f).collect() };
    let ch = Channels {
        session_time: col(&|t| t.et),
        lap_dist,
        lap_dist_pct: pct,
        speed: col(&|t| t.speed as f64),
        throttle: col(&|t| t.throttle as f64),
        brake: col(&|t| t.brake as f64),
        lap: Some(col(&|t| t.lap as f64)),
        gear: Some(col(&|t| t.gear as f64)),
        rpm: Some(col(&|t| t.rpm as f64)),
        steer: Some(col(&|t| t.steer_rad as f64)),
        fuel: Some(col(&|t| t.fuel as f64)),
        on_pit_road: Some(col(&|t| t.pit as u8 as f64)),
        track_surface: Some(col(&|t| if t.off { 0.0 } else { 3.0 })),
        incidents: Some(col(&|t| t.impacts as f64)),
        session_flags: Some(col(&|t| if t.yellow { FLAG_YELLOW } else { 0.0 })),
        session_num: Some(vec![h.session as f64; n]),
        yaw_rate: Some(col(&|t| t.yaw as f64)),
        position: Some(col(&|t| t.place as f64)),
        surface_material: Some(col(&|t| t.material as f64)),
        xy: Some((col(&|t| t.x as f64), col(&|t| t.z as f64))),
        extra: vec![("lap_invalid".into(), col(&|t| t.invalid as u8 as f64))],
        ..Default::default()
    };
    let si = SessionInfo { track_length_m: Some(h.track_length_m).filter(|l| *l > 100.0), sim: Some(SimId::Lmu), ..Default::default() };
    let mut laps = split_laps(&ch, &si);
    apply_official_times(&mut laps, &r.laps);
    for l in &mut laps {
        // Invalidation du jeu (limites de piste) : on ignore la première demi-seconde, où le drapeau
        // du tour précédent peut encore être levé.
        if let Some(inv) = l.trace.extra.remove("lap_invalid") {
            if l.meta.kind == LapKind::Flying {
                let flagged = inv.iter().zip(&l.trace.time_s).any(|(v, t)| *v > 0.5 && *t > 0.5);
                l.meta.validity_source = ValiditySource::SimReported;
                if flagged {
                    l.meta.validity = Validity::Invalid;
                }
            }
        }
    }

    let kind = SessionKind::from_label(session_label(h.session));
    let race = (kind == SessionKind::Race).then(|| race_data(r, &ch));
    let fp = super::ibt::fnv1a64(format!("{}|{}|{}|{}", h.track, h.car, h.session, h.vehicle_name).as_bytes(), 0xcbf29ce484222325) ^ (h.started_at_unix as u64).rotate_left(17) ^ (n as u64).rotate_left(41);
    let wet = match (h.raining, h.wetness) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0.0) > 0.1 || b.unwrap_or(0.0) > 0.1),
    };
    let profile = SessionProfile {
        car: CarProfile { class: h.car_class.clone(), fuel_max_l: h.fuel_capacity_l, redline_rpm: h.redline_rpm, gears: h.gears, est_lap_s: h.est_lap_s, ..Default::default() },
        track: TrackProfile { short_name: Some(h.track.clone()), ..Default::default() },
    };
    Ok(ImportedSession {
        meta: SessionMeta {
            sim: SimId::Lmu,
            track: h.track.clone(),
            track_config: None,
            track_length_m: Some(h.track_length_m).filter(|l| *l > 100.0),
            car: h.car.clone(),
            kind,
            started_at_unix: h.started_at_unix,
            weather: Weather {
                air_temp_c: h.air_temp_c,
                track_temp_c: h.track_temp_c,
                description: wet.map(|w| if w { "Pluie".to_string() } else { "Sec".to_string() }),
                wet,
            },
            source_fingerprint: format!("lmu:{fp:016x}"),
            source_path: None,
            setup: h.fixed_setup.map(|f| if f { SetupType::Fixed } else { SetupType::Open }),
        },
        laps,
        sample_rate_hz: Some(h.hz),
        race,
        profile: Some(profile),
    })
}

/// Temps officiels et secteurs du jeu, appariés aux tours découpés (dans l'ordre, à 0,3 s près).
fn apply_official_times(laps: &mut [ImportedLap], events: &[LapEvent]) {
    let mut p = 0usize;
    for l in laps.iter_mut() {
        let Some(t) = l.meta.lap_time_s else { continue };
        let found = (p..events.len().min(p + 4)).find(|&k| events[k].lap_time > 0.0 && (events[k].lap_time - t).abs() < 0.3);
        if let Some(k) = found {
            let e = events[k];
            l.meta.lap_time_s = Some(e.lap_time);
            if e.s1 > 0.0 && e.s2 > e.s1 && e.lap_time > e.s2 {
                l.meta.sector_times_s = vec![e.s1, e.s2 - e.s1, e.lap_time - e.s2];
            }
            p = k + 1;
        }
    }
}

fn race_data(r: &Recording, ch: &Channels) -> RaceData {
    let positions: Vec<(i32, u32)> = r.laps.iter().filter(|e| e.place > 0).map(|e| (e.total_laps, e.place as u32)).collect();
    let mut incidents = vec![];
    for i in 1..r.ticks.len() {
        if r.ticks[i].impacts > r.ticks[i - 1].impacts {
            incidents.push(RaceIncident { lap: r.ticks[i].lap, dist_m: ch.lap_dist[i] as f32, kind: IncidentKind::Other, points: 0, time_s: r.ticks[i].et });
        }
    }
    RaceData {
        field_size: r.header.field_size.unwrap_or(0),
        start_position: r.header.start_position.or_else(|| r.ticks.iter().find(|t| t.place > 0).map(|t| t.place as u32)),
        finish_position: r.laps.last().map(|e| e.place as u32).filter(|p| *p > 0).or_else(|| r.ticks.iter().rev().find(|t| t.place > 0).map(|t| t.place as u32)),
        positions,
        incident_total: 0,
        incidents,
        ..Default::default()
    }
}

// --- Direct (coach) -------------------------------------------------------------------------------

/// Suivi en direct : distance depuis la ligne et échantillons au format du coach (`LIVE_CHANNELS`).
#[derive(Default)]
pub struct LiveState {
    s: f64,
    last_et: Option<f64>,
    last_speed: f64,
    anchor: Option<(f64, f64)>,
    anchor_et: f64,
    wraps: f64,
    prev_lap_dist: Option<f64>,
    impacts: u32,
    last_impact_et: f64,
}

impl LiveState {
    /// Échantillon au format du coach, ou `None` sans voiture en piste.
    pub fn row(&mut self, f: &Frame) -> Option<[f64; LIVE_N]> {
        let t = f.telem.as_ref()?;
        let len = f.info.mLapDist;
        if len <= 100.0 || f.info.mInRealtime == 0 {
            return None;
        }
        let v = speed_mps(t) as f64;
        let et = t.mElapsedTime;
        if let Some(p) = self.last_et {
            if et < p - 1.0 {
                *self = LiveState::default();
            } else {
                self.s += 0.5 * (v + self.last_speed) * (et - p).clamp(0.0, 0.5);
            }
        }
        self.last_et = Some(et);
        self.last_speed = v;
        if t.mLastImpactET != self.last_impact_et {
            if t.mLastImpactET > 0.0 && self.last_impact_et != 0.0 {
                self.impacts += 1;
            }
            self.last_impact_et = t.mLastImpactET;
        }
        let s = f.scoring.as_ref();
        if let Some(sc) = s {
            if f.info.mCurrentET != self.anchor_et {
                self.anchor_et = f.info.mCurrentET;
                let ld = sc.mLapDist;
                if let Some(p) = self.prev_lap_dist {
                    if ld - p < -len / 2.0 {
                        self.wraps += 1.0;
                    } else if ld - p > len / 2.0 {
                        self.wraps -= 1.0;
                    }
                }
                self.prev_lap_dist = Some(ld);
                // Le relevé date de l'instant du classement : on retire le chemin parcouru depuis.
                let s_then = self.s - v * (et - f.info.mCurrentET).clamp(0.0, 0.5);
                self.anchor = Some((ld + self.wraps * len, s_then));
            }
        }
        let d = match self.anchor {
            Some((d0, s0)) => d0 + (self.s - s0),
            None => self.s,
        };
        let lap_dist = d.rem_euclid(len);
        let (mat, off) = surface(t);
        let i = &f.info;
        let time_remain = if i.mEndET > 0.0 { (i.mEndET - i.mCurrentET).max(0.0) } else { -1.0 };
        let laps_remain = match s {
            Some(sc) if i.mMaxLaps > 0 && i.mMaxLaps < 10_000 => (i.mMaxLaps - sc.mTotalLaps as i32).max(0) as f64,
            _ => 32767.0,
        };
        Some([
            et,
            lap_dist,
            lap_dist / len,
            v,
            t.mUnfilteredThrottle.clamp(0.0, 1.0),
            t.mUnfilteredBrake.clamp(0.0, 1.0),
            t.mLapNumber as f64,
            t.mGear as f64,
            t.mEngineRPM,
            steer_rad(t) as f64,
            0.0,
            0.0,
            t.mFuel,
            (t.mCurrentSector < 0 || s.is_some_and(|s| s.mInPits != 0)) as u8 as f64,
            if off { 0.0 } else { 3.0 },
            self.impacts as f64,
            if i.mGamePhase == 6 { FLAG_YELLOW } else { 0.0 },
            i.mSession as f64,
            t.mLocalRot.y,
            mat as f64,
            time_remain,
            laps_remain,
        ])
    }
}

/// YAML de séance façon iRacing, pour que le coach en direct traite LMU comme iRacing.
pub fn live_yaml(f: &Frame) -> Option<String> {
    let t = f.telem.as_ref()?;
    let s = f.scoring.as_ref();
    let i = &f.info;
    let one_line = |v: String| v.replace(['\n', '\r'], " ");
    let mut y = String::from("---\nWeekendInfo:\n ApexSim: lmu\n");
    y += &format!(" TrackDisplayName: {}\n", one_line(track_name(f)));
    y += &format!(" TrackLength: {:.4} km\n", i.mLapDist / 1000.0);
    y += &format!(" TrackAirTemp: {:.1} C\n TrackSurfaceTemp: {:.1} C\n", { i.mAmbientTemp }, { i.mTrackTemp });
    y += &format!(" WeekendOptions:\n  IsFixedSetup: {}\n", (i.mIsFixedSetup != 0) as u8);
    y += "SessionInfo:\n Sessions:\n";
    for n in 0..=13 {
        y += &format!(" - SessionNum: {n}\n   SessionType: {}\n", session_label(n));
    }
    y += "DriverInfo:\n DriverCarIdx: 0\n";
    if t.mFuelCapacity > 0.0 {
        y += &format!(" DriverCarFuelMaxLtr: {:.3}\n", { t.mFuelCapacity });
    }
    if t.mEngineMaxRPM > 0.0 {
        y += &format!(" DriverCarRedLine: {:.0}\n", { t.mEngineMaxRPM });
    }
    if t.mMaxGears > 0 {
        y += &format!(" DriverCarGearNumForward: {}\n", t.mMaxGears);
    }
    y += &format!(" Drivers:\n - CarIdx: 0\n   CarScreenName: {}\n", one_line(car_name(t)));
    if let Some(c) = s.map(|s| cstr(&s.mVehicleClass)).filter(|c| !c.is_empty()) {
        y += &format!("   CarClassShortName: {}\n", one_line(c));
    }
    Some(y)
}

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;
    use crate::connectors::lmu_shm::{zeroed, ScoringInfo};

    pub(crate) const LEN: f64 = 2000.0;

    /// Image synthétique : voiture à 50 m/s sur un cercle de 2 km, freinage au milieu du tour.
    pub(crate) fn frame(et: f64, d_abs: f64, scoring_et: f64, d_scoring: f64, session: i32) -> Frame {
        let mut info: ScoringInfo = zeroed();
        info.mTrackName[..12].copy_from_slice(b"Circuit Test");
        info.mLapDist = LEN;
        info.mSession = session;
        info.mInRealtime = 1;
        info.mNumVehicles = 12;
        info.mCurrentET = scoring_et;
        info.mIsFixedSetup = 1;
        let mut s: VehicleScoring = zeroed();
        s.mIsPlayer = 1;
        s.mPlace = 3;
        s.mLapDist = d_scoring.rem_euclid(LEN);
        s.mTotalLaps = (d_scoring / LEN).floor() as i16;
        // Temps officiel du dernier tour : 40 s pile (2 km à 50 m/s).
        s.mLastLapTime = if s.mTotalLaps > 0 { 40.0 } else { -1.0 };
        s.mLastSector1 = 13.0;
        s.mLastSector2 = 27.0;
        s.mVehicleClass[..4].copy_from_slice(b"LMP2");
        let mut t: VehicleTelemetry = zeroed();
        t.mElapsedTime = et;
        t.mLapNumber = (d_abs / LEN).floor() as i32;
        t.mLocalVel.z = 50.0;
        t.mUnfilteredThrottle = 1.0;
        let ld = d_abs.rem_euclid(LEN);
        if (900.0..1000.0).contains(&ld) {
            t.mUnfilteredBrake = 0.8;
        }
        let a = ld / LEN * std::f64::consts::TAU;
        let r = LEN / std::f64::consts::TAU;
        t.mPos.x = r * a.cos();
        t.mPos.z = r * a.sin();
        t.mFuel = 60.0 - d_abs / 1000.0;
        t.mFuelCapacity = 75.0;
        t.mVehicleModel[..13].copy_from_slice(b"Oreca 07 LMP2");
        t.mMaxGears = 6;
        // Tour 2 : limites de piste dépassées vers 1500 m.
        if t.mLapNumber == 2 && ld > 1500.0 {
            t.mLapInvalidated = 1;
        }
        Frame { game_version: 14000, info, scoring: Some(s), telem: Some(t) }
    }

    /// 4 tours et demi enregistrés à 50 Hz ; le classement n'est relevé qu'à 5 Hz.
    pub(crate) fn record(session: i32) -> Recording {
        let mut rec = Recorder::new(50.0);
        let start = 1500.0; // sortie des stands à 1500 m
        let total_s = (LEN * 4.5) / 50.0;
        let mut et = 0.0f64;
        while et < total_s {
            let d = start + et * 50.0;
            let sc_et = (et * 5.0).floor() / 5.0;
            let f = frame(100.0 + et, d, 100.0 + sc_et, start + sc_et * 50.0, session);
            assert!(rec.push(&f, 1_790_000_000).is_none());
            et += 0.01;
        }
        rec.finish().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

    #[test]
    fn recording_round_trips_through_the_file_format() {
        let r = record(1);
        assert!(r.ticks.len() > 8000, "{}", r.ticks.len());
        let back = decode(&encode(&r)).unwrap();
        assert_eq!(back.header, r.header);
        assert_eq!(back.ticks.len(), r.ticks.len());
        assert_eq!(back.ticks[1234], r.ticks[1234]);
        assert_eq!(back.anchors, r.anchors);
        assert_eq!(back.laps, r.laps);
    }

    #[test]
    fn distance_is_rebuilt_between_scoring_updates() {
        let r = record(1);
        let d = unwrapped_distance(&r.ticks, &r.anchors, LEN);
        // Vraie distance déroulée : 1500 + 50 × t.
        let worst = r.ticks.iter().zip(&d).map(|(t, d)| (d - (1500.0 + (t.et - 100.0) * 50.0)).abs()).fold(0.0, f64::max);
        assert!(worst < 2.0, "écart max {worst:.2} m");
    }

    #[test]
    fn imports_laps_with_official_times_and_validity() {
        let s = import_recording(&record(1)).unwrap();
        assert_eq!(s.meta.sim, SimId::Lmu);
        assert_eq!(s.meta.track, "Circuit Test");
        assert_eq!(s.meta.car, "Oreca 07 LMP2");
        assert_eq!(s.meta.kind, SessionKind::Practice);
        assert_eq!(s.meta.setup, Some(SetupType::Fixed));
        let flying: Vec<_> = s.laps.iter().filter(|l| l.meta.kind == LapKind::Flying).collect();
        assert!(flying.len() >= 3, "{:?}", s.laps.iter().map(|l| (l.meta.number, l.meta.kind)).collect::<Vec<_>>());
        for l in &flying {
            assert_eq!(l.meta.lap_time_s, Some(40.0));
            assert_eq!(l.meta.sector_times_s, vec![13.0, 14.0, 13.0]);
            let tr = &l.trace;
            assert!(tr.pos_x.is_some() && tr.pos_y.is_some());
            assert!((tr.dist_m.last().unwrap() - 2000.0).abs() < 5.0, "{}", tr.dist_m.last().unwrap());
            assert!(!tr.extra.contains_key("lap_invalid"));
        }
        let lap2 = s.laps.iter().find(|l| l.meta.number == 2).unwrap();
        assert_eq!(lap2.meta.validity, Validity::Invalid);
        assert_eq!(lap2.meta.validity_source, ValiditySource::SimReported);
        assert!(s.laps.iter().filter(|l| l.meta.number == 3).all(|l| l.meta.validity != Validity::Invalid));
        let p = s.profile.unwrap();
        assert_eq!(p.car.fuel_max_l, Some(75.0));
        assert_eq!(p.car.class.as_deref(), Some("LMP2"));
    }

    #[test]
    fn race_sessions_keep_positions() {
        let s = import_recording(&record(10)).unwrap();
        assert_eq!(s.meta.kind, SessionKind::Race);
        let race = s.race.unwrap();
        assert_eq!(race.field_size, 12);
        assert_eq!(race.finish_position, Some(3));
        assert!(!race.positions.is_empty());
    }

    #[test]
    fn a_new_session_closes_the_previous_one() {
        let mut rec = Recorder::new(50.0);
        let mut et = 0.0f64;
        while et < 60.0 {
            rec.push(&frame(10.0 + et, et * 50.0, 10.0 + et, et * 50.0, 1), 0);
            et += 0.02;
        }
        let done = rec.push(&frame(5.0, 0.0, 5.0, 0.0, 5), 0).expect("séance d'essais rendue");
        assert_eq!(done.header.session, 1);
        assert_eq!(rec.current().unwrap().header.session, 5);
    }

    #[test]
    fn live_rows_follow_the_lap() {
        let mut live = LiveState::default();
        let mut last = None;
        let mut crossed = 0;
        let mut et = 0.0f64;
        while et < 90.0 {
            let d = 1500.0 + et * 50.0;
            let sc_et = (et * 5.0).floor() / 5.0;
            let row = live.row(&frame(100.0 + et, d, 100.0 + sc_et, 1500.0 + sc_et * 50.0, 1)).unwrap();
            assert!((row[1] - d.rem_euclid(LEN)).abs() < 3.0 || (row[1] - d.rem_euclid(LEN)).abs() > LEN - 3.0, "{} vs {}", row[1], d.rem_euclid(LEN));
            if let Some(p) = last {
                if p > 0.9 && row[2] < 0.1 {
                    crossed += 1;
                }
            }
            last = Some(row[2]);
            et += 0.02;
        }
        assert_eq!(crossed, 2);
        let y = live_yaml(&frame(0.0, 0.0, 0.0, 0.0, 10)).unwrap();
        let meta = crate::live_coach::session_meta_from_yaml(&y);
        assert_eq!(meta.sim, SimId::Lmu);
        assert_eq!(meta.car, "Oreca 07 LMP2");
        assert_eq!(meta.track_length_m, Some(2000.0));
        assert_eq!(meta.setup, Some(SetupType::Fixed));
    }
}
