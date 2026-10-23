//! Commandes vocales du coach : reconnaissance hors ligne (Vosk, Apache-2.0) limitée à quelques
//! mots-clés. Restreindre le vocabulaire (« grammaire ») rend la reconnaissance fiable malgré le
//! bruit moteur et un micro de casque, et rapide (quelques dizaines de ms pour une seconde d'audio).
//!
//! Bibliothèque : github.com/alphacep/vosk-api (v0.3.45, Windows 64 bits).
//! Modèle : vosk-model-small-fr-0.22 (alphacephei.com, Apache-2.0), ~41 Mo.

use serde::Serialize;
use std::path::PathBuf;

const LIB_URL: &str = "https://github.com/alphacep/vosk-api/releases/download/v0.3.45/vosk-win64-0.3.45.zip";
const LIB_DIR: &str = "vosk-win64-0.3.45";
const MODEL_URL: &str = "https://alphacephei.com/vosk/models/vosk-model-small-fr-0.22.zip";
const MODEL_DIR: &str = "vosk-model-small-fr-0.22";

/// Mots reconnus (tout le reste est rejeté comme « [unk] »).
pub const GRAMMAR: &[&str] = &["répète", "répéter", "redis", "silence", "tais-toi", "stop", "reprends", "reprise", "radio", "point", "écart", "plan", "objectif", "objectifs", "carburant", "essence"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceCommand {
    /// Répète le dernier conseil.
    Repeat,
    /// Coupe les messages automatiques.
    Mute,
    /// Reprend les messages.
    Resume,
    /// Point : écart du dernier tour, meilleur tour, focus.
    Status,
    /// Rappel des objectifs de la séance.
    Plan,
    /// Carburant restant et tours possibles.
    Fuel,
}

/// Commande correspondant au texte reconnu.
pub fn parse_command(text: &str) -> Option<VoiceCommand> {
    let t = text.to_lowercase();
    let has = |w: &[&str]| w.iter().any(|x| t.split(|c: char| !c.is_alphanumeric() && c != '-').any(|tok| tok == *x));
    if has(&["silence", "tais-toi", "stop"]) {
        Some(VoiceCommand::Mute)
    } else if has(&["reprends", "reprise", "radio"]) {
        Some(VoiceCommand::Resume)
    } else if has(&["répète", "répéter", "redis"]) {
        Some(VoiceCommand::Repeat)
    } else if has(&["point", "écart"]) {
        Some(VoiceCommand::Status)
    } else if has(&["plan", "objectif", "objectifs"]) {
        Some(VoiceCommand::Plan)
    } else if has(&["carburant", "essence"]) {
        Some(VoiceCommand::Fuel)
    } else {
        None
    }
}

/// Grammaire Vosk : liste JSON des mots acceptés, plus « [unk] » pour rejeter le reste.
pub fn grammar_json() -> String {
    let mut v: Vec<String> = GRAMMAR.iter().map(|s| s.to_string()).collect();
    v.push("[unk]".into());
    serde_json::to_string(&v).unwrap()
}

/// Ré-échantillonnage linéaire vers 16 kHz (tests : la synthèse vocale sort du 22,05 kHz).
pub fn resample_to_16k(samples: &[i16], rate: u32) -> Vec<i16> {
    if rate == 16_000 || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = rate as f64 / 16_000.0;
    let n = (samples.len() as f64 / ratio) as usize;
    (0..n)
        .map(|i| {
            let x = i as f64 * ratio;
            let a = x.floor() as usize;
            let b = (a + 1).min(samples.len() - 1);
            let f = x - a as f64;
            (samples[a] as f64 * (1.0 - f) + samples[b] as f64 * f) as i16
        })
        .collect()
}

/// Échantillons et fréquence d'un WAV PCM 16 bits mono.
pub fn wav_pcm16(wav: &[u8]) -> Option<(Vec<i16>, u32)> {
    if wav.len() < 44 || &wav[..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let mut i = 12;
    let mut rate = 0u32;
    while i + 8 <= wav.len() {
        let id = &wav[i..i + 4];
        let len = u32::from_le_bytes(wav[i + 4..i + 8].try_into().ok()?) as usize;
        let body = wav.get(i + 8..(i + 8 + len).min(wav.len()))?;
        if id == b"fmt " && body.len() >= 8 {
            rate = u32::from_le_bytes(body[4..8].try_into().ok()?);
        } else if id == b"data" {
            return Some((body.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect(), rate));
        }
        i += 8 + len + (len & 1);
    }
    None
}

#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("commandes vocales disponibles sous Windows uniquement")]
    Unsupported,
    #[error("téléchargement impossible ({0}) : vérifie ta connexion")]
    Download(String),
    #[error("installation de la reconnaissance vocale : {0}")]
    Install(String),
    #[error("reconnaissance vocale : {0}")]
    Engine(String),
}

pub struct VoiceRecognizer {
    dir: PathBuf,
    engine: std::sync::Mutex<Option<sys::Engine>>,
}

impl VoiceRecognizer {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, engine: std::sync::Mutex::new(None) }
    }

    pub fn installed(&self) -> bool {
        self.dir.join(LIB_DIR).join("libvosk.dll").exists() && self.dir.join(MODEL_DIR).join("conf").exists()
    }

    /// Télécharge la bibliothèque (~15 Mo) et le modèle français (~41 Mo) s'ils manquent.
    pub fn install(&self) -> Result<(), VoiceError> {
        if !cfg!(windows) {
            return Err(VoiceError::Unsupported);
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| VoiceError::Install(e.to_string()))?;
        for (url, marker, zip) in [(LIB_URL, self.dir.join(LIB_DIR).join("libvosk.dll"), "vosk-lib.zip"), (MODEL_URL, self.dir.join(MODEL_DIR).join("conf"), "vosk-model.zip")] {
            if marker.exists() {
                continue;
            }
            let z = self.dir.join(zip);
            crate::tts::download_to(url, &z).map_err(|e| VoiceError::Download(e.to_string()))?;
            crate::tts::extract_zip(&z, &self.dir).map_err(|e| VoiceError::Install(e.to_string()))?;
            let _ = std::fs::remove_file(&z);
            if !marker.exists() {
                return Err(VoiceError::Install(format!("{} absent après extraction", marker.display())));
            }
        }
        Ok(())
    }

    /// Texte reconnu (parmi les mots-clés) dans un enregistrement 16 kHz.
    pub fn recognize(&self, pcm16k: &[i16]) -> Result<String, VoiceError> {
        if !self.installed() {
            self.install()?;
        }
        let mut g = self.engine.lock().unwrap_or_else(|p| p.into_inner());
        if g.is_none() {
            *g = Some(sys::Engine::load(&self.dir.join(LIB_DIR), &self.dir.join(MODEL_DIR))?);
        }
        g.as_ref().unwrap().recognize(pcm16k, &grammar_json())
    }
}

#[cfg(windows)]
mod sys {
    use super::VoiceError;
    use std::ffi::{c_char, c_int, c_void, CStr, CString};
    use std::path::Path;
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_WITH_ALTERED_SEARCH_PATH};

    type ModelNew = unsafe extern "C" fn(*const c_char) -> *mut c_void;
    type RecNewGrm = unsafe extern "C" fn(*mut c_void, f32, *const c_char) -> *mut c_void;
    type AcceptS = unsafe extern "C" fn(*mut c_void, *const i16, c_int) -> c_int;
    type FinalResult = unsafe extern "C" fn(*mut c_void) -> *const c_char;
    type RecFree = unsafe extern "C" fn(*mut c_void);
    type SetLogLevel = unsafe extern "C" fn(c_int);

    pub struct Engine {
        model: *mut c_void,
        rec_new: RecNewGrm,
        accept: AcceptS,
        final_result: FinalResult,
        rec_free: RecFree,
    }

    // La bibliothèque et le modèle restent chargés pour toute la durée de l'app ; accès sérialisé par un Mutex.
    unsafe impl Send for Engine {}

    impl Engine {
        pub fn load(lib_dir: &Path, model_dir: &Path) -> Result<Self, VoiceError> {
            let dll: Vec<u16> = lib_dir.join("libvosk.dll").as_os_str().to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
            // Recherche des DLL dépendantes (libstdc++, libgcc, winpthread) dans le dossier de libvosk.
            let h = unsafe { LoadLibraryExW(dll.as_ptr(), std::ptr::null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH) };
            if h.is_null() {
                return Err(VoiceError::Engine("chargement de libvosk.dll impossible".into()));
            }
            macro_rules! sym {
                ($name:literal, $t:ty) => {{
                    let p = unsafe { GetProcAddress(h, concat!($name, "\0").as_ptr()) };
                    match p {
                        Some(f) => unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, $t>(f) },
                        None => return Err(VoiceError::Engine(format!("fonction {} absente", $name))),
                    }
                }};
            }
            let set_log: SetLogLevel = sym!("vosk_set_log_level", SetLogLevel);
            unsafe { set_log(-1) };
            let model_new: ModelNew = sym!("vosk_model_new", ModelNew);
            let path = CString::new(model_dir.to_string_lossy().as_bytes()).map_err(|e| VoiceError::Engine(e.to_string()))?;
            let model = unsafe { model_new(path.as_ptr()) };
            if model.is_null() {
                return Err(VoiceError::Engine("modèle vocal illisible".into()));
            }
            Ok(Self {
                model,
                rec_new: sym!("vosk_recognizer_new_grm", RecNewGrm),
                accept: sym!("vosk_recognizer_accept_waveform_s", AcceptS),
                final_result: sym!("vosk_recognizer_final_result", FinalResult),
                rec_free: sym!("vosk_recognizer_free", RecFree),
            })
        }

        pub fn recognize(&self, pcm: &[i16], grammar: &str) -> Result<String, VoiceError> {
            let g = CString::new(grammar).map_err(|e| VoiceError::Engine(e.to_string()))?;
            let rec = unsafe { (self.rec_new)(self.model, crate::mic::SAMPLE_RATE as f32, g.as_ptr()) };
            if rec.is_null() {
                return Err(VoiceError::Engine("création du moteur de reconnaissance impossible".into()));
            }
            for chunk in pcm.chunks(4000) {
                unsafe { (self.accept)(rec, chunk.as_ptr(), chunk.len() as c_int) };
            }
            let json = unsafe { CStr::from_ptr((self.final_result)(rec)) }.to_string_lossy().into_owned();
            unsafe { (self.rec_free)(rec) };
            let v: serde_json::Value = serde_json::from_str(&json).map_err(|e| VoiceError::Engine(e.to_string()))?;
            Ok(v.get("text").and_then(|t| t.as_str()).unwrap_or("").replace("[unk]", "").trim().to_string())
        }
    }
}

#[cfg(not(windows))]
mod sys {
    use super::VoiceError;
    use std::path::Path;
    pub struct Engine;
    impl Engine {
        pub fn load(_lib: &Path, _model: &Path) -> Result<Self, VoiceError> {
            Err(VoiceError::Unsupported)
        }
        pub fn recognize(&self, _pcm: &[i16], _g: &str) -> Result<String, VoiceError> {
            Err(VoiceError::Unsupported)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_to_commands() {
        assert_eq!(parse_command("répète"), Some(VoiceCommand::Repeat));
        assert_eq!(parse_command("silence"), Some(VoiceCommand::Mute));
        assert_eq!(parse_command("reprends"), Some(VoiceCommand::Resume));
        assert_eq!(parse_command("radio"), Some(VoiceCommand::Resume));
        assert_eq!(parse_command("point"), Some(VoiceCommand::Status));
        assert_eq!(parse_command("plan"), Some(VoiceCommand::Plan));
        assert_eq!(parse_command("carburant"), Some(VoiceCommand::Fuel));
        assert_eq!(parse_command(""), None);
        assert_eq!(parse_command("bonjour"), None);
        assert!(grammar_json().starts_with("[\"répète\"") && grammar_json().ends_with("\"[unk]\"]"));
    }

    #[test]
    fn wav_and_resampling() {
        let samples: Vec<i16> = (0..2205).map(|i| (i % 100) as i16).collect();
        let mut wav = b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0".to_vec();
        wav.extend_from_slice(&22050u32.to_le_bytes());
        wav.extend_from_slice(&44100u32.to_le_bytes());
        wav.extend_from_slice(&[2, 0, 16, 0]);
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&((samples.len() * 2) as u32).to_le_bytes());
        for s in &samples {
            wav.extend_from_slice(&s.to_le_bytes());
        }
        let (pcm, rate) = wav_pcm16(&wav).unwrap();
        assert_eq!((pcm.len(), rate), (2205, 22050));
        assert_eq!(resample_to_16k(&pcm, rate).len(), 1600);
    }

    /// Bout en bout réel (CI Windows, APEX_TTS_E2E=1) : la voix du coach prononce chaque mot-clé,
    /// la reconnaissance doit retrouver la bonne commande.
    #[cfg(windows)]
    #[test]
    fn windows_spoken_keywords_are_recognized() {
        if std::env::var("APEX_TTS_E2E").is_err() {
            return;
        }
        let tts = crate::tts::Tts::new(std::env::temp_dir().join("apex-tts-e2e"));
        let rec = VoiceRecognizer::new(std::env::temp_dir().join("apex-voice-e2e"));
        for (word, cmd) in [("Répète.", VoiceCommand::Repeat), ("Silence.", VoiceCommand::Mute), ("Reprends.", VoiceCommand::Resume), ("Point.", VoiceCommand::Status), ("Plan.", VoiceCommand::Plan), ("Carburant.", VoiceCommand::Fuel)] {
            let wav = tts.synthesize("tom", word).unwrap();
            let (pcm, rate) = wav_pcm16(&wav).unwrap();
            let text = rec.recognize(&resample_to_16k(&pcm, rate)).unwrap();
            assert_eq!(parse_command(&text), Some(cmd), "« {word} » reconnu comme « {text} »");
        }
    }
}
