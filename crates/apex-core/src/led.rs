//! Repères de freinage pour les LEDs du volant.
//!
//! Apex connaît les points de freinage de la référence (tour d'un pilote plus rapide, ou ton meilleur
//! tour). Pendant que tu roules, il calcule à chaque échantillon le temps qui te sépare du prochain
//! point de freinage et l'envoie, en local (UDP 127.0.0.1), au plugin SimHub d'Apex. Le plugin en fait
//! des propriétés SimHub (`ApexCompanion.BrakeProgress`, `ApexCompanion.BrakeNow`…) qu'un profil LED
//! utilise : barre qui se remplit à l'approche, flash au point de freinage.
//!
//! Format du datagramme (texte ASCII, séparateur « ; ») :
//! `APEX1;<progression 0..1>;<freine maintenant 0|1>;<secondes avant le point>;<écart de vitesse km/h>`

use crate::model::LapTrace;
use std::net::UdpSocket;

pub const DEFAULT_PORT: u16 = 47823;
/// Durée de l'approche affichée (s) : la barre commence à se remplir 1,5 s avant le point de freinage.
pub const APPROACH_S: f32 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrakePoint {
    pub dist_m: f32,
    /// Vitesse de la référence au point de freinage (m/s).
    pub speed_mps: f32,
}

/// Points de freinage d'un tour : la pédale dépasse 15 % après au moins 40 m sans freiner.
pub fn brake_points(t: &LapTrace) -> Vec<BrakePoint> {
    let mut out = vec![];
    let mut last_release: Option<f32> = Some(f32::NEG_INFINITY);
    for i in 0..t.len() {
        let (b, d) = (t.brake[i], t.dist_m[i]);
        if b < 0.05 {
            last_release.get_or_insert(d);
        } else if b >= 0.15 {
            if let Some(r) = last_release {
                if d - r >= 40.0 || r == f32::NEG_INFINITY {
                    out.push(BrakePoint { dist_m: d, speed_mps: t.speed_mps[i] });
                }
            }
            last_release = None;
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LedCue {
    /// 0 loin du point de freinage, 1 au point de freinage.
    pub progress: f32,
    /// Au point de freinage (et 0,3 s après) : flash.
    pub brake_now: bool,
    /// Secondes avant le prochain point de freinage (99 s'il n'y en a pas devant).
    pub seconds: f32,
    /// Ta vitesse moins celle de la référence au point de freinage (km/h) : positive = tu arrives plus vite.
    pub speed_gap_kmh: f32,
}

/// Repère à l'instant : `dist` (m depuis la ligne), `speed` (m/s), `len` = longueur du circuit.
pub fn cue(points: &[BrakePoint], len: f32, dist: f32, speed: f32) -> LedCue {
    let none = LedCue { seconds: 99.0, ..Default::default() };
    if points.is_empty() || len <= 0.0 || speed < 5.0 {
        return none;
    }
    // Point juste passé (moins de 0,3 s) : on reste en « freine ».
    let ahead = |p: &BrakePoint| (p.dist_m - dist).rem_euclid(len);
    let behind = |p: &BrakePoint| (dist - p.dist_m).rem_euclid(len);
    if let Some(p) = points.iter().find(|p| behind(p) <= speed * 0.3) {
        return LedCue { progress: 1.0, brake_now: true, seconds: 0.0, speed_gap_kmh: (speed - p.speed_mps) * 3.6 };
    }
    let Some(p) = points.iter().min_by(|a, b| ahead(a).total_cmp(&ahead(b))) else { return none };
    let s = ahead(p) / speed;
    LedCue {
        progress: (1.0 - s / APPROACH_S).clamp(0.0, 1.0),
        brake_now: s <= 0.05,
        seconds: s.min(99.0),
        speed_gap_kmh: (speed - p.speed_mps) * 3.6,
    }
}

pub fn encode(c: &LedCue) -> String {
    format!("APEX1;{:.3};{};{:.2};{:.1}", c.progress, c.brake_now as u8, c.seconds, c.speed_gap_kmh)
}

/// Envoi au plugin SimHub (local uniquement).
pub struct LedSender {
    sock: UdpSocket,
    port: u16,
}

impl LedSender {
    pub fn new(port: u16) -> std::io::Result<Self> {
        let sock = UdpSocket::bind("127.0.0.1:0")?;
        sock.set_nonblocking(true)?;
        Ok(Self { sock, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Envoi sans attente : plugin absent = datagramme perdu, sans conséquence.
    pub fn send(&self, c: &LedCue) {
        let _ = self.sock.send_to(encode(c).as_bytes(), ("127.0.0.1", self.port));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tour de 2 km : freinages à 500 m et 1500 m.
    fn lap() -> LapTrace {
        let n = 2000;
        let dist: Vec<f32> = (0..n).map(|i| i as f32).collect();
        let brake: Vec<f32> = dist.iter().map(|d| if (500.0..560.0).contains(d) || (1500.0..1540.0).contains(d) { 0.8 } else { 0.0 }).collect();
        LapTrace { time_s: dist.iter().map(|d| d / 50.0).collect(), speed_mps: vec![50.0; n], throttle: vec![1.0; n], brake, dist_m: dist, ..Default::default() }
    }

    #[test]
    fn finds_each_braking_point_once() {
        let p = brake_points(&lap());
        assert_eq!(p.iter().map(|p| p.dist_m).collect::<Vec<_>>(), vec![500.0, 1500.0]);
    }

    #[test]
    fn cue_fills_up_then_flashes() {
        let p = brake_points(&lap());
        // 1,5 s à 50 m/s = 75 m avant le point : la barre démarre.
        assert_eq!(cue(&p, 2000.0, 400.0, 50.0).progress, 0.0);
        let mid = cue(&p, 2000.0, 462.5, 50.0);
        assert!((mid.progress - 0.5).abs() < 0.01, "{mid:?}");
        assert!(!mid.brake_now);
        let at = cue(&p, 2000.0, 505.0, 50.0);
        assert!(at.brake_now && at.progress == 1.0);
        // Fin de tour : le prochain point est après la ligne (500 m du tour suivant).
        let wrap = cue(&p, 2000.0, 1990.0, 50.0);
        assert!((wrap.seconds - 510.0 / 50.0).abs() < 0.01, "{wrap:?}");
        // Arrivée plus rapide que la référence.
        assert!((cue(&p, 2000.0, 470.0, 55.0).speed_gap_kmh - 18.0).abs() < 0.1);
        assert_eq!(encode(&at), "APEX1;1.000;1;0.00;0.0");
    }

    #[test]
    fn sender_reaches_a_local_listener() {
        let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
        rx.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
        let s = LedSender::new(rx.local_addr().unwrap().port()).unwrap();
        s.send(&LedCue { progress: 0.5, brake_now: false, seconds: 0.75, speed_gap_kmh: -3.0 });
        let mut buf = [0u8; 128];
        let n = rx.recv(&mut buf).unwrap();
        assert_eq!(std::str::from_utf8(&buf[..n]).unwrap(), "APEX1;0.500;0;0.75;-3.0");
    }
}
