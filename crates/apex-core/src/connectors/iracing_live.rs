//! Lecture live de la mémoire partagée iRacing (Windows uniquement).
//!
//! Même en-tête que les `.ibt` (irsdk_header). On choisit le buffer au
//! `tickCount` le plus élevé, on le copie, puis on vérifie que le tick n'a pas
//! changé pendant la copie (sinon on recommence) : lecture cohérente sans verrou.
//!
//! ⚠ Non validé sur une installation réelle : à tester avant de passer en `Implemented`.

use super::{ConnectorError, LiveSnapshot, LiveSource};
use crate::ingest::ibt::{parse_header, parse_var_headers, read_value, VarHeader, HEADER_LEN};
use std::collections::HashMap;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Memory::{MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS};
use windows_sys::Win32::System::Threading::{OpenEventW, WaitForSingleObject};

const SYNCHRONIZE: u32 = 0x0010_0000;
const STATUS_CONNECTED: i32 = 1;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub struct IrsdkLive {
    mapping: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    event: HANDLE,
    vars: HashMap<String, VarHeader>,
    last_tick: i32,
    /// Dernière version du YAML de session transmise (-1 = jamais).
    last_session_info: i32,
}

// Les handles Win32 sont utilisables depuis n'importe quel thread.
unsafe impl Send for IrsdkLive {}

impl IrsdkLive {
    pub fn open() -> Result<Self, ConnectorError> {
        unsafe {
            let mapping = OpenFileMappingW(FILE_MAP_READ, 0, wide("Local\\IRSDKMemMapFileName").as_ptr());
            if mapping.is_null() {
                return Err(ConnectorError::Import("iRacing non lancé (mémoire partagée absente)".into()));
            }
            let view = MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, 0);
            if view.Value.is_null() {
                CloseHandle(mapping);
                return Err(ConnectorError::Import("MapViewOfFile a échoué".into()));
            }
            let event = OpenEventW(SYNCHRONIZE, 0, wide("Local\\IRSDKDataValidEvent").as_ptr());
            Ok(Self { mapping, view, event, vars: HashMap::new(), last_tick: -1, last_session_info: -1 })
        }
    }

    fn base(&self) -> *const u8 {
        self.view.Value as *const u8
    }

    /// Copie `len` octets depuis l'offset `off` de la vue.
    fn copy(&self, off: usize, len: usize) -> Vec<u8> {
        let mut v = vec![0u8; len];
        unsafe { std::ptr::copy_nonoverlapping(self.base().add(off), v.as_mut_ptr(), len) };
        v
    }
}

impl LiveSource for IrsdkLive {
    fn poll(&mut self, timeout_ms: u32) -> Result<Option<LiveSnapshot>, ConnectorError> {
        if !self.event.is_null() {
            let r = unsafe { WaitForSingleObject(self.event, timeout_ms) };
            if r != WAIT_OBJECT_0 {
                return Ok(None);
            }
        }
        let hdr_bytes = self.copy(0, HEADER_LEN);
        let h = parse_header(&hdr_bytes).map_err(|e| ConnectorError::Import(e.to_string()))?;
        if h.status & STATUS_CONNECTED == 0 {
            return Ok(Some(LiveSnapshot { connected: false, on_track: false, session_time_s: 0.0, lap: 0, lap_dist_pct: 0.0, speed_kmh: 0.0, gear: 0, throttle: 0.0, brake: 0.0, tick: 0, disk_logging: None, channels: None, session_info: None }));
        }
        if self.vars.is_empty() {
            let len = h.var_header_offset + h.num_vars * crate::ingest::ibt::VAR_HEADER_LEN;
            let all = self.copy(0, len);
            let vars = parse_var_headers(&all, &h).map_err(|e| ConnectorError::Import(e.to_string()))?;
            self.vars = vars.into_iter().map(|v| (v.name.clone(), v)).collect();
        }
        // Buffer le plus récent, copie cohérente (2 tentatives).
        for _ in 0..2 {
            let (tick, off) = *h.var_bufs.iter().max_by_key(|b| b.0).unwrap();
            if tick == self.last_tick {
                return Ok(None);
            }
            let rec = self.copy(off, h.buf_len);
            let h2 = parse_header(&self.copy(0, HEADER_LEN)).map_err(|e| ConnectorError::Import(e.to_string()))?;
            if h2.var_bufs.iter().any(|b| b.1 == off && b.0 == tick) {
                self.last_tick = tick;
                // YAML de session : relu seulement quand iRacing l'a modifié (il fait ~60 ko).
                let session_info = if h2.session_info_update != self.last_session_info && h2.session_info_len > 0 {
                    self.last_session_info = h2.session_info_update;
                    let raw = self.copy(h2.session_info_offset, h2.session_info_len);
                    let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
                    Some(crate::ingest::ibt::decode_text(&raw[..end]))
                } else {
                    None
                };
                let g = |n: &str| self.vars.get(n).map(|v| read_value(&rec, v)).unwrap_or(0.0);
                let mut channels = [0.0f64; 19];
                for (k, name) in crate::ingest::ibt::LIVE_CHANNELS.iter().enumerate() {
                    channels[k] = g(name);
                }
                return Ok(Some(LiveSnapshot {
                    connected: true,
                    on_track: g("IsOnTrack") > 0.5,
                    session_time_s: g("SessionTime"),
                    lap: g("Lap") as i32,
                    lap_dist_pct: g("LapDistPct") as f32,
                    speed_kmh: g("Speed") as f32 * 3.6,
                    gear: g("Gear") as i32,
                    throttle: g("Throttle") as f32,
                    brake: g("Brake") as f32,
                    tick: tick as i64,
                    disk_logging: self.vars.get("IsDiskLoggingActive").map(|v| read_value(&rec, v) > 0.5),
                    channels: Some(channels),
                    session_info,
                }));
            }
        }
        Ok(None)
    }
}

impl Drop for IrsdkLive {
    fn drop(&mut self) {
        unsafe {
            UnmapViewOfFile(self.view);
            CloseHandle(self.mapping);
            if !self.event.is_null() {
                CloseHandle(self.event);
            }
        }
    }
}
