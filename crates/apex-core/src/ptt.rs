//! Bouton « radio » du volant (push-to-talk) : un bouton de volant/joystick ou une touche clavier,
//! lu en arrière-plan pendant que le simulateur a le focus.
//!
//! Joystick : API winmm (`joyGetPosEx`), qui fonctionne sans focus et voit les 32 premiers boutons
//! de chaque périphérique DirectInput (volant, base, boîtier). Si le bouton voulu est au-delà, il
//! suffit de l'associer à une touche clavier (SimHub, logiciel du volant) : les touches sont lues
//! avec `GetAsyncKeyState`, elles aussi sans focus.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PttBinding {
    /// Bouton `button` (0 = bouton 1) du périphérique de jeu `id`.
    Joystick { id: u32, button: u32, name: String },
    /// Touche clavier (code virtuel Windows).
    Key { vk: u32, name: String },
}

impl PttBinding {
    pub fn label(&self) -> String {
        match self {
            PttBinding::Joystick { button, name, .. } => format!("{name}, bouton {}", button + 1),
            PttBinding::Key { name, .. } => format!("touche {name}"),
        }
    }
}

/// État instantané de toutes les entrées surveillées.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InputSnapshot {
    /// (périphérique, masque des boutons 1 à 32)
    pub joysticks: Vec<(u32, u32)>,
    /// Touches enfoncées (codes virtuels).
    pub keys: Vec<u32>,
}

/// Touches ignorées en apprentissage (souris, modificateurs seuls) : on veut une touche dédiée.
const IGNORED_VK: [u32; 12] = [0x01, 0x02, 0x04, 0x05, 0x06, 0x10, 0x11, 0x12, 0xA0, 0xA1, 0xA2, 0xA3];

/// Nom lisible d'un code de touche virtuelle Windows.
pub fn key_name(vk: u32) -> String {
    match vk {
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0x60..=0x69 => format!("Pavé {}", vk - 0x60),
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk).map(|c| c.to_string()).unwrap_or_default(),
        0x20 => "Espace".into(),
        0x0D => "Entrée".into(),
        0x09 => "Tab".into(),
        0x13 => "Pause".into(),
        0x91 => "Arrêt défil".into(),
        0x2D => "Insert".into(),
        0x24 => "Début".into(),
        0x21 => "Page préc.".into(),
        0x22 => "Page suiv.".into(),
        0x6A => "Pavé *".into(),
        0x6B => "Pavé +".into(),
        0x6D => "Pavé -".into(),
        0x6F => "Pavé /".into(),
        v => format!("code {v}"),
    }
}

/// Première entrée nouvellement enfoncée entre deux instantanés (apprentissage du bouton).
pub fn newly_pressed(prev: &InputSnapshot, now: &InputSnapshot, joy_name: impl Fn(u32) -> String) -> Option<PttBinding> {
    for (id, mask) in &now.joysticks {
        let before = prev.joysticks.iter().find(|(i, _)| i == id).map(|(_, m)| *m).unwrap_or(*mask);
        let pressed = mask & !before;
        if pressed != 0 {
            let button = pressed.trailing_zeros();
            return Some(PttBinding::Joystick { id: *id, button, name: joy_name(*id) });
        }
    }
    now.keys.iter().find(|k| !prev.keys.contains(k) && !IGNORED_VK.contains(k)).map(|&vk| PttBinding::Key { vk, name: key_name(vk) })
}

/// La liaison est-elle enfoncée dans cet instantané ?
pub fn is_down(b: &PttBinding, s: &InputSnapshot) -> bool {
    match b {
        PttBinding::Joystick { id, button, .. } => s.joysticks.iter().any(|(i, m)| i == id && m & (1 << button) != 0),
        PttBinding::Key { vk, .. } => s.keys.contains(vk),
    }
}

#[cfg(windows)]
mod sys {
    use super::InputSnapshot;
    use windows_sys::Win32::Media::Multimedia::{joyGetDevCapsW, joyGetNumDevs, joyGetPosEx, JOYCAPSW, JOYERR_NOERROR, JOYINFOEX, JOY_RETURNBUTTONS};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

    pub fn snapshot(with_keys: bool) -> InputSnapshot {
        let mut s = InputSnapshot::default();
        let n = unsafe { joyGetNumDevs() }.min(16);
        for id in 0..n {
            let mut ji: JOYINFOEX = unsafe { std::mem::zeroed() };
            ji.dwSize = std::mem::size_of::<JOYINFOEX>() as u32;
            ji.dwFlags = JOY_RETURNBUTTONS as u32;
            if unsafe { joyGetPosEx(id, &mut ji) } == JOYERR_NOERROR {
                s.joysticks.push((id, ji.dwButtons));
            }
        }
        if with_keys {
            for vk in 0x08u32..0xFF {
                if unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000 != 0 {
                    s.keys.push(vk);
                }
            }
        }
        s
    }

    pub fn key_down(vk: u32) -> bool {
        unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
    }

    /// Boutons d'un seul périphérique (lire un identifiant absent est lent avec winmm : on évite de tout balayer).
    pub fn joy_buttons(id: u32) -> Option<u32> {
        let mut ji: JOYINFOEX = unsafe { std::mem::zeroed() };
        ji.dwSize = std::mem::size_of::<JOYINFOEX>() as u32;
        ji.dwFlags = JOY_RETURNBUTTONS as u32;
        (unsafe { joyGetPosEx(id, &mut ji) } == JOYERR_NOERROR).then_some(ji.dwButtons)
    }

    pub fn joystick_name(id: u32) -> String {
        let mut caps: JOYCAPSW = unsafe { std::mem::zeroed() };
        if unsafe { joyGetDevCapsW(id as usize, &mut caps, std::mem::size_of::<JOYCAPSW>() as u32) } == JOYERR_NOERROR {
            let name = caps.szPname;
            let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
            let s = String::from_utf16_lossy(&name[..end]);
            if !s.trim().is_empty() {
                return s.trim().to_string();
            }
        }
        format!("Périphérique {}", id + 1)
    }
}

#[cfg(not(windows))]
mod sys {
    use super::InputSnapshot;
    pub fn snapshot(_with_keys: bool) -> InputSnapshot {
        InputSnapshot::default()
    }
    pub fn key_down(_vk: u32) -> bool {
        false
    }
    pub fn joy_buttons(_id: u32) -> Option<u32> {
        None
    }
    pub fn joystick_name(id: u32) -> String {
        format!("Périphérique {}", id + 1)
    }
}

/// Instantané des entrées (clavier compris seulement si demandé : 250 lectures).
pub fn snapshot(with_keys: bool) -> InputSnapshot {
    sys::snapshot(with_keys)
}

pub fn joystick_name(id: u32) -> String {
    sys::joystick_name(id)
}

/// Lecture rapide de la seule liaison configurée.
pub fn binding_down(b: &PttBinding) -> bool {
    match b {
        PttBinding::Key { vk, .. } => sys::key_down(*vk),
        PttBinding::Joystick { id, button, .. } => sys::joy_buttons(*id).is_some_and(|m| m & (1 << button) != 0),
    }
}

/// Attend qu'une entrée soit enfoncée (apprentissage), au plus `timeout`.
pub fn learn(timeout: std::time::Duration) -> Option<PttBinding> {
    let until = std::time::Instant::now() + timeout;
    // Entrées déjà enfoncées au départ ignorées : on attend un nouvel appui.
    let mut prev = snapshot(true);
    while std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(15));
        let now = snapshot(true);
        if let Some(b) = newly_pressed(&prev, &now, joystick_name) {
            return Some(b);
        }
        prev = now;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learns_the_new_button_and_reads_it() {
        let prev = InputSnapshot { joysticks: vec![(0, 0b0001), (1, 0)], keys: vec![] };
        let now = InputSnapshot { joysticks: vec![(0, 0b0001), (1, 1 << 7)], keys: vec![] };
        let b = newly_pressed(&prev, &now, |i| format!("Simagic {i}")).unwrap();
        assert_eq!(b, PttBinding::Joystick { id: 1, button: 7, name: "Simagic 1".into() });
        assert_eq!(b.label(), "Simagic 1, bouton 8");
        assert!(is_down(&b, &now) && !is_down(&b, &prev));
        // Bouton déjà maintenu au départ : pas un nouvel appui.
        assert!(newly_pressed(&now, &now, |_| String::new()).is_none());
        // Touche clavier (F13 envoyée par SimHub par exemple) ; Maj seule ignorée.
        let k = newly_pressed(&InputSnapshot::default(), &InputSnapshot { joysticks: vec![], keys: vec![0x10, 0x7C] }, |_| String::new()).unwrap();
        assert_eq!(k, PttBinding::Key { vk: 0x7C, name: "F13".into() });
        let json = serde_json::to_string(&k).unwrap();
        assert_eq!(serde_json::from_str::<PttBinding>(&json).unwrap(), k);
    }
}
