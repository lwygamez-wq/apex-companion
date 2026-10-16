//! Sérialisation binaire colonnaire d'un `LapTrace`, compressée en LZ4.
//!
//! Format (avant compression) :
//! `b"APXT"` | version u16 | n_samples u32 | n_channels u16 |
//! pour chaque canal : name_len u8 | name utf8 | n_samples × f32 LE
//!
//! Un tour de 5 km à 60 Hz ≈ 6 000 échantillons × ~12 canaux ≈ 290 Ko brut, ~100 Ko compressé.
//! Une ligne SQLite par échantillon aurait été 100× plus lente pour les comparaisons.

use crate::model::LapTrace;
use std::collections::BTreeMap;

const MAGIC: &[u8; 4] = b"APXT";
const VERSION: u16 = 1;
pub const CODEC_ID: i64 = 1;

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("données de trace corrompues : {0}")]
    Corrupt(String),
    #[error("décompression LZ4 : {0}")]
    Lz4(#[from] lz4_flex::block::DecompressError),
}

fn channels(t: &LapTrace) -> Vec<(&str, &[f32])> {
    let mut v: Vec<(&str, &[f32])> = vec![
        ("time_s", &t.time_s),
        ("dist_m", &t.dist_m),
        ("speed_mps", &t.speed_mps),
        ("throttle", &t.throttle),
        ("brake", &t.brake),
    ];
    let opt = [
        ("gear", &t.gear),
        ("rpm", &t.rpm),
        ("steer_rad", &t.steer_rad),
        ("pos_x", &t.pos_x),
        ("pos_y", &t.pos_y),
        ("fuel_l", &t.fuel_l),
        ("yaw_rate", &t.yaw_rate),
    ];
    for (name, ch) in opt {
        if let Some(c) = ch {
            v.push((name, c.as_slice()));
        }
    }
    for (k, c) in &t.extra {
        v.push((k.as_str(), c.as_slice()));
    }
    v
}

pub fn encode(t: &LapTrace) -> Vec<u8> {
    let n = t.len();
    let chans = channels(t);
    let mut raw = Vec::with_capacity(12 + chans.len() * (n * 4 + 16));
    raw.extend_from_slice(MAGIC);
    raw.extend_from_slice(&VERSION.to_le_bytes());
    raw.extend_from_slice(&(n as u32).to_le_bytes());
    raw.extend_from_slice(&(chans.len() as u16).to_le_bytes());
    for (name, data) in chans {
        let nb = name.as_bytes();
        raw.push(nb.len().min(255) as u8);
        raw.extend_from_slice(&nb[..nb.len().min(255)]);
        for i in 0..n {
            let v = data.get(i).copied().unwrap_or(f32::NAN);
            raw.extend_from_slice(&v.to_le_bytes());
        }
    }
    lz4_flex::compress_prepend_size(&raw)
}

pub fn decode(bytes: &[u8]) -> Result<LapTrace, CodecError> {
    let raw = lz4_flex::decompress_size_prepended(bytes)?;
    let mut p = 0usize;
    let take = |p: &mut usize, k: usize| -> Result<&[u8], CodecError> {
        let s = raw
            .get(*p..*p + k)
            .ok_or_else(|| CodecError::Corrupt(format!("fin inattendue à l'octet {}", *p)))?;
        *p += k;
        Ok(s)
    };
    if take(&mut p, 4)? != MAGIC {
        return Err(CodecError::Corrupt("signature invalide".into()));
    }
    let ver = u16::from_le_bytes(take(&mut p, 2)?.try_into().unwrap());
    if ver != VERSION {
        return Err(CodecError::Corrupt(format!("version {ver} non supportée")));
    }
    let n = u32::from_le_bytes(take(&mut p, 4)?.try_into().unwrap()) as usize;
    let nch = u16::from_le_bytes(take(&mut p, 2)?.try_into().unwrap()) as usize;
    let mut map: BTreeMap<String, Vec<f32>> = BTreeMap::new();
    for _ in 0..nch {
        let len = take(&mut p, 1)?[0] as usize;
        let name = String::from_utf8(take(&mut p, len)?.to_vec())
            .map_err(|_| CodecError::Corrupt("nom de canal non UTF-8".into()))?;
        let body = take(&mut p, n * 4)?;
        let data = body
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        map.insert(name, data);
    }
    let mut req = |k: &str| {
        map.remove(k)
            .ok_or_else(|| CodecError::Corrupt(format!("canal requis manquant : {k}")))
    };
    let mut t = LapTrace {
        time_s: req("time_s")?,
        dist_m: req("dist_m")?,
        speed_mps: req("speed_mps")?,
        throttle: req("throttle")?,
        brake: req("brake")?,
        ..Default::default()
    };
    t.gear = map.remove("gear");
    t.rpm = map.remove("rpm");
    t.steer_rad = map.remove("steer_rad");
    t.pos_x = map.remove("pos_x");
    t.pos_y = map.remove("pos_y");
    t.fuel_l = map.remove("fuel_l");
    t.yaw_rate = map.remove("yaw_rate");
    t.extra = map;
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_preserves_optional_channels() {
        let mut t = LapTrace {
            time_s: vec![0.0, 0.5, 1.0],
            dist_m: vec![0.0, 20.0, 40.0],
            speed_mps: vec![40.0, 41.0, 42.0],
            throttle: vec![1.0, 1.0, 0.8],
            brake: vec![0.0, 0.0, 0.2],
            steer_rad: Some(vec![0.0, 0.1, 0.2]),
            ..Default::default()
        };
        t.extra.insert("tyre_temp_fl_c".into(), vec![80.0, 81.0, 82.0]);
        let back = decode(&encode(&t)).unwrap();
        assert_eq!(back, t);
        assert!(back.rpm.is_none(), "un canal absent doit rester absent");
    }

    #[test]
    fn corrupt_data_is_rejected() {
        assert!(decode(&[1, 2, 3]).is_err());
    }
}
