//! Bouton « radio » du volant (push-to-talk), lu en arrière-plan pendant que le simulateur a le focus.
//!
//! Trois sources :
//! - **HID** (volants, bases, boîtiers à boutons) : lecture directe des rapports HID, tous les
//!   boutons (un volant Simagic en déclare souvent plus de 32), sans focus. Source principale.
//! - **winmm** (`joyGetPosEx`) : ancienne API de joystick, 32 premiers boutons ; secours.
//! - **clavier** (`GetAsyncKeyState`) : pour un bouton associé à une touche (SimHub, logiciel du volant).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PttBinding {
    /// Bouton HID (`button` = numéro de bouton déclaré par l'appareil, à partir de 1).
    Hid { vid: u16, pid: u16, button: u16, name: String },
    /// Bouton `button` (0 = bouton 1) du joystick winmm `id`.
    Joystick { id: u32, button: u32, name: String },
    /// Touche clavier (code virtuel Windows).
    Key { vk: u32, name: String },
}

impl PttBinding {
    pub fn label(&self) -> String {
        match self {
            PttBinding::Hid { button, name, .. } => format!("{name}, bouton {button}"),
            PttBinding::Joystick { button, name, .. } => format!("{name}, bouton {}", button + 1),
            PttBinding::Key { name, .. } => format!("touche {name}"),
        }
    }
}

/// État instantané de toutes les entrées surveillées.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InputSnapshot {
    /// Appareils HID : (vid, pid, nom, boutons enfoncés).
    pub hid: Vec<(u16, u16, String, Vec<u16>)>,
    /// (joystick winmm, masque des boutons 1 à 32)
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
    for (vid, pid, name, buttons) in &now.hid {
        let before: &[u16] = prev.hid.iter().find(|d| d.0 == *vid && d.1 == *pid).map(|d| d.3.as_slice()).unwrap_or(buttons);
        if let Some(&b) = buttons.iter().find(|b| !before.contains(b)) {
            return Some(PttBinding::Hid { vid: *vid, pid: *pid, button: b, name: name.clone() });
        }
    }
    for (id, mask) in &now.joysticks {
        let before = prev.joysticks.iter().find(|(i, _)| i == id).map(|(_, m)| *m).unwrap_or(*mask);
        let pressed = mask & !before;
        if pressed != 0 {
            return Some(PttBinding::Joystick { id: *id, button: pressed.trailing_zeros(), name: joy_name(*id) });
        }
    }
    now.keys.iter().find(|k| !prev.keys.contains(k) && !IGNORED_VK.contains(k)).map(|&vk| PttBinding::Key { vk, name: key_name(vk) })
}

/// La liaison est-elle enfoncée dans cet instantané ?
pub fn is_down(b: &PttBinding, s: &InputSnapshot) -> bool {
    match b {
        PttBinding::Hid { vid, pid, button, .. } => s.hid.iter().any(|d| d.0 == *vid && d.1 == *pid && d.3.contains(button)),
        PttBinding::Joystick { id, button, .. } => s.joysticks.iter().any(|(i, m)| i == id && m & (1 << button) != 0),
        PttBinding::Key { vk, .. } => s.keys.contains(vk),
    }
}

#[cfg(windows)]
mod sys {
    use super::InputSnapshot;
    use std::collections::{BTreeMap, HashMap, HashSet};
    use std::sync::{Arc, Mutex, OnceLock};
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
        SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
    };
    use windows_sys::Win32::Devices::HumanInterfaceDevice::{
        HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetHidGuid, HidD_GetPreparsedData, HidD_GetProductString, HidP_GetCaps, HidP_GetUsages, HidP_Input,
        HidP_MaxUsageListLength, HIDD_ATTRIBUTES, HIDP_CAPS, HIDP_STATUS_SUCCESS,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Media::Multimedia::{joyGetDevCapsW, joyGetNumDevs, joyGetPosEx, JOYCAPSW, JOYERR_NOERROR, JOYINFOEX, JOY_RETURNBUTTONS};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, ReadFile, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

    /// Page d'usage HID des boutons.
    const USAGE_PAGE_BUTTON: u16 = 0x09;

    /// Appareil HID ouvert : boutons enfoncés par numéro de rapport (un appareil peut répartir ses
    /// boutons sur plusieurs rapports).
    struct HidDev {
        vid: u16,
        pid: u16,
        name: String,
        pressed: BTreeMap<u8, Vec<u16>>,
    }

    #[derive(Default)]
    struct HidState {
        devices: HashMap<String, HidDev>,
    }

    fn state() -> &'static Arc<Mutex<HidState>> {
        static S: OnceLock<Arc<Mutex<HidState>>> = OnceLock::new();
        S.get_or_init(|| {
            let s = Arc::new(Mutex::new(HidState::default()));
            let s2 = s.clone();
            // Recherche des appareils toutes les 3 s (branchement à chaud : volant AirLink allumé plus tard).
            let _ = std::thread::Builder::new().name("apex-hid-scan".into()).spawn(move || loop {
                for path in hid_game_devices() {
                    let known = s2.lock().map(|g| g.devices.contains_key(&path)).unwrap_or(true);
                    if !known {
                        open_device(&s2, path);
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(3));
            });
            s
        })
    }

    fn wide(p: &str) -> Vec<u16> {
        p.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Chemins des interfaces HID présentes.
    fn hid_game_devices() -> Vec<String> {
        let mut out = vec![];
        unsafe {
            let mut guid = std::mem::zeroed();
            HidD_GetHidGuid(&mut guid);
            let set = SetupDiGetClassDevsW(&guid, std::ptr::null(), std::ptr::null_mut(), DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
            if set == -1 || set == 0 {
                return out;
            }
            for i in 0..256u32 {
                let mut ifd: SP_DEVICE_INTERFACE_DATA = std::mem::zeroed();
                ifd.cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
                if SetupDiEnumDeviceInterfaces(set, std::ptr::null(), &guid, i, &mut ifd) == 0 {
                    break;
                }
                let mut need = 0u32;
                SetupDiGetDeviceInterfaceDetailW(set, &ifd, std::ptr::null_mut(), 0, &mut need, std::ptr::null_mut());
                if need == 0 {
                    continue;
                }
                let mut buf = vec![0u32; (need as usize).div_ceil(4) + 1];
                let detail = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
                (*detail).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
                if SetupDiGetDeviceInterfaceDetailW(set, &ifd, detail, need, std::ptr::null_mut(), std::ptr::null_mut()) == 0 {
                    continue;
                }
                let p = std::ptr::addr_of!((*detail).DevicePath) as *const u16;
                let mut len = 0;
                while *p.add(len) != 0 {
                    len += 1;
                }
                out.push(String::from_utf16_lossy(std::slice::from_raw_parts(p, len)));
            }
            SetupDiDestroyDeviceInfoList(set);
        }
        out
    }

    /// Ouvre un appareil s'il s'agit d'un contrôleur de jeu (joystick, manette, multi-axes) et lance sa lecture.
    fn open_device(st: &Arc<Mutex<HidState>>, path: String) {
        unsafe {
            let h: HANDLE = CreateFileW(wide(&path).as_ptr(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, std::ptr::null(), OPEN_EXISTING, 0, std::ptr::null_mut());
            // Appareil inaccessible (clavier, souris, ouverts en exclusivité par Windows) : on le mémorise pour ne pas réessayer.
            let placeholder = |st: &Arc<Mutex<HidState>>| {
                if let Ok(mut g) = st.lock() {
                    g.devices.insert(path.clone(), HidDev { vid: 0, pid: 0, name: String::new(), pressed: BTreeMap::new() });
                }
            };
            if h == INVALID_HANDLE_VALUE || h.is_null() {
                placeholder(st);
                return;
            }
            let mut pp = 0isize;
            if HidD_GetPreparsedData(h, &mut pp) == 0 {
                CloseHandle(h);
                placeholder(st);
                return;
            }
            let mut caps: HIDP_CAPS = std::mem::zeroed();
            HidP_GetCaps(pp, &mut caps);
            let is_game = caps.UsagePage == 0x01 && matches!(caps.Usage, 0x04 | 0x05 | 0x08);
            if !is_game || caps.InputReportByteLength == 0 {
                HidD_FreePreparsedData(pp);
                CloseHandle(h);
                placeholder(st);
                return;
            }
            let mut attr: HIDD_ATTRIBUTES = std::mem::zeroed();
            attr.Size = std::mem::size_of::<HIDD_ATTRIBUTES>() as u32;
            HidD_GetAttributes(h, &mut attr);
            let mut name_buf = [0u16; 128];
            let name = if HidD_GetProductString(h, name_buf.as_mut_ptr() as *mut _, (name_buf.len() * 2) as u32) != 0 {
                let end = name_buf.iter().position(|&c| c == 0).unwrap_or(name_buf.len());
                String::from_utf16_lossy(&name_buf[..end]).trim().to_string()
            } else {
                String::new()
            };
            let name = if name.is_empty() { format!("Appareil {:04X}:{:04X}", attr.VendorID, attr.ProductID) } else { name };
            if let Ok(mut g) = st.lock() {
                g.devices.insert(path.clone(), HidDev { vid: attr.VendorID, pid: attr.ProductID, name, pressed: BTreeMap::new() });
            }
            let report_len = caps.InputReportByteLength as usize;
            let max_usages = HidP_MaxUsageListLength(HidP_Input, USAGE_PAGE_BUTTON, pp).max(1) as usize;
            let st2 = st.clone();
            let h_raw = h as usize;
            let _ = std::thread::Builder::new().name("apex-hid".into()).spawn(move || {
                let h = h_raw as HANDLE;
                let mut report = vec![0u8; report_len];
                let mut usages = vec![0u16; max_usages];
                loop {
                    let mut read = 0u32;
                    // Lecture bloquante : l'appareil envoie un rapport à chaque changement d'état.
                    if ReadFile(h, report.as_mut_ptr(), report_len as u32, &mut read, std::ptr::null_mut()) == 0 {
                        break;
                    }
                    let mut n = max_usages as u32;
                    let st = HidP_GetUsages(HidP_Input, USAGE_PAGE_BUTTON, 0, usages.as_mut_ptr(), &mut n, pp, report.as_mut_ptr(), read);
                    let pressed: Vec<u16> = if st == HIDP_STATUS_SUCCESS { usages[..n as usize].to_vec() } else { continue };
                    if let Ok(mut g) = st2.lock() {
                        if let Some(d) = g.devices.get_mut(&path) {
                            d.pressed.insert(report[0], pressed);
                        }
                    }
                }
                // Appareil débranché : retiré, il sera rouvert au prochain balayage.
                if let Ok(mut g) = st2.lock() {
                    g.devices.remove(&path);
                }
                HidD_FreePreparsedData(pp);
                CloseHandle(h);
            });
        }
    }

    pub fn hid_snapshot() -> Vec<(u16, u16, String, Vec<u16>)> {
        let st = state();
        let g = match st.lock() {
            Ok(g) => g,
            Err(_) => return vec![],
        };
        let mut seen = HashSet::new();
        let mut out: Vec<(u16, u16, String, Vec<u16>)> = vec![];
        for d in g.devices.values().filter(|d| d.vid != 0 || d.pid != 0) {
            let mut b: Vec<u16> = d.pressed.values().flatten().copied().collect();
            b.sort_unstable();
            b.dedup();
            // Un même appareil peut exposer plusieurs interfaces : on les fusionne.
            if seen.insert((d.vid, d.pid)) {
                out.push((d.vid, d.pid, d.name.clone(), b));
            } else if let Some(e) = out.iter_mut().find(|e| e.0 == d.vid && e.1 == d.pid) {
                e.3.extend(b);
                e.3.sort_unstable();
                e.3.dedup();
            }
        }
        out
    }

    pub fn snapshot(with_keys: bool) -> InputSnapshot {
        let mut s = InputSnapshot { hid: hid_snapshot(), ..Default::default() };
        let n = unsafe { joyGetNumDevs() }.min(16);
        for id in 0..n {
            if let Some(m) = joy_buttons(id) {
                s.joysticks.push((id, m));
            }
        }
        if with_keys {
            for vk in 0x08u32..0xFF {
                if key_down(vk) {
                    s.keys.push(vk);
                }
            }
        }
        s
    }

    pub fn key_down(vk: u32) -> bool {
        unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
    }

    /// Boutons d'un seul joystick winmm (lire un identifiant absent est lent : on évite de tout balayer).
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
    pub fn hid_snapshot() -> Vec<(u16, u16, String, Vec<u16>)> {
        vec![]
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

/// Appareils de jeu détectés (diagnostic) : « nom (boutons enfoncés) ».
pub fn devices() -> Vec<String> {
    let mut v: Vec<String> = sys::hid_snapshot().into_iter().map(|(vid, pid, name, _)| format!("{name} ({vid:04X}:{pid:04X})")).collect();
    v.sort();
    v
}

/// Lecture rapide de la seule liaison configurée.
pub fn binding_down(b: &PttBinding) -> bool {
    match b {
        PttBinding::Key { vk, .. } => sys::key_down(*vk),
        PttBinding::Joystick { id, button, .. } => sys::joy_buttons(*id).is_some_and(|m| m & (1 << button) != 0),
        PttBinding::Hid { vid, pid, button, .. } => sys::hid_snapshot().iter().any(|d| d.0 == *vid && d.1 == *pid && d.3.contains(button)),
    }
}

/// Attend qu'une entrée soit enfoncée (apprentissage), au plus `timeout`.
pub fn learn(timeout: std::time::Duration) -> Option<PttBinding> {
    let until = std::time::Instant::now() + timeout;
    // Premier balayage HID : laisse le temps d'ouvrir les appareils.
    let _ = snapshot(false);
    std::thread::sleep(std::time::Duration::from_millis(400));
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
        // Bouton HID au-delà de 32 (cas des volants Simagic) : vu par la lecture HID.
        let prev = InputSnapshot { hid: vec![(0x0483, 0x0522, "Simagic Alpha EVO".into(), vec![3])], ..Default::default() };
        let now = InputSnapshot { hid: vec![(0x0483, 0x0522, "Simagic Alpha EVO".into(), vec![3, 47])], ..Default::default() };
        let b = newly_pressed(&prev, &now, |_| String::new()).unwrap();
        assert_eq!(b, PttBinding::Hid { vid: 0x0483, pid: 0x0522, button: 47, name: "Simagic Alpha EVO".into() });
        assert_eq!(b.label(), "Simagic Alpha EVO, bouton 47");
        assert!(is_down(&b, &now) && !is_down(&b, &prev));
        assert!(newly_pressed(&now, &now, |_| String::new()).is_none(), "bouton déjà maintenu : pas un nouvel appui");
        // winmm (secours).
        let prev = InputSnapshot { joysticks: vec![(0, 0b0001), (1, 0)], ..Default::default() };
        let now = InputSnapshot { joysticks: vec![(0, 0b0001), (1, 1 << 7)], ..Default::default() };
        let j = newly_pressed(&prev, &now, |i| format!("Joy {i}")).unwrap();
        assert_eq!(j, PttBinding::Joystick { id: 1, button: 7, name: "Joy 1".into() });
        // Touche clavier (F13 envoyée par SimHub par exemple) ; Maj seule ignorée.
        let k = newly_pressed(&InputSnapshot::default(), &InputSnapshot { keys: vec![0x10, 0x7C], ..Default::default() }, |_| String::new()).unwrap();
        assert_eq!(k, PttBinding::Key { vk: 0x7C, name: "F13".into() });
        for x in [b, j, k] {
            let json = serde_json::to_string(&x).unwrap();
            assert_eq!(serde_json::from_str::<PttBinding>(&json).unwrap(), x);
        }
    }

    /// Sous Windows : l'énumération HID, winmm et clavier ne plante pas, même sans aucun volant (CI).
    #[cfg(windows)]
    #[test]
    fn windows_enumeration_is_safe() {
        let _ = snapshot(true);
        std::thread::sleep(std::time::Duration::from_millis(600));
        let s = snapshot(true);
        let _ = devices();
        let b = PttBinding::Hid { vid: 1, pid: 2, button: 3, name: "x".into() };
        assert!(!binding_down(&b));
        assert!(s.hid.iter().all(|d| d.0 != 0 || d.1 != 0));
    }
}
