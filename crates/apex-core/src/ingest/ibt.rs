//! Lecture des fichiers de télémétrie iRacing `.ibt`.
//!
//! Format public décrit par `irsdk_defines.h` (SDK iRacing) : en-tête
//! `irsdk_header` (112 o), sous-en-tête disque (32 o), en-têtes de variables
//! (144 o chacun), YAML de session, puis enregistrements de `bufLen` octets.
//! Le même en-tête est utilisé par la mémoire partagée live (voir `iracing_live`).
//!
//! ⚠ Validé sur des fichiers synthétiques conformes à la spec ; à confirmer sur
//! des `.ibt` réels (voir tests d'intégration `tests/ibt_real.rs`).

use crate::model::*;
use std::collections::HashMap;
use std::path::Path;

pub const HEADER_LEN: usize = 112;
pub const DISK_SUBHEADER_LEN: usize = 32;
pub const VAR_HEADER_LEN: usize = 144;

#[derive(Debug, thiserror::Error)]
pub enum IbtError {
    #[error("lecture fichier : {0}")]
    Io(#[from] std::io::Error),
    #[error("fichier .ibt invalide : {0}")]
    Format(String),
    #[error("canal obligatoire absent : {0}")]
    MissingChannel(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarType {
    Char,
    Bool,
    Int,
    BitField,
    Float,
    Double,
}

impl VarType {
    fn from_i32(v: i32) -> Option<Self> {
        Some(match v {
            0 => VarType::Char,
            1 => VarType::Bool,
            2 => VarType::Int,
            3 => VarType::BitField,
            4 => VarType::Float,
            5 => VarType::Double,
            _ => return None,
        })
    }
    pub fn size(&self) -> usize {
        match self {
            VarType::Char | VarType::Bool => 1,
            VarType::Int | VarType::BitField | VarType::Float => 4,
            VarType::Double => 8,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VarHeader {
    pub ty: VarType,
    pub offset: usize,
    pub count: usize,
    pub name: String,
    pub unit: String,
}

#[derive(Debug, Clone)]
pub struct Header {
    pub ver: i32,
    pub status: i32,
    pub tick_rate: i32,
    pub session_info_len: usize,
    pub session_info_offset: usize,
    pub num_vars: usize,
    pub var_header_offset: usize,
    pub buf_len: usize,
    /// (tickCount, bufOffset) pour chacun des buffers.
    pub var_bufs: Vec<(i32, usize)>,
}

fn rd_i32(b: &[u8], o: usize) -> Result<i32, IbtError> {
    b.get(o..o + 4)
        .map(|s| i32::from_le_bytes(s.try_into().unwrap()))
        .ok_or_else(|| IbtError::Format(format!("lecture hors limites à {o}")))
}

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    // YAML iRacing en Windows-1252 : on remplace les octets non UTF-8.
    String::from_utf8_lossy(&b[..end]).into_owned()
}

pub fn parse_header(b: &[u8]) -> Result<Header, IbtError> {
    if b.len() < HEADER_LEN {
        return Err(IbtError::Format("en-tête tronqué".into()));
    }
    let num_buf = rd_i32(b, 32)?.clamp(0, 4) as usize;
    let var_bufs = (0..num_buf)
        .map(|i| Ok((rd_i32(b, 48 + i * 16)?, rd_i32(b, 52 + i * 16)? as usize)))
        .collect::<Result<Vec<_>, IbtError>>()?;
    let h = Header {
        ver: rd_i32(b, 0)?,
        status: rd_i32(b, 4)?,
        tick_rate: rd_i32(b, 8)?,
        session_info_len: rd_i32(b, 16)?.max(0) as usize,
        session_info_offset: rd_i32(b, 20)?.max(0) as usize,
        num_vars: rd_i32(b, 24)?.max(0) as usize,
        var_header_offset: rd_i32(b, 28)?.max(0) as usize,
        buf_len: rd_i32(b, 36)?.max(0) as usize,
        var_bufs,
    };
    if h.ver != 2 {
        log::warn!("version irsdk {} (attendu 2) : lecture tentée", h.ver);
    }
    if h.num_vars == 0 || h.num_vars > 4096 || h.buf_len == 0 || h.var_bufs.is_empty() {
        return Err(IbtError::Format(format!("en-tête incohérent (vars={}, bufLen={})", h.num_vars, h.buf_len)));
    }
    Ok(h)
}

pub fn parse_var_headers(b: &[u8], h: &Header) -> Result<Vec<VarHeader>, IbtError> {
    (0..h.num_vars)
        .map(|i| {
            let o = h.var_header_offset + i * VAR_HEADER_LEN;
            let s = b
                .get(o..o + VAR_HEADER_LEN)
                .ok_or_else(|| IbtError::Format("en-têtes de variables tronqués".into()))?;
            let ty = VarType::from_i32(rd_i32(s, 0)?)
                .ok_or_else(|| IbtError::Format(format!("type de variable inconnu (var {i})")))?;
            let v = VarHeader {
                ty,
                offset: rd_i32(s, 4)?.max(0) as usize,
                count: rd_i32(s, 8)?.max(1) as usize,
                name: cstr(&s[16..48]),
                unit: cstr(&s[112..144]),
            };
            if v.offset + v.ty.size() * v.count > h.buf_len {
                return Err(IbtError::Format(format!("variable {} hors du buffer", v.name)));
            }
            Ok(v)
        })
        .collect()
}

/// Lit la valeur scalaire (élément 0) d'une variable dans un enregistrement.
pub fn read_value(rec: &[u8], v: &VarHeader) -> f64 {
    let o = v.offset;
    match v.ty {
        VarType::Char | VarType::Bool => rec[o] as f64,
        VarType::Int | VarType::BitField => i32::from_le_bytes(rec[o..o + 4].try_into().unwrap()) as f64,
        VarType::Float => f32::from_le_bytes(rec[o..o + 4].try_into().unwrap()) as f64,
        VarType::Double => f64::from_le_bytes(rec[o..o + 8].try_into().unwrap()),
    }
}

/// Extraction minimaliste du YAML de session (sans dépendance YAML).
#[derive(Debug, Default, Clone)]
pub struct SessionInfo {
    pub track_display_name: Option<String>,
    pub track_config: Option<String>,
    pub track_length_m: Option<f32>,
    pub track_air_temp_c: Option<f32>,
    pub track_surface_temp_c: Option<f32>,
    pub track_skies: Option<String>,
    pub track_wetness: Option<String>,
    pub car_screen_name: Option<String>,
    pub session_types: Vec<(i32, String)>,
}

fn yaml_val<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let t = line.trim_start().trim_start_matches("- ");
    t.strip_prefix(key)?.strip_prefix(':').map(|v| v.trim().trim_matches('"'))
}

fn leading_f32(s: &str) -> Option<f32> {
    let num: String = s.chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
    num.parse().ok()
}

pub fn parse_session_info(yaml: &str) -> SessionInfo {
    let mut si = SessionInfo::default();
    let mut driver_car_idx: Option<i32> = None;
    let mut cur_car_idx: Option<i32> = None;
    let mut cars: HashMap<i32, String> = HashMap::new();
    let mut cur_session_num: Option<i32> = None;
    for line in yaml.lines() {
        if let Some(v) = yaml_val(line, "TrackDisplayName") {
            si.track_display_name = Some(v.to_string());
        } else if let Some(v) = yaml_val(line, "TrackConfigName") {
            if !v.is_empty() {
                si.track_config = Some(v.to_string());
            }
        } else if let Some(v) = yaml_val(line, "TrackLength") {
            // « 5.51 km » (ou « mi » selon version)
            si.track_length_m = leading_f32(v).map(|x| if v.contains("mi") { x * 1609.344 } else { x * 1000.0 });
        } else if let Some(v) = yaml_val(line, "TrackAirTemp") {
            si.track_air_temp_c = leading_f32(v);
        } else if let Some(v) = yaml_val(line, "TrackSurfaceTemp") {
            si.track_surface_temp_c = leading_f32(v);
        } else if let Some(v) = yaml_val(line, "TrackSkies") {
            si.track_skies = Some(v.to_string());
        } else if let Some(v) = yaml_val(line, "TrackWetness") {
            si.track_wetness = Some(v.to_string());
        } else if let Some(v) = yaml_val(line, "DriverCarIdx") {
            driver_car_idx = v.parse().ok();
        } else if let Some(v) = yaml_val(line, "CarIdx") {
            cur_car_idx = v.parse().ok();
        } else if let Some(v) = yaml_val(line, "CarScreenName") {
            if let Some(i) = cur_car_idx {
                cars.insert(i, v.to_string());
            }
        } else if let Some(v) = yaml_val(line, "SessionNum") {
            cur_session_num = v.parse().ok();
        } else if let Some(v) = yaml_val(line, "SessionType") {
            if let Some(n) = cur_session_num {
                si.session_types.push((n, v.to_string()));
            }
        }
    }
    si.car_screen_name = driver_car_idx.and_then(|i| cars.get(&i).cloned());
    si
}

/// Canaux lus (tous optionnels sauf ceux marqués requis).
struct Channels {
    session_time: Vec<f64>,
    lap_dist: Vec<f64>,
    lap_dist_pct: Vec<f64>,
    speed: Vec<f64>,
    throttle: Vec<f64>,
    brake: Vec<f64>,
    lap: Option<Vec<f64>>,
    gear: Option<Vec<f64>>,
    rpm: Option<Vec<f64>>,
    steer: Option<Vec<f64>>,
    lat: Option<Vec<f64>>,
    lon: Option<Vec<f64>>,
    fuel: Option<Vec<f64>>,
    on_pit_road: Option<Vec<f64>>,
    track_surface: Option<Vec<f64>>,
    incidents: Option<Vec<f64>>,
    session_flags: Option<Vec<f64>>,
    session_num: Option<Vec<f64>>,
    yaw_rate: Option<Vec<f64>>,
}

/// Drapeaux iRacing signalant une neutralisation locale ou globale.
const YELLOW_MASK: u32 = 0x0008 | 0x0100 | 0x4000 | 0x8000;
/// `irsdk_TrkLoc::irsdk_OffTrack`
const TRK_OFF_TRACK: f64 = 0.0;

pub fn fnv1a64(data: &[u8], mut h: u64) -> u64 {
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub fn import_ibt_file(path: &Path) -> Result<ImportedSession, IbtError> {
    let bytes = std::fs::read(path)?;
    let mut s = import_ibt_bytes(&bytes)?;
    s.meta.source_path = Some(path.to_string_lossy().into_owned());
    Ok(s)
}

pub fn import_ibt_bytes(b: &[u8]) -> Result<ImportedSession, IbtError> {
    let h = parse_header(b)?;
    let vars = parse_var_headers(b, &h)?;
    let sub = HEADER_LEN;
    if b.len() < sub + DISK_SUBHEADER_LEN {
        return Err(IbtError::Format("sous-en-tête disque absent".into()));
    }
    let start_date = i64::from_le_bytes(b[sub..sub + 8].try_into().unwrap());
    let record_count = rd_i32(b, sub + 28)?.max(0) as usize;
    let yaml = b
        .get(h.session_info_offset..h.session_info_offset + h.session_info_len)
        .map(cstr)
        .unwrap_or_default();
    let si = parse_session_info(&yaml);

    let data_off = h.var_bufs[0].1;
    let available = b.len().saturating_sub(data_off) / h.buf_len;
    let n = if record_count == 0 { available } else { record_count.min(available) };
    if n < 10 {
        return Err(IbtError::Format(format!("trop peu d'enregistrements ({n})")));
    }
    let by_name: HashMap<&str, &VarHeader> = vars.iter().map(|v| (v.name.as_str(), v)).collect();
    let read = |name: &str| -> Option<Vec<f64>> {
        let v = by_name.get(name)?;
        Some((0..n).map(|i| read_value(&b[data_off + i * h.buf_len..data_off + (i + 1) * h.buf_len], v)).collect())
    };
    let req = |name: &'static str| read(name).ok_or(IbtError::MissingChannel(name));
    let ch = Channels {
        session_time: req("SessionTime")?,
        lap_dist: req("LapDist")?,
        lap_dist_pct: req("LapDistPct")?,
        speed: req("Speed")?,
        throttle: req("Throttle")?,
        brake: req("Brake")?,
        lap: read("Lap"),
        gear: read("Gear"),
        rpm: read("RPM"),
        steer: read("SteeringWheelAngle"),
        lat: read("Lat"),
        lon: read("Lon"),
        fuel: read("FuelLevel"),
        on_pit_road: read("OnPitRoad"),
        track_surface: read("PlayerTrackSurface"),
        incidents: read("PlayerCarMyIncidentCount"),
        session_flags: read("SessionFlags"),
        session_num: read("SessionNum"),
        yaw_rate: read("YawRate"),
    };

    let laps = split_laps(&ch, &si);
    let session_label = ch
        .session_num
        .as_ref()
        .and_then(|s| s.first())
        .and_then(|&num| si.session_types.iter().find(|(k, _)| *k == num as i32))
        .or(si.session_types.last())
        .map(|(_, l)| l.clone());

    let fp = fnv1a64(yaml.as_bytes(), 0xcbf29ce484222325) ^ (start_date as u64).rotate_left(17) ^ (n as u64).rotate_left(41);
    let wet = si.track_wetness.as_deref().map(|w| !w.to_ascii_lowercase().contains("dry"));
    Ok(ImportedSession {
        meta: SessionMeta {
            sim: SimId::Iracing,
            track: si.track_display_name.clone().unwrap_or_else(|| "Circuit inconnu".into()),
            track_config: si.track_config.clone(),
            track_length_m: si.track_length_m,
            car: si.car_screen_name.clone().unwrap_or_else(|| "Voiture inconnue".into()),
            kind: session_label.as_deref().map(SessionKind::from_label).unwrap_or(SessionKind::Unknown),
            started_at_unix: start_date,
            weather: Weather {
                air_temp_c: si.track_air_temp_c,
                track_temp_c: si.track_surface_temp_c,
                description: si.track_skies.clone(),
                wet,
            },
            source_fingerprint: format!("ibt:{fp:016x}"),
            source_path: None,
        },
        laps,
        sample_rate_hz: (h.tick_rate > 0).then_some(h.tick_rate as f32),
    })
}

/// Découpe en tours sur les franchissements de ligne (LapDistPct 1→0), temps interpolé.
fn split_laps(ch: &Channels, si: &SessionInfo) -> Vec<ImportedLap> {
    let n = ch.session_time.len();
    let pct = &ch.lap_dist_pct;
    let t = &ch.session_time;
    // (index du 1er échantillon du nouveau tour, temps de franchissement interpolé)
    let mut crossings: Vec<(usize, f64)> = Vec::new();
    for i in 1..n {
        if pct[i - 1] > 0.9 && pct[i] < 0.1 && pct[i] >= 0.0 {
            let span = pct[i] + 1.0 - pct[i - 1];
            let frac = if span > 0.0 { (1.0 - pct[i - 1]) / span } else { 0.0 };
            crossings.push((i, t[i - 1] + frac * (t[i] - t[i - 1])));
        }
    }
    let length = si.track_length_m.map(|l| l as f64);
    let mut laps = Vec::new();

    // Portion avant le 1er franchissement : tour de sortie incomplet.
    let mut segments: Vec<(usize, usize, Option<f64>, Option<f64>)> = Vec::new();
    if let Some(&(first, t0)) = crossings.first() {
        if first > 60 {
            segments.push((0, first, None, Some(t0)));
        }
        for w in crossings.windows(2) {
            segments.push((w[0].0, w[1].0, Some(w[0].1), Some(w[1].1)));
        }
        let (last, tl) = *crossings.last().unwrap();
        if n - last > 60 {
            segments.push((last, n, Some(tl), None));
        }
    } else {
        segments.push((0, n, None, None));
    }

    for (seg_i, (a, b, t_start, t_end)) in segments.into_iter().enumerate() {
        let complete = t_start.is_some() && t_end.is_some();
        let base_t = t_start.unwrap_or(t[a]);
        let mut discontinuity = false;
        for i in a + 1..b {
            let dt = t[i] - t[i - 1];
            let dd = ch.lap_dist[i] - ch.lap_dist[i - 1];
            // Saut temporel (pause/replay) ou saut de distance incompatible avec la vitesse (reset/tow).
            if !(0.0..=1.0).contains(&dt) || dd.abs() > (ch.speed[i].abs() * dt.max(0.0) + 50.0) {
                discontinuity = true;
            }
        }
        let any = |v: &Option<Vec<f64>>, f: &dyn Fn(f64) -> bool| v.as_ref().is_some_and(|x| x[a..b].iter().any(|&y| f(y)));
        let pit_start = ch.on_pit_road.as_ref().is_some_and(|p| p[a] > 0.5);
        let half = a + (b - a) / 2;
        let pit_end = ch.on_pit_road.as_ref().is_some_and(|p| p[half..b].iter().any(|&y| y > 0.5));
        let off_track = any(&ch.track_surface, &|s| s == TRK_OFF_TRACK);
        let incident = ch.incidents.as_ref().is_some_and(|x| x[b - 1] > x[a]);
        let yellow = any(&ch.session_flags, &|f| (f as i64 as u32) & YELLOW_MASK != 0);
        let pit = any(&ch.on_pit_road, &|p| p > 0.5);

        let kind = if !complete || discontinuity {
            LapKind::Incomplete
        } else if pit_start || (seg_i == 0 && t_start.is_none()) {
            LapKind::OutLap
        } else if pit_end {
            LapKind::InLap
        } else {
            LapKind::Flying
        };
        let flags = LapFlags { off_track, incident, pit, yellow_flag: yellow, discontinuity };
        let validity = match kind {
            LapKind::Flying if off_track || incident => Validity::Invalid,
            LapKind::Flying => Validity::Valid,
            _ => Validity::Unknown,
        };
        let f = |v: &[f64]| -> Vec<f32> { v[a..b].iter().map(|&x| x as f32).collect() };
        // Distance : LapDist, ou LapDistPct × longueur si LapDist est vide.
        let dist: Vec<f32> = (a..b)
            .map(|i| {
                let d = ch.lap_dist[i];
                match length {
                    Some(len) if d <= 0.0 => (pct[i] * len) as f32,
                    _ => d as f32,
                }
            })
            .collect();
        let (pos_x, pos_y) = match (&ch.lat, &ch.lon) {
            (Some(la), Some(lo)) if la[a..b].iter().any(|v| *v != 0.0) => {
                let (lat0, lon0) = (la[a], lo[a]);
                let k = lat0.to_radians().cos();
                let x = (a..b).map(|i| ((lo[i] - lon0) * 111_320.0 * k) as f32).collect();
                let y = (a..b).map(|i| ((la[i] - lat0) * 110_540.0) as f32).collect();
                (Some(x), Some(y))
            }
            _ => (None, None),
        };
        let fuel = ch.fuel.as_ref().map(|v| f(v));
        let trace = LapTrace {
            time_s: t[a..b].iter().map(|&x| (x - base_t) as f32).collect(),
            dist_m: dist,
            speed_mps: f(&ch.speed),
            throttle: f(&ch.throttle),
            brake: f(&ch.brake),
            gear: ch.gear.as_ref().map(|v| f(v)),
            rpm: ch.rpm.as_ref().map(|v| f(v)),
            steer_rad: ch.steer.as_ref().map(|v| f(v)),
            pos_x,
            pos_y,
            fuel_l: fuel.clone(),
            yaw_rate: ch.yaw_rate.as_ref().map(|v| f(v)),
            extra: Default::default(),
        };
        let number = ch.lap.as_ref().map(|l| l[half] as i32).unwrap_or(seg_i as i32);
        laps.push(ImportedLap {
            meta: LapMeta {
                number,
                kind,
                lap_time_s: if complete { Some(t_end.unwrap() - t_start.unwrap()) } else { None },
                sector_times_s: vec![],
                flags,
                validity,
                validity_source: ValiditySource::Derived,
                fuel_start_l: fuel.as_ref().and_then(|v| v.first().copied()),
                fuel_used_l: fuel.as_ref().and_then(|v| Some(v.first()? - v.last()?)).filter(|u| *u >= 0.0),
            },
            trace,
        });
    }
    laps
}

// ---------------------------------------------------------------------------
// Écriture de fichiers .ibt synthétiques (tests uniquement, conforme à la spec).
// ---------------------------------------------------------------------------
#[cfg(test)]
pub mod writer {
    use super::*;

    pub struct Var<'a> {
        pub name: &'a str,
        pub ty: VarType,
        pub unit: &'a str,
    }

    /// Construit un .ibt : `rows[i][v]` = valeur de la variable v à l'échantillon i.
    pub fn build(vars: &[Var], rows: &[Vec<f64>], yaml: &str, tick_rate: i32, start_date: i64) -> Vec<u8> {
        let mut offsets = Vec::new();
        let mut buf_len = 0usize;
        for v in vars {
            offsets.push(buf_len);
            buf_len += v.ty.size();
        }
        let var_hdr_off = HEADER_LEN + DISK_SUBHEADER_LEN;
        let yaml_off = var_hdr_off + vars.len() * VAR_HEADER_LEN;
        let yaml_bytes = yaml.as_bytes();
        let data_off = yaml_off + yaml_bytes.len() + 1;
        let mut out = vec![0u8; data_off];
        let w = |out: &mut Vec<u8>, o: usize, v: i32| out[o..o + 4].copy_from_slice(&v.to_le_bytes());
        w(&mut out, 0, 2);
        w(&mut out, 8, tick_rate);
        w(&mut out, 16, yaml_bytes.len() as i32);
        w(&mut out, 20, yaml_off as i32);
        w(&mut out, 24, vars.len() as i32);
        w(&mut out, 28, var_hdr_off as i32);
        w(&mut out, 32, 1);
        w(&mut out, 36, buf_len as i32);
        w(&mut out, 48, 0);
        w(&mut out, 52, data_off as i32);
        out[HEADER_LEN..HEADER_LEN + 8].copy_from_slice(&start_date.to_le_bytes());
        w(&mut out, HEADER_LEN + 28, rows.len() as i32);
        for (i, v) in vars.iter().enumerate() {
            let o = var_hdr_off + i * VAR_HEADER_LEN;
            let ty = match v.ty { VarType::Char => 0, VarType::Bool => 1, VarType::Int => 2, VarType::BitField => 3, VarType::Float => 4, VarType::Double => 5 };
            w(&mut out, o, ty);
            w(&mut out, o + 4, offsets[i] as i32);
            w(&mut out, o + 8, 1);
            out[o + 16..o + 16 + v.name.len()].copy_from_slice(v.name.as_bytes());
            out[o + 112..o + 112 + v.unit.len()].copy_from_slice(v.unit.as_bytes());
        }
        out[yaml_off..yaml_off + yaml_bytes.len()].copy_from_slice(yaml_bytes);
        for row in rows {
            let mut rec = vec![0u8; buf_len];
            for (i, v) in vars.iter().enumerate() {
                let o = offsets[i];
                match v.ty {
                    VarType::Char | VarType::Bool => rec[o] = row[i] as u8,
                    VarType::Int | VarType::BitField => rec[o..o + 4].copy_from_slice(&(row[i] as i32).to_le_bytes()),
                    VarType::Float => rec[o..o + 4].copy_from_slice(&(row[i] as f32).to_le_bytes()),
                    VarType::Double => rec[o..o + 8].copy_from_slice(&row[i].to_le_bytes()),
                }
            }
            out.extend_from_slice(&rec);
        }
        out
    }

    pub const YAML: &str = "---\nWeekendInfo:\n TrackDisplayName: Circuit Test\n TrackConfigName: GP\n TrackLength: 2.00 km\n TrackAirTemp: 21.3 C\n TrackSurfaceTemp: 30.1 C\n TrackSkies: Partly Cloudy\n TrackWetness: Dry\nSessionInfo:\n Sessions:\n - SessionNum: 0\n   SessionType: Offline Testing\nDriverInfo:\n DriverCarIdx: 1\n Drivers:\n - CarIdx: 0\n   CarScreenName: Pace Car\n - CarIdx: 1\n   CarScreenName: Ferrari 296 GT3\n...\n";

    /// 3 tours à vitesse constante 50 m/s sur 2 km, 60 Hz, pit au début, hors-piste au tour 2.
    pub fn synthetic() -> Vec<u8> {
        let vars = [
            Var { name: "SessionTime", ty: VarType::Double, unit: "s" },
            Var { name: "Lap", ty: VarType::Int, unit: "" },
            Var { name: "LapDist", ty: VarType::Float, unit: "m" },
            Var { name: "LapDistPct", ty: VarType::Float, unit: "%" },
            Var { name: "Speed", ty: VarType::Float, unit: "m/s" },
            Var { name: "Throttle", ty: VarType::Float, unit: "%" },
            Var { name: "Brake", ty: VarType::Float, unit: "%" },
            Var { name: "Gear", ty: VarType::Int, unit: "" },
            Var { name: "OnPitRoad", ty: VarType::Bool, unit: "" },
            Var { name: "PlayerTrackSurface", ty: VarType::Int, unit: "irsdk_TrkLoc" },
            Var { name: "SessionNum", ty: VarType::Int, unit: "" },
        ];
        let (v, len, hz) = (50.0f64, 2000.0f64, 60.0f64);
        let start_d = 1500.0; // on part à 1500 m (sortie des stands)
        let total = (len - start_d) + 3.0 * len + 300.0;
        let n = (total / v * hz) as usize;
        let rows: Vec<Vec<f64>> = (0..n)
            .map(|i| {
                let t = i as f64 / hz;
                let d_abs = start_d + t * v;
                let lap = (d_abs / len).floor();
                let d = d_abs - lap * len;
                let pit = if d_abs < 1700.0 { 1.0 } else { 0.0 };
                let surf = if lap as i32 == 2 && d > 500.0 && d < 520.0 { 0.0 } else { 3.0 };
                vec![100.0 + t, lap, d, d / len, v, 1.0, 0.0, 4.0, pit, surf, 0.0]
            })
            .collect();
        build(&vars, &rows, YAML, 60, 1_760_000_000)
    }
}

#[cfg(test)]
mod tests {
    use super::writer::*;
    use super::*;


    #[test]
    fn parses_session_info() {
        let si = parse_session_info(YAML);
        assert_eq!(si.track_display_name.as_deref(), Some("Circuit Test"));
        assert_eq!(si.car_screen_name.as_deref(), Some("Ferrari 296 GT3"));
        assert_eq!(si.track_length_m, Some(2000.0));
        assert_eq!(si.session_types, vec![(0, "Offline Testing".to_string())]);
    }

    #[test]
    fn imports_synthetic_ibt() {
        let s = import_ibt_bytes(&synthetic()).unwrap();
        assert_eq!(s.meta.sim, SimId::Iracing);
        assert_eq!(s.meta.track, "Circuit Test");
        assert_eq!(s.meta.car, "Ferrari 296 GT3");
        assert_eq!(s.meta.kind, SessionKind::PrivateTesting);
        assert_eq!(s.meta.weather.wet, Some(false));
        assert_eq!(s.sample_rate_hz, Some(60.0));
        // partiel (sortie stands) + 3 tours complets + partiel final
        let complete: Vec<_> = s.laps.iter().filter(|l| l.meta.lap_time_s.is_some()).collect();
        assert_eq!(complete.len(), 3, "{:#?}", s.laps.iter().map(|l| (&l.meta.kind, l.meta.lap_time_s)).collect::<Vec<_>>());
        for l in &complete {
            // 2000 m à 50 m/s = 40 s, franchissement interpolé
            assert!((l.meta.lap_time_s.unwrap() - 40.0).abs() < 0.02, "{:?}", l.meta.lap_time_s);
        }
        let lap2 = complete.iter().find(|l| l.meta.number == 2).unwrap();
        assert!(lap2.meta.flags.off_track);
        assert_eq!(lap2.meta.validity, Validity::Invalid);
        let lap1 = complete.iter().find(|l| l.meta.number == 1).unwrap();
        assert_eq!(lap1.meta.kind, LapKind::Flying);
        assert_eq!(lap1.meta.validity, Validity::Valid);
        assert!(lap1.trace.gear.is_some());
        assert!(lap1.trace.steer_rad.is_none(), "canal absent du fichier => None");
        for l in &s.laps {
            l.trace.validate().unwrap();
        }
    }

    #[test]
    fn same_file_same_fingerprint() {
        let a = import_ibt_bytes(&synthetic()).unwrap();
        let b = import_ibt_bytes(&synthetic()).unwrap();
        assert_eq!(a.meta.source_fingerprint, b.meta.source_fingerprint);
    }

    #[test]
    fn rejects_garbage() {
        assert!(import_ibt_bytes(&[0u8; 50]).is_err());
        assert!(import_ibt_bytes(&vec![7u8; 4000]).is_err());
    }
}
