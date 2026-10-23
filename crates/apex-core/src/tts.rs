//! Voix naturelles pour le coach : synthèse vocale neuronale hors ligne (Piper, MIT) avec des voix
//! françaises téléchargées à la demande. Le moteur et la voix choisie sont installés une fois dans
//! les données de l'app ; ensuite tout reste sur le PC, sans service en ligne.
//!
//! Moteur : github.com/rhasspy/piper (licence MIT), version Windows précompilée.
//! Voix : huggingface.co/rhasspy/piper-voices v1.0.0 (licence propre à chaque voix, usage personnel).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

const ENGINE_URL: &str = "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip";
const VOICES_URL: &str = "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0";

#[derive(Debug, Clone, Copy)]
pub struct VoiceDef {
    pub id: &'static str,
    pub label: &'static str,
    /// Chemin du modèle dans le dépôt de voix, sans extension.
    pub path: &'static str,
    /// Locuteur dans un modèle à plusieurs voix (nom dans `speaker_id_map`).
    pub speaker: Option<&'static str>,
}

pub const VOICES: &[VoiceDef] = &[
    VoiceDef { id: "tom", label: "Tom (homme)", path: "fr/fr_FR/tom/medium/fr_FR-tom-medium", speaker: None },
    VoiceDef { id: "upmc-pierre", label: "Pierre (homme)", path: "fr/fr_FR/upmc/medium/fr_FR-upmc-medium", speaker: Some("pierre") },
    VoiceDef { id: "siwis", label: "Siwis (femme)", path: "fr/fr_FR/siwis/medium/fr_FR-siwis-medium", speaker: None },
    VoiceDef { id: "upmc-jessica", label: "Jessica (femme)", path: "fr/fr_FR/upmc/medium/fr_FR-upmc-medium", speaker: Some("jessica") },
    VoiceDef { id: "gilles", label: "Gilles (homme, qualité réduite)", path: "fr/fr_FR/gilles/low/fr_FR-gilles-low", speaker: None },
];

pub fn voice(id: &str) -> Option<&'static VoiceDef> {
    VOICES.iter().find(|v| v.id == id)
}

#[derive(Debug, thiserror::Error)]
pub enum TtsError {
    #[error("voix inconnue : {0}")]
    UnknownVoice(String),
    #[error("voix naturelles disponibles sous Windows uniquement")]
    Unsupported,
    #[error("téléchargement impossible ({0}) : vérifie ta connexion")]
    Download(String),
    #[error("installation du moteur vocal : {0}")]
    Install(String),
    #[error("synthèse vocale : {0}")]
    Synth(String),
}

/// Moteur et voix installés dans `dir`.
pub struct Tts {
    dir: PathBuf,
    lock: Mutex<()>,
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Numéro du locuteur dans le fichier de configuration de la voix.
pub fn speaker_id(config_json: &str, name: &str) -> Option<i64> {
    let v: serde_json::Value = serde_json::from_str(config_json).ok()?;
    v.get("speaker_id_map")?.get(name)?.as_i64()
}

/// Arguments de la ligne de commande Piper (le texte est envoyé sur l'entrée standard).
pub fn piper_args(model: &Path, out: &Path, speaker: Option<i64>) -> Vec<String> {
    let mut a = vec![
        "--model".into(),
        model.to_string_lossy().into_owned(),
        "--output_file".into(),
        out.to_string_lossy().into_owned(),
        // Débit légèrement plus vif qu'en lecture, comme à la radio.
        "--length_scale".into(),
        "0.92".into(),
        "--sentence_silence".into(),
        "0.12".into(),
    ];
    if let Some(s) = speaker {
        a.push("--speaker".into());
        a.push(s.to_string());
    }
    a
}

/// Téléchargement vers un fichier (via un fichier temporaire renommé à la fin).
pub fn download_to(url: &str, dest: &Path) -> Result<(), TtsError> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(600))).build().into();
    let mut resp = agent.get(url).header("User-Agent", "apex-companion").call().map_err(|e| TtsError::Download(e.to_string()))?;
    let tmp = dest.with_extension("part");
    let mut f = std::fs::File::create(&tmp).map_err(|e| TtsError::Install(e.to_string()))?;
    std::io::copy(&mut resp.body_mut().with_config().limit(500 * 1024 * 1024).reader(), &mut f).map_err(|e| TtsError::Download(e.to_string()))?;
    drop(f);
    if std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0) < 1024 {
        let _ = std::fs::remove_file(&tmp);
        return Err(TtsError::Download("fichier vide".into()));
    }
    std::fs::rename(&tmp, dest).map_err(|e| TtsError::Install(e.to_string()))
}

impl Tts {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, lock: Mutex::new(()) }
    }

    fn engine(&self) -> PathBuf {
        self.dir.join("piper").join("piper.exe")
    }

    fn model(&self, v: &VoiceDef) -> PathBuf {
        self.dir.join(format!("{}.onnx", file_name(v.path)))
    }

    pub fn installed(&self, id: &str) -> bool {
        voice(id).is_some_and(|v| self.engine().exists() && self.model(v).exists() && self.model(v).with_extension("onnx.json").exists())
    }

    /// Télécharge le moteur et la voix s'ils manquent (~22 Mo + ~60 Mo).
    pub fn install(&self, id: &str) -> Result<(), TtsError> {
        let v = voice(id).ok_or_else(|| TtsError::UnknownVoice(id.into()))?;
        if !cfg!(windows) {
            return Err(TtsError::Unsupported);
        }
        let _g = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        std::fs::create_dir_all(&self.dir).map_err(|e| TtsError::Install(e.to_string()))?;
        if !self.engine().exists() {
            let zip = self.dir.join("piper_windows_amd64.zip");
            download_to(ENGINE_URL, &zip)?;
            extract_zip(&zip, &self.dir)?;
            let _ = std::fs::remove_file(&zip);
            if !self.engine().exists() {
                return Err(TtsError::Install("extraction du moteur échouée".into()));
            }
        }
        let model = self.model(v);
        let config = model.with_extension("onnx.json");
        if !config.exists() {
            download_to(&format!("{VOICES_URL}/{}.onnx.json?download=true", v.path), &config)?;
        }
        if !model.exists() {
            download_to(&format!("{VOICES_URL}/{}.onnx?download=true", v.path), &model)?;
        }
        Ok(())
    }

    /// Fichier WAV prononçant `text` avec la voix `id` (installée au besoin).
    pub fn synthesize(&self, id: &str, text: &str) -> Result<Vec<u8>, TtsError> {
        let v = voice(id).ok_or_else(|| TtsError::UnknownVoice(id.into()))?;
        if !self.installed(id) {
            self.install(id)?;
        }
        let model = self.model(v);
        let speaker = match v.speaker {
            Some(name) => {
                let cfg = std::fs::read_to_string(model.with_extension("onnx.json")).map_err(|e| TtsError::Synth(e.to_string()))?;
                speaker_id(&cfg, name)
            }
            None => None,
        };
        let out = self.dir.join(format!("coach-{}.wav", std::process::id()));
        let mut cmd = std::process::Command::new(self.engine());
        cmd.args(piper_args(&model, &out, speaker)).current_dir(self.dir.join("piper")).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
        no_window(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| TtsError::Synth(e.to_string()))?;
        {
            use std::io::Write;
            let mut stdin = child.stdin.take().ok_or_else(|| TtsError::Synth("entrée standard".into()))?;
            stdin.write_all(text.replace('\n', " ").as_bytes()).map_err(|e| TtsError::Synth(e.to_string()))?;
        }
        let res = child.wait_with_output().map_err(|e| TtsError::Synth(e.to_string()))?;
        if !res.status.success() {
            return Err(TtsError::Synth(String::from_utf8_lossy(&res.stderr).lines().last().unwrap_or("échec").to_string()));
        }
        let wav = std::fs::read(&out).map_err(|e| TtsError::Synth(e.to_string()))?;
        let _ = std::fs::remove_file(&out);
        Ok(wav)
    }
}

/// Extraction d'une archive .zip avec tar.exe, fourni par Windows depuis Windows 10.
pub fn extract_zip(zip: &Path, dest: &Path) -> Result<(), TtsError> {
    let mut cmd = std::process::Command::new("tar");
    cmd.arg("-xf").arg(zip).arg("-C").arg(dest);
    no_window(&mut cmd);
    let st = cmd.status().map_err(|e| TtsError::Install(format!("extraction : {e}")))?;
    if st.success() {
        Ok(())
    } else {
        Err(TtsError::Install(format!("extraction de {} échouée", zip.display())))
    }
}

/// Pas de fenêtre de console quand l'app lance un outil.
fn no_window(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_and_command_line() {
        assert!(VOICES.iter().all(|v| v.path.starts_with("fr/fr_FR/")));
        assert_eq!(file_name(voice("tom").unwrap().path), "fr_FR-tom-medium");
        let cfg = r#"{"audio":{"sample_rate":22050},"num_speakers":2,"speaker_id_map":{"jessica":0,"pierre":1}}"#;
        assert_eq!(speaker_id(cfg, "pierre"), Some(1));
        assert_eq!(speaker_id(cfg, "inconnu"), None);
        let a = piper_args(Path::new("m.onnx"), Path::new("o.wav"), Some(1));
        assert_eq!(&a[..4], &["--model", "m.onnx", "--output_file", "o.wav"]);
        assert_eq!(&a[a.len() - 2..], &["--speaker", "1"]);
        let t = Tts::new(std::env::temp_dir().join("apex-tts-test-absent"));
        assert!(!t.installed("tom"));
        assert!(matches!(t.synthesize("nope", "x"), Err(TtsError::UnknownVoice(_))));
    }

    /// Bout en bout réel (CI Windows, APEX_TTS_E2E=1) : téléchargement du moteur et d'une voix,
    /// puis synthèse d'une phrase du coach en WAV.
    #[cfg(windows)]
    #[test]
    fn windows_end_to_end_synthesis() {
        if std::env::var("APEX_TTS_E2E").is_err() {
            return;
        }
        let dir = std::env::temp_dir().join("apex-tts-e2e");
        let t = Tts::new(dir);
        for id in ["tom", "upmc-pierre"] {
            let wav = t.synthesize(id, "Plus 3 dixièmes. Focus virage 16 : freine 10 mètres plus tard.").unwrap();
            assert!(wav.len() > 20_000 && &wav[..4] == b"RIFF" && &wav[8..12] == b"WAVE", "{id} : {} octets", wav.len());
            assert!(t.installed(id));
        }
    }
}
