//! Couche de service applicative : toute la logique des commandes UI,
//! indépendante de Tauri (donc testable sans WebView).

use crate::analysis::compare::{compare_laps_named, Comparison, LapInput};
use crate::analysis::goals::{evaluate as evaluate_goal, GoalMetric, GoalResult, GoalSuggestion};
use crate::analysis::progress::{progress_zones_named, ProgressZone, SessionLap};
use crate::analysis::session::{best_clean_lap, consistency, theoretical_best, Consistency};
use crate::connectors::{registry, Capabilities, ConnectorConfig, InstallStatus, SimulatorConnector};
use crate::mock::demo_session;
use crate::model::*;
use crate::storage::{InsertOutcome, LapRow, SessionFilter, SessionSummary, Store, StoreError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    User(String),
}

impl Serialize for ServiceError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ServiceError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    pub iracing_telemetry_dir: Option<String>,
    pub units: String,
    pub live_enabled: bool,
    pub sample_rate_hz: u32,
    pub auto_import: bool,
    pub coach_in_race: bool,
    pub coach_audio: bool,
    pub coach_min_interval_s: u32,
    pub log_level: String,
    /// Toujours false dans cette version : aucune fonctionnalité cloud n'existe.
    pub cloud_enabled: bool,
    /// Mode débutant : 3 priorités maximum, régularité avant vitesse pure.
    pub beginner_mode: bool,
    /// Vérifie au démarrage si une nouvelle version existe (seule requête réseau de l'app).
    pub check_updates: bool,
    /// Dossier où copier automatiquement chaque nouvelle session (ex. iCloud Drive) pour l'app mobile.
    /// Vide = désactivé. L'app n'envoie rien elle-même : c'est le dossier choisi qui synchronise.
    pub mobile_export_dir: Option<String>,
    /// Jeton d'accès personnel Garage 61 (vide = fonction désactivée, aucune connexion).
    pub garage61_token: Option<String>,
    /// Après chaque nouvelle session, cherche sur Garage 61 un tour de référence plus rapide.
    pub garage61_auto: bool,
    /// Importe automatiquement les tours « Garage 61 - … .csv » qui arrivent dans Téléchargements.
    pub watch_downloads: bool,
    /// Pilotes Garage 61 à utiliser comme références (identifiant ou adresse de leur profil).
    pub garage61_drivers: Vec<String>,
    /// Dossier de téléchargements à surveiller (vide = dossier Téléchargements de Windows, où qu'il soit).
    pub downloads_dir: Option<String>,
    /// Voix du coach : « windows » (synthèse de Windows) ou une voix naturelle de `tts::VOICES`.
    #[serde(default = "default_voice")]
    pub coach_voice: String,
    /// Volume de la voix du coach (0 à 100).
    #[serde(default = "default_volume")]
    pub coach_volume: u8,
    /// Annonce courte juste avant le virage du focus.
    #[serde(default = "default_true")]
    pub coach_callouts: bool,
    /// Clé et région de la ressource Azure AI Speech (voix en ligne Azure).
    pub azure_tts_key: Option<String>,
    pub azure_tts_region: Option<String>,
    /// Bips de freinage sur le virage du focus (point de freinage de la référence).
    #[serde(default = "default_true")]
    pub coach_brake_beeps: bool,
    /// Commandes vocales au volant (bouton maintenu + mot-clé).
    pub voice_commands: bool,
    /// Bouton « radio » du volant (ou touche clavier).
    pub ptt_binding: Option<crate::ptt::PttBinding>,
}

fn default_true() -> bool {
    true
}

fn default_volume() -> u8 {
    50
}

fn default_voice() -> String {
    "windows".into()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            iracing_telemetry_dir: None,
            units: "metric".into(),
            live_enabled: true,
            sample_rate_hz: 60,
            auto_import: true,
            coach_in_race: false,
            coach_audio: false,
            coach_min_interval_s: 8,
            log_level: "info".into(),
            cloud_enabled: false,
            beginner_mode: true,
            check_updates: true,
            mobile_export_dir: None,
            garage61_token: None,
            garage61_auto: true,
            watch_downloads: true,
            garage61_drivers: vec![],
            downloads_dir: None,
            coach_voice: default_voice(),
            coach_volume: default_volume(),
            coach_callouts: true,
            coach_brake_beeps: true,
            azure_tts_key: None,
            azure_tts_region: None,
            voice_commands: false,
            ptt_binding: None,
        }
    }
}

/// Un circuit roulé (circuit + configuration + voiture), vu sur tout l'historique.
#[derive(Debug, Clone, Serialize)]
pub struct CircuitOverview {
    /// Type de série : les temps fixed et open ne sont pas mélangés (`None` : sessions de type inconnu).
    pub setup: Option<SetupType>,
    pub sim: SimId,
    pub track: String,
    pub track_config: Option<String>,
    pub car: String,
    pub track_length_m: Option<f32>,
    pub sessions: usize,
    pub laps: usize,
    /// Tours lancés complets (valides ou non).
    pub timed_laps: usize,
    pub clean_laps: usize,
    pub best_clean_s: Option<f64>,
    pub best_any_s: Option<f64>,
    pub first_unix: i64,
    pub last_unix: i64,
    /// Noms de virages : "exact" (relevé), "aligned" (géométrie publique), "none".
    pub corner_names: String,
    /// Référence externe (pilote, temps) si une est importée pour ce circuit et cette voiture.
    pub external_reference: Option<(String, f64)>,
}

/// Un tour de l'historique d'un circuit.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryLap {
    pub lap_id: i64,
    pub session_id: i64,
    pub session_kind: SessionKind,
    pub started_at_unix: i64,
    pub number: i32,
    pub kind: LapKind,
    pub lap_time_s: Option<f64>,
    pub clean: bool,
    pub off_track: bool,
    pub incident: bool,
    pub is_reference: bool,
    /// Écart au meilleur tour propre du circuit (s).
    pub delta_to_best_s: Option<f64>,
    /// Ce tour était le meilleur tour propre au moment où il a été roulé (progression).
    pub personal_best_at_time: bool,
}

#[derive(Debug, Clone)]
pub struct ExternalImportReport {
    pub file: String,
    pub result: std::result::Result<ExternalImport, String>,
}

/// Résultat de la récupération de tes sessions depuis Garage 61.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RecoverReport {
    /// Sessions recréées dans Apex.
    pub sessions: usize,
    pub laps: usize,
    /// Une ligne lisible par combinaison circuit/voiture.
    pub messages: Vec<String>,
}

/// Un de tes tours sur Garage 61, ramené à l'heure locale du PC (comme les sessions iRacing).
#[derive(Debug, Clone)]
struct RemoteLap {
    id: String,
    start: i64,
    lap_time: f64,
    clean: bool,
    number: Option<i64>,
}

/// Circuit, configuration, voiture.
type ComboKey = (String, Option<String>, String);

/// Session Apex reconstruite à partir de tours Garage 61 (`csv` : télémétrie d'un tour par son id).
/// `Err` : téléchargement impossible (on s'arrête) ; `Ok(None)` : aucun tour lisible.
/// Avertissement quand la référence et le tour comparé ne sont pas du même type de série.
fn setup_warning(reference: Option<SetupType>, lap: Option<SetupType>) -> Option<String> {
    match (reference, lap) {
        (Some(r), Some(l)) if r != l => Some(format!(
            "Attention : référence roulée en {}, ton tour en {}. Vitesses de pointe et points de freinage ne sont pas comparables.",
            r.label(),
            l.label()
        )),
        (None, Some(l)) => Some(format!("Type de série de la référence inconnu (ton tour : {}). Précise-le dans Sessions › Références d'autres pilotes.", l.label())),
        _ => None,
    }
}

/// Type de série d'un groupe de tours : celui de la majorité des tours dont le type est connu.
fn group_setup(g: &[RemoteLap], tags: &std::collections::HashMap<String, SetupType>) -> Option<SetupType> {
    let fixed = g.iter().filter(|l| tags.get(&l.id) == Some(&SetupType::Fixed)).count();
    let open = g.iter().filter(|l| tags.get(&l.id) == Some(&SetupType::Open)).count();
    match (fixed, open) {
        (0, 0) => None,
        (f, o) if f >= o => Some(SetupType::Fixed),
        _ => Some(SetupType::Open),
    }
}

fn recovered_session(meta0: &SessionMeta, length: f32, setup: Option<SetupType>, g: &[RemoteLap], mut csv: impl FnMut(&str) -> std::result::Result<String, String>) -> std::result::Result<Option<ImportedSession>, String> {
    let mut out_laps = vec![];
    for l in g {
        let text = csv(&l.id)?;
        match crate::ingest::garage61::parse_csv(&text, length, l.lap_time) {
            Ok(p) => {
                let invalid = !l.clean || p.off_track;
                out_laps.push(ImportedLap {
                    meta: LapMeta {
                        number: l.number.map(|n| n.max(0) as i32).unwrap_or(out_laps.len() as i32 + 1),
                        kind: LapKind::Flying,
                        lap_time_s: Some(l.lap_time),
                        sector_times_s: vec![],
                        flags: LapFlags { off_track: p.off_track, ..Default::default() },
                        validity: if invalid { Validity::Invalid } else { Validity::Valid },
                        validity_source: ValiditySource::Derived,
                        fuel_start_l: None,
                        fuel_used_l: None,
                    },
                    trace: p.trace,
                });
            }
            Err(e) => log::warn!("tour Garage 61 {} illisible : {e}", l.id),
        }
    }
    if out_laps.is_empty() {
        return Ok(None);
    }
    Ok(Some(ImportedSession {
        meta: SessionMeta {
            sim: SimId::Iracing,
            track: meta0.track.clone(),
            track_config: meta0.track_config.clone(),
            track_length_m: Some(length),
            car: meta0.car.clone(),
            kind: SessionKind::Unknown,
            started_at_unix: g[0].start,
            weather: Weather::default(),
            source_fingerprint: format!("garage61-session:{}", g[0].id),
            source_path: Some(format!("Garage 61 (récupérée) {}", g[0].id)),
            setup,
        },
        laps: out_laps,
        sample_rate_hz: Some(60.0),
        race: None,
        profile: None,
    }))
}

/// Regroupe des tours (triés par heure de début) en sessions : nouvelle session dès qu'il s'écoule
/// plus de `gap_s` entre la fin d'un tour et le début du suivant (retour au stand prolongé, autre séance).
fn group_into_sessions(mut laps: Vec<RemoteLap>, gap_s: i64) -> Vec<Vec<RemoteLap>> {
    laps.sort_by_key(|l| l.start);
    let mut out: Vec<Vec<RemoteLap>> = vec![];
    for l in laps {
        match out.last_mut() {
            Some(cur) if l.start - cur.last().map(|p| p.start + p.lap_time.ceil() as i64).unwrap_or(l.start) <= gap_s => cur.push(l),
            _ => out.push(vec![l]),
        }
    }
    out
}

/// Décalage (s) de l'heure locale du PC à une date donnée : iRacing horodate ses fichiers en heure
/// locale, les sessions récupérées doivent l'être aussi pour être comparables.
fn local_offset_at(utc: i64) -> i64 {
    use chrono::{Local, Offset, TimeZone};
    Local.timestamp_opt(utc, 0).single().map(|d| d.offset().fix().local_minus_utc() as i64).unwrap_or(0)
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalImport {
    pub session_id: i64,
    pub lap_id: i64,
    pub driver: String,
    pub lap_time_s: f64,
    pub track: String,
    /// La voiture du fichier correspond à celle de tes sessions sur ce circuit.
    pub car_matches: bool,
    pub set_as_reference: bool,
    /// Type de série du tour importé (`None` : à préciser par le pilote).
    pub setup: Option<SetupType>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectorInfo {
    pub id: SimId,
    pub name: String,
    pub process_names: Vec<String>,
    pub capabilities: Capabilities,
    pub install: InstallStatus,
    pub import_extensions: Vec<String>,
    pub default_import_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    pub path: String,
    pub outcome: Option<InsertOutcome>,
    pub laps: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExportSummary {
    pub path: String,
    pub bytes: u64,
    pub laps: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AutoLogStatus {
    /// `None` : réglage absent du fichier (on n'invente rien).
    pub enabled: Option<bool>,
    pub path: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct SyncReport {
    pub imported: usize,
    pub duplicates: usize,
    /// Fichiers encore en cours d'écriture (session en cours).
    pub pending: usize,
    pub errors: Vec<String>,
    /// Sessions nouvellement importées.
    pub new_sessions: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionDetail {
    pub id: i64,
    pub meta: SessionMeta,
    /// Fiche voiture / circuit (session, ou dernière connue pour ce circuit et cette voiture).
    pub profile: Option<SessionProfile>,
    /// Circuit et configuration reconnus dans la banque OpenStreetMap.
    pub bank: Option<crate::trackdb::BankMatch>,
    pub laps: Vec<LapRow>,
    pub best_lap_id: Option<i64>,
    pub theoretical_best_s: Option<f64>,
    pub consistency: Option<Consistency>,
    /// Référence personnelle du même combo (sim/circuit/config/voiture), si elle existe.
    pub personal_reference_lap_id: Option<i64>,
    /// Tour lancé complet le plus rapide, même invalide (hors-piste, incident).
    pub fastest_lap_id: Option<i64>,
    /// Référence utilisée par défaut pour l'analyse : référence perso, sinon meilleur tour propre,
    /// sinon tour lancé le plus rapide (quand aucun tour n'est propre).
    pub analysis_reference_lap_id: Option<i64>,
    /// Sorties de piste regroupées par endroit.
    pub off_tracks: Vec<crate::analysis::offtrack::OffTrackZone>,
    /// Libellé lisible de la référence personnelle quand elle vient d'ailleurs (autre pilote, autre session).
    pub reference_label: Option<String>,
    /// Carte de risque par virage (toutes tes sessions sur ce circuit et cette voiture), la plus prioritaire d'abord.
    pub risk: Vec<crate::analysis::risk::CornerRisk>,
    pub risk_headline: Option<String>,
    /// Coach de course (courses iRacing) : grille, positions, incidents, points à travailler.
    pub race: Option<crate::analysis::race::RaceReport>,
}

/// Objectif du programme et son résultat sur la séance suivante.
#[derive(Debug, Clone, Serialize)]
pub struct PlanItem {
    #[serde(flatten)]
    pub objective: crate::analysis::plan::PlanObjective,
    pub result: Option<GoalResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanView {
    pub id: i64,
    pub created_at_unix: i64,
    pub track: String,
    pub car: String,
    pub objectives: Vec<PlanItem>,
    /// Séance sur laquelle le programme a été évalué (la première après sa création).
    pub evaluated_session_id: Option<i64>,
    /// Version radio.
    pub spoken: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GoalView {
    pub id: i64,
    pub title: String,
    pub track_key: String,
    pub created_at_unix: i64,
    /// Résultat sur la dernière session du même circuit roulée après la création de l'objectif.
    pub result: Option<GoalResult>,
    pub evaluated_session_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Dashboard {
    pub active_goal: Option<GoalView>,
    /// Objectif proposé (mode débutant : rester en piste / régularité).
    pub next_goal_suggestion: Option<GoalSuggestion>,
    pub last_session: Option<SessionSummary>,
    pub theoretical_best_s: Option<f64>,
    pub progress_zones: Vec<ProgressZone>,
    /// (début de session, écart-type des tours propres) pour les sessions du même combo.
    pub consistency_trend: Vec<(i64, f64)>,
    pub next_goal: Option<String>,
    pub total_sessions: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostics {
    pub app_version: String,
    pub os: String,
    pub arch: String,
    pub sessions: usize,
    pub connectors: Vec<(String, Capabilities)>,
    pub note: String,
}

pub struct AppService {
    store: Mutex<Store>,
    connectors: Vec<Box<dyn SimulatorConnector>>,
    /// Catalogue Garage 61 (circuits, voitures), chargé une fois par lancement.
    g61_catalog: Mutex<Option<(Vec<crate::garage61_api::G61Track>, Vec<crate::garage61_api::G61Car>)>>,
    /// Dernière carte de risque calculée : (clé combo + nb de tours + référence, résultat).
    risk_cache: Mutex<Option<(String, Vec<crate::analysis::risk::CornerRisk>, crate::analysis::risk::RiskTotals)>>,
    /// Circuit reconnu dans la banque OpenStreetMap, par circuit + configuration.
    bank_cache: Mutex<std::collections::HashMap<String, Option<crate::trackdb::BankMatch>>>,
    /// Dernier relevé de piste calculé (clé : circuit, tour de référence, nombre de tours).
    overlay_cache: Mutex<Option<(String, crate::analysis::surface::TrackOverlay)>>,
    /// Fichiers en erreur déjà réessayés pendant ce lancement (on retente une fois par lancement,
    /// et à chaque passage si l'erreur est passagère : fichier verrouillé par un autre programme).
    retried: Mutex<std::collections::HashSet<String>>,
    /// Dossier des copies de garde (liens physiques) des .ibt : un autre logiciel (l'agent Garage 61
    /// avec « supprimer après envoi ») peut effacer le fichier quelques secondes après la fin de
    /// l'écriture, avant qu'Apex ait pu l'importer. `None` : pas de garde (tests en mémoire).
    keep_dir: Option<PathBuf>,
    /// Voix naturelles du coach (moteur et voix téléchargés à la demande).
    tts: Option<crate::tts::Tts>,
    /// Messages déjà synthétisés par Azure (voix|texte → WAV).
    tts_cache: Mutex<std::collections::HashMap<String, Vec<u8>>>,
    /// Reconnaissance des commandes vocales (téléchargée à la demande).
    voice: Option<crate::voice_cmd::VoiceRecognizer>,
    /// Fichiers déjà gardés (ou déjà importés) pendant ce lancement : évite de retoucher la base à chaque tour de boucle.
    guarded: Mutex<std::collections::HashSet<String>>,
}

const SETTINGS_KEY: &str = "app_settings";

/// Clé de circuit pour les noms de virages : simulateur + circuit + configuration.
/// Temps au tour pour la voix : « 1 minute 56 point 5 ».
fn spoken_lap(t: Option<f64>) -> String {
    match t {
        Some(t) => {
            let m = (t / 60.0).floor() as i64;
            let s = t - m as f64 * 60.0;
            if m > 0 {
                format!("{m} minute {} point {}", s.floor() as i64, ((s - s.floor()) * 10.0).floor() as i64)
            } else {
                format!("{} secondes point {}", s.floor() as i64, ((s - s.floor()) * 10.0).floor() as i64)
            }
        }
        None => String::new(),
    }
}

pub fn track_key(m: &SessionMeta) -> String {
    format!("{}|{}|{}", m.sim.as_str(), m.track, m.track_config.as_deref().unwrap_or(""))
}

/// 66.052 → « 1:06.052 »
/// Dossier Téléchargements de Windows (dossier connu FOLDERID_Downloads : suit les déplacements).
#[cfg(windows)]
fn system_downloads_dir() -> Option<PathBuf> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{SHGetKnownFolderPath, FOLDERID_Downloads};
    let mut p: windows_sys::core::PWSTR = std::ptr::null_mut();
    // SAFETY : appel documenté ; la chaîne renvoyée est libérée par CoTaskMemFree.
    let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_Downloads, 0, std::ptr::null_mut(), &mut p) };
    let out = if hr >= 0 && !p.is_null() {
        let mut len = 0;
        // SAFETY : chaîne UTF-16 terminée par 0.
        unsafe {
            while *p.add(len) != 0 {
                len += 1;
            }
            Some(PathBuf::from(String::from_utf16_lossy(std::slice::from_raw_parts(p, len))))
        }
    } else {
        None
    };
    if !p.is_null() {
        unsafe { CoTaskMemFree(p as *const core::ffi::c_void) };
    }
    out.filter(|d| d.is_dir()).or_else(|| {
        let d = PathBuf::from(std::env::var_os("USERPROFILE")?).join("Downloads");
        d.is_dir().then_some(d)
    })
}

#[cfg(not(windows))]
fn system_downloads_dir() -> Option<PathBuf> {
    let d = PathBuf::from(std::env::var_os("HOME")?).join("Downloads");
    d.is_dir().then_some(d)
}

fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Date civile (UTC) d'un horodatage Unix : (année, mois, jour).
fn civil_date(unix: i64) -> (i64, u32, u32) {
    let z = unix.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Nom de fichier d'export lisible : « 2026-10-17 21h05 Interlagos - Super Formula Lights.apex.json.gz ».
pub fn export_file_name(m: &SessionMeta) -> String {
    let (y, mo, d) = civil_date(m.started_at_unix);
    let secs = m.started_at_unix.rem_euclid(86_400);
    let raw = format!("{y:04}-{mo:02}-{d:02} {:02}h{:02} {} - {}", secs / 3600, (secs % 3600) / 60, m.track, m.car);
    let clean: String = raw.chars().map(|c| if c.is_alphanumeric() || " -_.()".contains(c) { c } else { '_' }).collect();
    format!("{}.apex.json.gz", clean.trim())
}

/// Remplace les conseils de vitesse des virages à sécuriser (ou coupés) par un conseil de sécurité,
/// placé en tête : chercher du temps là où l'on sort une fois sur trois est contre-productif.
pub(crate) fn apply_risk_to_insights(c: &mut Comparison, risk: &[crate::analysis::risk::CornerRisk]) {
    use crate::analysis::insights::{Confidence, Evidence, Insight, Priority};
    use crate::analysis::risk::RiskVerdict;
    let mut front = vec![];
    for r in risk.iter().filter(|r| matches!(r.verdict, RiskVerdict::Secure | RiskVerdict::Cuts)) {
        c.insights.retain(|i| i.segment.as_deref() != Some(r.name.as_str()));
        let cuts = r.verdict == RiskVerdict::Cuts;
        front.push(Insight {
            id: format!("risk-{}", r.apex_m as i64),
            title: format!("{} : {}", r.name, if cuts { "tu coupes la piste" } else { "sécuriser avant d'attaquer" }),
            priority: Priority::Critical,
            est_gain_s: (!cuts && r.expected_cost_s > 0.02).then_some([r.expected_cost_s * 0.5, r.expected_cost_s]),
            segment: Some(r.name.clone()),
            segment_range_m: Some([r.entry_m, r.exit_m]),
            explanation: r.message.clone(),
            evidence: vec![
                Evidence { label: "Sorties de piste".into(), reference: "0".into(), compared: format!("{} sur {} passages", r.offs, r.passes), delta: format!("{:.0} %", r.off_rate * 100.0) },
                Evidence {
                    label: "Temps de traversée".into(),
                    reference: r.mean_clean_s.map_or("—".into(), |v| format!("{v:.2} s (propre)")),
                    compared: r.mean_off_s.map_or("—".into(), |v| format!("{v:.2} s (sortie)")),
                    delta: r.off_cost_s.map_or("—".into(), |v| format!("{v:+.2} s")),
                },
            ],
            confidence: if r.passes >= 8 { Confidence::High } else { Confidence::Medium },
            confidence_notes: vec![format!("mesuré sur {} passages, {} session(s)", r.passes, r.sessions)],
            recommendation: if cuts {
                format!("Reste dans les limites de piste à {} : vise la ligne blanche intérieure sans la dépasser, même si c'est plus lent. Un tour coupé ne compte pas et te coûte un incident.", r.name)
            } else {
                format!("Prochaine session à {} : freine 5 m plus tôt que d'habitude et vise 90 % de passages sans sortie. Tu rattraperas le temps ensuite, quand le virage sera fiable.", r.name)
            },
            goal: Some(crate::analysis::goals::GoalSuggestion {
                title: format!("{} : 90 % de passages sans sortie", r.name),
                metric: crate::analysis::goals::GoalMetric::CornerClean { apex_m: r.apex_m, target_rate: 0.9 },
            }),
        });
    }
    front.extend(std::mem::take(&mut c.insights));
    c.insights = front;
}

pub fn fmt_lap(s: f64) -> String {
    let ms = (s * 1000.0).round() as i64;
    format!("{}:{:06.3}", ms / 60_000, (ms % 60_000) as f64 / 1000.0)
}

impl AppService {
    pub fn open(db: &Path) -> Result<Self> {
        if let Some(dir) = db.parent() {
            std::fs::create_dir_all(dir).map_err(|e| ServiceError::User(format!("dossier de données : {e}")))?;
        }
        let mut store = Store::open(db)?;
        match store.dedupe_sessions() {
            Ok(n) if n > 0 => log::info!("{n} session(s) en double supprimée(s)"),
            Ok(_) => {}
            Err(e) => log::warn!("dédoublonnage des sessions : {e}"),
        }
        Ok(Self {
            store: Mutex::new(store),
            connectors: registry(),
            g61_catalog: Mutex::new(None),
            risk_cache: Mutex::new(None),
            bank_cache: Mutex::new(Default::default()),
            overlay_cache: Mutex::new(None),
            retried: Mutex::new(Default::default()),
            keep_dir: db.parent().map(|d| d.join("telemetrie-gardee")),
            tts: db.parent().map(|d| crate::tts::Tts::new(d.join("voix"))),
            tts_cache: Mutex::new(Default::default()),
            voice: db.parent().map(|d| crate::voice_cmd::VoiceRecognizer::new(d.join("voix"))),
            guarded: Mutex::new(Default::default()),
        })
    }

    pub fn in_memory() -> Result<Self> {
        Ok(Self {
            store: Mutex::new(Store::open_in_memory()?),
            connectors: registry(),
            g61_catalog: Mutex::new(None),
            risk_cache: Mutex::new(None),
            bank_cache: Mutex::new(Default::default()),
            overlay_cache: Mutex::new(None),
            retried: Mutex::new(Default::default()),
            keep_dir: None,
            tts: None,
            tts_cache: Mutex::new(Default::default()),
            voice: None,
            guarded: Mutex::new(Default::default()),
        })
    }

    /// Faut-il (re)traiter ce fichier ? Nouveau ou modifié : oui. Déjà traité sans erreur : non.
    /// En erreur : une fois par lancement, ou à chaque passage si l'erreur est passagère.
    fn should_process(&self, key: &str, size: i64, mtime: i64) -> Result<bool> {
        let mark = || self.retried.lock().unwrap_or_else(|p| p.into_inner()).insert(key.to_string());
        match self.db().file_record(key)? {
            None => {
                mark();
                Ok(true)
            }
            Some((s, m, _)) if (s, m) != (size, mtime) => {
                mark();
                Ok(true)
            }
            Some((_, _, None)) => Ok(false),
            Some((_, _, Some(err))) => {
                let transient = err.contains("os error 32") || err.contains("os error 33") || err.contains("utilisé par un autre processus");
                // Déjà tenté pendant ce lancement : on ne réessaie que si l'erreur est passagère.
                let first_retry = mark();
                Ok(transient || first_retry)
            }
        }
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Store> {
        // Un panic pendant une requête ne doit pas bloquer l'appli : on récupère le verrou.
        self.store.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn connectors(&self) -> &[Box<dyn SimulatorConnector>] {
        &self.connectors
    }

    pub fn settings(&self) -> AppSettings {
        self.db()
            .get_setting(SETTINGS_KEY)
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, mut s: AppSettings) -> Result<AppSettings> {
        if !(10..=120).contains(&s.sample_rate_hz) {
            return Err(ServiceError::User("fréquence d'échantillonnage : 10 à 120 Hz".into()));
        }
        if !matches!(s.units.as_str(), "metric" | "imperial") {
            return Err(ServiceError::User("unités : metric ou imperial".into()));
        }
        s.cloud_enabled = false;
        s.coach_min_interval_s = s.coach_min_interval_s.clamp(3, 60);
        self.db().set_setting(SETTINGS_KEY, &serde_json::to_string(&s).unwrap())?;
        Ok(s)
    }

    pub fn connector_config(&self, id: SimId) -> ConnectorConfig {
        let s = self.settings();
        ConnectorConfig {
            telemetry_dir: match id {
                SimId::Iracing => s.iracing_telemetry_dir.map(PathBuf::from),
                _ => None,
            },
            live_enabled: s.live_enabled,
            sample_rate_hz: s.sample_rate_hz,
            ..Default::default()
        }
    }

    pub fn connectors_info(&self) -> Vec<ConnectorInfo> {
        self.connectors
            .iter()
            .map(|c| {
                let cfg = self.connector_config(c.id());
                ConnectorInfo {
                    id: c.id(),
                    name: c.display_name().into(),
                    process_names: c.process_names().iter().map(|s| s.to_string()).collect(),
                    capabilities: c.capabilities(),
                    install: c.detect_installation(&cfg),
                    import_extensions: c.importer().map(|i| i.extensions().iter().map(|s| s.to_string()).collect()).unwrap_or_default(),
                    default_import_dir: c
                        .importer()
                        .and_then(|i| cfg.telemetry_dir.clone().or_else(|| i.default_dir()))
                        .map(|p| p.to_string_lossy().into_owned()),
                }
            })
            .collect()
    }

    pub fn list_sessions(&self, f: &SessionFilter) -> Result<Vec<SessionSummary>> {
        Ok(self.db().list_sessions(f)?)
    }

    pub fn import_paths(&self, paths: &[PathBuf]) -> Vec<ImportReport> {
        paths
            .iter()
            .map(|p| {
                let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
                let fname = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if ext == "csv" && crate::ingest::garage61::parse_file_name(&fname).is_some() {
                    let path = p.to_string_lossy().into_owned();
                    return match self.import_garage61(p) {
                        Ok(r) => ImportReport { path, outcome: Some(InsertOutcome::Inserted(r.session_id)), laps: 1, error: None },
                        Err(e) => ImportReport { path, outcome: None, laps: 0, error: Some(e.to_string()) },
                    };
                }
                let importer = self.connectors.iter().filter_map(|c| c.importer()).find(|i| i.extensions().contains(&ext.as_str()));
                let path = p.to_string_lossy().into_owned();
                let Some(imp) = importer else {
                    return ImportReport { path, outcome: None, laps: 0, error: Some(format!("format .{ext} non pris en charge")) };
                };
                match imp.import_file(p) {
                    Ok(s) => match self.db().insert_session(&s) {
                        Ok(o) => ImportReport { path, outcome: Some(o), laps: s.laps.len(), error: None },
                        Err(e) => ImportReport { path, outcome: None, laps: 0, error: Some(e.to_string()) },
                    },
                    Err(e) => ImportReport { path, outcome: None, laps: 0, error: Some(e.to_string()) },
                }
            })
            .collect()
    }

    /// Dossier de télémétrie iRacing (réglage, sinon emplacement par défaut).
    pub fn iracing_dir(&self) -> Result<PathBuf> {
        let c = self.connectors.iter().find(|c| c.id() == SimId::Iracing).expect("connecteur iRacing");
        let imp = c.importer().expect("import iRacing");
        let dir = self
            .connector_config(SimId::Iracing)
            .telemetry_dir
            .or_else(|| imp.default_dir())
            .ok_or_else(|| ServiceError::User("dossier de télémétrie iRacing non configuré".into()))?;
        if !dir.exists() {
            return Err(ServiceError::User(format!("dossier introuvable : {}", dir.display())));
        }
        Ok(dir)
    }

    fn ibt_files(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        let c = self.connectors.iter().find(|c| c.id() == SimId::Iracing).expect("connecteur iRacing");
        c.importer().expect("import iRacing").scan(dir).map_err(|e| ServiceError::User(e.to_string()))
    }

    /// Importe un tour exporté de Garage 61 comme référence externe et le définit comme référence
    /// personnelle pour ce circuit et cette voiture. Le circuit doit déjà exister dans tes sessions
    /// (sa longueur exacte sert à convertir la position en distance).
    pub fn import_garage61(&self, path: &Path) -> Result<ExternalImport> {
        use crate::ingest::garage61;
        let fname = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let name = garage61::parse_file_name(&fname)
            .ok_or_else(|| ServiceError::User("nom de fichier non reconnu : garde le nom donné par Garage 61 (« Garage 61 - pilote - voiture - circuit - temps - … .csv »)".into()))?;
        let text = std::fs::read_to_string(path).map_err(|e| ServiceError::User(format!("lecture du fichier : {e}")))?;
        self.import_garage61_text(name, &text, fname, None)
    }

    /// Importe un tour Garage 61 (contenu CSV + nom), quelle que soit sa provenance (fichier, API).
    /// `setup` : type de série du tour s'il est connu (recherche API filtrée) ; sinon à préciser par le pilote.
    pub fn import_garage61_text(&self, name: crate::ingest::garage61::G61Name, text: &str, fname: String, setup: Option<SetupType>) -> Result<ExternalImport> {
        use crate::ingest::garage61;
        let own = self.list_sessions(&SessionFilter { sim: Some(SimId::Iracing), ..Default::default() })?;
        let score = |s: &SessionSummary| garage61::track_match_score(&name.track, &s.track, s.track_config.as_deref());
        let same_car = |s: &SessionSummary| garage61::car_matches(&name.car, &s.car);
        // Meilleure correspondance de circuit, en préférant la même voiture ; à égalité, la plus récente.
        let best = own.iter().filter_map(|s| score(s).map(|sc| (sc, same_car(s), s))).max_by(|a, b| (a.1, a.0).cmp(&(b.1, b.0)).then(b.2.started_at_unix.cmp(&a.2.started_at_unix).reverse()));
        let m = match best {
            Some((_, _, s)) => s,
            None => {
                let mut known: Vec<String> = own.iter().map(|s| format!("{}{}", s.track, s.track_config.as_deref().map(|c| format!(" ({c})")).unwrap_or_default())).collect();
                known.sort();
                known.dedup();
                return Err(ServiceError::User(format!(
                    "le circuit du fichier, « {} », ne correspond à aucune de tes sessions iRacing ({}). Il faut avoir roulé au moins une session sur ce circuit (l'app a besoin de sa longueur exacte).",
                    name.track,
                    if known.is_empty() { "aucune session".into() } else { format!("circuits connus : {}", known.join(", ")) }
                )));
            }
        };
        let meta0 = self.db().session_meta(m.id)?;
        let length = meta0.track_length_m.ok_or_else(|| ServiceError::User("longueur du circuit inconnue".into()))?;
        let lap = garage61::parse_csv(text, length, name.lap_time_s).map_err(ServiceError::User)?;
        let car_matches = same_car(m);
        let meta = SessionMeta {
            sim: SimId::Iracing,
            track: meta0.track.clone(),
            track_config: meta0.track_config.clone(),
            track_length_m: Some(length),
            car: if car_matches { meta0.car.clone() } else { name.car.clone() },
            kind: SessionKind::Unknown,
            started_at_unix: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0),
            weather: Weather::default(),
            source_fingerprint: format!("garage61:{}", name.id),
            source_path: Some(fname.clone()),
            setup,
        };
        let lm = LapMeta {
            number: 1,
            kind: LapKind::Flying,
            lap_time_s: Some(name.lap_time_s),
            sector_times_s: vec![],
            flags: LapFlags { off_track: lap.off_track, ..Default::default() },
            validity: if lap.off_track { Validity::Invalid } else { Validity::Valid },
            validity_source: ValiditySource::Derived,
            fuel_start_l: None,
            fuel_used_l: None,
        };
        let s = ImportedSession { meta, laps: vec![ImportedLap { meta: lm, trace: lap.trace }], sample_rate_hz: Some(60.0), race: None, profile: None };
        let sid = match self.db().insert_session(&s)? {
            InsertOutcome::Inserted(id) | InsertOutcome::Duplicate(id) => id,
        };
        self.db().set_external_driver(sid, &name.driver)?;
        let lap_id = self.db().laps(sid)?.first().map(|l| l.id).ok_or_else(|| ServiceError::User("tour absent".into()))?;
        if car_matches {
            self.db().set_reference(lap_id)?;
        }
        Ok(ExternalImport { session_id: sid, lap_id, driver: name.driver, lap_time_s: name.lap_time_s, track: meta0.track, car_matches, set_as_reference: car_matches, setup })
    }

    fn g61_api(&self) -> Result<crate::garage61_api::G61Api> {
        let token = self.settings().garage61_token.filter(|t| !t.trim().is_empty()).ok_or_else(|| ServiceError::User("ajoute d'abord ton jeton Garage 61 dans les Réglages".into()))?;
        Ok(crate::garage61_api::G61Api::new(&token))
    }

    /// Vérifie le jeton Garage 61 et renvoie un résumé lisible du compte.
    pub fn garage61_check(&self) -> Result<String> {
        let me = self.g61_api()?.me().map_err(ServiceError::User)?;
        let name = format!("{} {}", me.first_name.unwrap_or_default(), me.last_name.unwrap_or_default());
        Ok(format!(
            "Connecté : {} (formule {}, {} équipe(s)).",
            if name.trim().is_empty() { me.slug } else { name.trim().to_string() },
            me.subscription_plan.unwrap_or_else(|| "inconnue".into()),
            me.teams.len()
        ))
    }

    /// Cherche sur Garage 61 le meilleur tour des pilotes que tu suis (et de tes coéquipiers) sur le
    /// circuit et la voiture de cette session, et l'importe comme référence s'il est plus rapide que
    /// la référence externe actuelle. Renvoie un message lisible.
    pub fn find_garage61_reference(&self, session_id: i64) -> Result<String> {
        self.find_garage61_reference_with(session_id, None, true)
    }

    fn find_garage61_reference_with(&self, session_id: i64, me: Option<&crate::garage61_api::G61User>, diagnose: bool) -> Result<String> {
        use crate::garage61_api::{driver_slug, lap_name, match_car, match_track};
        let api = self.g61_api()?;
        let extra: Vec<String> = self.settings().garage61_drivers.iter().filter_map(|d| driver_slug(d)).collect();
        let meta = self.db().session_meta(session_id)?;
        if meta.sim != SimId::Iracing {
            return Err(ServiceError::User("Garage 61 : seules les sessions iRacing sont prises en charge".into()));
        }
        let (track, car) = {
            let mut cat = self.g61_catalog.lock().unwrap_or_else(|p| p.into_inner());
            if cat.is_none() {
                *cat = Some((api.tracks().map_err(ServiceError::User)?, api.cars().map_err(ServiceError::User)?));
            }
            let (tracks, cars) = cat.as_ref().unwrap();
            (
                match_track(tracks, &meta.track, meta.track_config.as_deref()).cloned().ok_or_else(|| ServiceError::User(format!("circuit introuvable sur Garage 61 : {}", meta.track)))?,
                match_car(cars, &meta.car).cloned().ok_or_else(|| ServiceError::User(format!("voiture introuvable sur Garage 61 : {}", meta.car)))?,
            )
        };
        let me = match me {
            Some(m) => m.clone(),
            None => api.me().map_err(ServiceError::User)?,
        };
        let teams: Vec<String> = me.teams.iter().map(|t| t.slug.clone()).collect();
        let mut laps: Vec<_> = api
            .best_laps(track.id, car.id, &teams, &extra, meta.setup)
            .map_err(ServiceError::User)?
            .into_iter()
            .filter(|l| l.driver.as_ref().is_none_or(|d| d.slug != me.slug))
            .collect();
        laps.sort_by(|a, b| a.lap_time.total_cmp(&b.lap_time));
        if laps.is_empty() {
            let who = if extra.is_empty() { "des pilotes que tu suis".to_string() } else { format!("des pilotes que tu suis ni des {} pilote(s) ajouté(s)", extra.len()) };
            let series = meta.setup.map(|s| format!(" en série {}", s.label())).unwrap_or_default();
            let mut m = format!("aucun tour {who} n'est visible par l'API Garage 61 sur ce circuit avec cette voiture{series}.");
            if diagnose {
                // Un seul appel de contrôle (l'API gratuite limite fortement le nombre de requêtes).
                let mine = api.count_laps(track.id, Some(car.id), Some("me")).map(|v| v.to_string()).unwrap_or_else(|e| format!("erreur ({e})"));
                m += &format!(" Contrôle : tes propres tours visibles = {mine} (circuit n° {}, voiture n° {}).", track.id, car.id);
            }
            return Ok(m);
        }
        // Référence externe déjà présente sur ce combo : on ne la remplace que par plus rapide.
        let current = self
            .list_sessions(&SessionFilter { sim: Some(SimId::Iracing), track: Some(meta.track.clone()), external: Some(true), ..Default::default() })?
            .into_iter()
            .filter(|s| s.track_config == meta.track_config && s.car == meta.car)
            // Une référence open ne bloque pas la recherche d'une référence fixed (et inversement).
            .filter(|s| meta.setup.is_none() || s.setup == meta.setup)
            .filter_map(|s| s.best_lap_s)
            .fold(f64::INFINITY, f64::min);
        let mut last_err = None;
        for l in laps.iter().take(5) {
            if l.lap_time >= current - 0.001 {
                return Ok(format!("Ta référence externe actuelle ({}) est déjà la plus rapide disponible.", fmt_lap(current)));
            }
            match api.lap_csv(&l.id) {
                Ok(csv) => {
                    let name = lap_name(l);
                    let fname = format!("Garage 61 (API) - {} - {} - {} - {}", name.driver, name.car, name.track, name.id);
                    let r = self.import_garage61_text(name, &csv, fname, meta.setup)?;
                    let series = meta.setup.map(|s| format!(", série {}", s.label())).unwrap_or_default();
                    return Ok(format!("Nouvelle référence : {} en {} (Garage 61{series}).", r.driver, fmt_lap(r.lap_time_s)));
                }
                Err(e) => last_err = Some(e),
            }
        }
        Err(ServiceError::User(format!(
            "tours trouvés sur Garage 61 mais télémétrie non accessible ({}). Son accès peut demander l'abonnement Pro.",
            last_err.unwrap_or_default()
        )))
    }

    /// Cherche une référence Garage 61 pour chaque combinaison circuit/voiture déjà roulée
    /// (une recherche par combinaison, sur la session la plus récente). Renvoie un message par combinaison.
    pub fn find_garage61_references_all(&self) -> Result<Vec<String>> {
        self.g61_api()?;
        let sessions = self.list_sessions(&SessionFilter { sim: Some(SimId::Iracing), ..Default::default() })?;
        let me = self.g61_api()?.me().map_err(ServiceError::User)?;
        let mut seen = std::collections::HashSet::new();
        let mut out = vec![];
        // list_sessions est trié du plus récent au plus ancien.
        for s in sessions {
            if !seen.insert((s.track.clone(), s.track_config.clone(), s.car.clone(), s.setup)) {
                continue;
            }
            let label = format!("{} / {}{}", s.track, s.car, s.setup.map(|x| format!(" ({})", x.label())).unwrap_or_default());
            if !out.is_empty() {
                // Espacement des requêtes : l'API gratuite limite le débit.
                std::thread::sleep(std::time::Duration::from_millis(1500));
            }
            out.push(match self.find_garage61_reference_with(s.id, Some(&me), false) {
                Ok(m) => format!("{label} : {m}"),
                Err(e) => format!("{label} : {e}"),
            });
        }
        self.db().set_setting("garage61_last_full_scan", &now_secs().to_string())?;
        Ok(out)
    }

    /// Récupère depuis Garage 61 tes sessions des `days` derniers jours absentes d'Apex (fichiers .ibt
    /// effacés avant import, par exemple par l'agent Garage 61). Pour chaque circuit/voiture déjà roulé :
    /// liste de tes tours, regroupement en sessions, et téléchargement de la télémétrie des tours qui ne
    /// sont couverts par aucune session existante. Données Garage 61 : 60 Hz, sans carburant ni secteurs.
    pub fn recover_garage61_sessions(&self, days: i64) -> Result<RecoverReport> {
        use crate::garage61_api::{match_car, match_track, parse_iso_utc};
        let api = self.g61_api()?;
        let own = self.list_sessions(&SessionFilter { sim: Some(SimId::Iracing), ..Default::default() })?;
        {
            let mut cat = self.g61_catalog.lock().unwrap_or_else(|p| p.into_inner());
            if cat.is_none() {
                *cat = Some((api.tracks().map_err(ServiceError::User)?, api.cars().map_err(ServiceError::User)?));
            }
        }
        // Fenêtres déjà couvertes par tes sessions (heure locale) : début → fin du dernier tour + 5 min.
        let mut windows: std::collections::HashMap<ComboKey, Vec<(i64, i64)>> = Default::default();
        for s in &own {
            let total: f64 = self.db().laps(s.id)?.iter().filter_map(|l| l.meta.lap_time_s).sum();
            windows.entry((s.track.clone(), s.track_config.clone(), s.car.clone())).or_default().push((s.started_at_unix - 120, s.started_at_unix + total.ceil() as i64 + 300));
        }
        let now = now_secs();
        let after = {
            let t = now - days * 86400;
            let r = t.rem_euclid(86400);
            let (y, m, dd) = civil_date(t);
            format!("{y:04}-{m:02}-{dd:02}T{:02}:{:02}:00Z", r / 3600, r % 3600 / 60)
        };
        let mut rep = RecoverReport::default();
        let mut first_call = true;
        let pause = |first: &mut bool| {
            // Espacement des requêtes : l'API gratuite limite fortement le débit.
            if !std::mem::take(first) {
                std::thread::sleep(std::time::Duration::from_millis(1200));
            }
        };
        let mut combos: Vec<&SessionSummary> = vec![];
        for s in &own {
            if !combos.iter().any(|c| c.track == s.track && c.track_config == s.track_config && c.car == s.car) {
                combos.push(s);
            }
        }
        for c in combos {
            let label = format!("{} / {}", c.track, c.car);
            let (track, car) = {
                let cat = self.g61_catalog.lock().unwrap_or_else(|p| p.into_inner());
                let (tracks, cars) = cat.as_ref().unwrap();
                (match_track(tracks, &c.track, c.track_config.as_deref()).cloned(), match_car(cars, &c.car).cloned())
            };
            let (Some(track), Some(car)) = (track, car) else {
                rep.messages.push(format!("{label} : circuit ou voiture introuvable sur Garage 61"));
                continue;
            };
            pause(&mut first_call);
            let laps = match api.my_laps(track.id, car.id, &after, None) {
                Ok(v) => v,
                Err(e) => {
                    rep.messages.push(format!("{label} : {e}"));
                    if e.contains("trop de requêtes") || e.contains("jeton") {
                        break;
                    }
                    continue;
                }
            };
            let remote: Vec<RemoteLap> = laps
                .iter()
                .filter_map(|l| {
                    let utc = parse_iso_utc(l.start_time.as_deref()?)?;
                    Some(RemoteLap { id: l.id.clone(), start: utc + local_offset_at(utc), lap_time: l.lap_time, clean: l.clean, number: l.lap_number })
                })
                .filter(|l| l.lap_time > 0.0)
                .collect();
            let key = (c.track.clone(), c.track_config.clone(), c.car.clone());
            let covered = |t: i64| windows.get(&key).is_some_and(|w| w.iter().any(|(a, b)| t >= *a && t <= *b));
            let missing: Vec<Vec<RemoteLap>> = group_into_sessions(remote.clone(), 1200)
                .into_iter()
                .map(|g| g.into_iter().filter(|l| !covered(l.start)).collect::<Vec<_>>())
                .filter(|g| !g.is_empty())
                .collect();
            // Type de série (fixed / open) de chaque tour : deux requêtes filtrées, seulement s'il y a des
            // sessions à créer ou des sessions déjà récupérées dont le type est inconnu.
            let backfill: Vec<(i64, String)> = self
                .db()
                .garage61_sessions_without_setup()?
                .into_iter()
                .filter(|(_, _, t, cfg, cr)| *t == c.track && *cfg == c.track_config && *cr == c.car)
                .map(|(id, lap, ..)| (id, lap))
                .collect();
            let mut tags: std::collections::HashMap<String, SetupType> = Default::default();
            if !remote.is_empty() && (!missing.is_empty() || !backfill.is_empty()) {
                let mut ids = |st: SetupType| -> Option<std::collections::HashSet<String>> {
                    pause(&mut first_call);
                    api.my_laps(track.id, car.id, &after, Some(st)).ok().map(|v| v.into_iter().map(|l| l.id).collect())
                };
                if let (Some(fixed), Some(open)) = (ids(SetupType::Fixed), ids(SetupType::Open)) {
                    // Filtre ignoré par l'API (mêmes tours des deux côtés) : type laissé inconnu.
                    if fixed != open || fixed.is_empty() {
                        tags.extend(fixed.into_iter().map(|id| (id, SetupType::Fixed)));
                        tags.extend(open.into_iter().map(|id| (id, SetupType::Open)));
                    }
                }
            }
            for (sid, lap) in &backfill {
                if let Some(st) = tags.get(lap) {
                    self.db().set_session_setup(*sid, Some(*st))?;
                }
            }
            if missing.is_empty() {
                rep.messages.push(format!("{label} : {} tour(s) sur Garage 61, tous déjà dans Apex", remote.len()));
                continue;
            }
            let meta0 = self.db().session_meta(c.id)?;
            let Some(length) = meta0.track_length_m else { continue };
            let mut made = 0;
            let mut fail: Option<String> = None;
            for g in missing {
                let fetch = |id: &str| {
                    pause(&mut first_call);
                    api.lap_csv(id)
                };
                let setup = group_setup(&g, &tags);
                let session = match recovered_session(&meta0, length, setup, &g, fetch) {
                    Ok(Some(x)) => x,
                    Ok(None) => continue,
                    Err(e) => {
                        fail = Some(e);
                        break;
                    }
                };
                let n = session.laps.len();
                if let InsertOutcome::Inserted(id) = self.insert_imported(&session)? {
                    made += 1;
                    rep.sessions += 1;
                    rep.laps += n;
                    windows.entry(key.clone()).or_default().push((g[0].start - 120, g[0].start + session.laps.iter().filter_map(|l| l.meta.lap_time_s).sum::<f64>() as i64 + 300));
                    if let Err(e) = self.export_to_mobile_dir(id) {
                        log::warn!("copie pour le mobile : {e}");
                    }
                }
            }
            let mut m = format!("{label} : {made} session(s) récupérée(s)");
            if let Some(e) = fail {
                m += &format!(" — arrêt : {e}");
                rep.messages.push(m);
                if e.contains("trop de requêtes") {
                    rep.messages.push("Relance dans quelques minutes : les sessions déjà récupérées ne seront pas retéléchargées.".into());
                    break;
                }
                continue;
            }
            rep.messages.push(m);
        }
        if !rep.messages.iter().any(|m| m.contains("trop de requêtes") || m.contains("jeton")) {
            self.db().set_setting("garage61_last_recover", &now_secs().to_string())?;
        }
        Ok(rep)
    }

    /// Vrai si la dernière récupération automatique de tes sessions Garage 61 date de plus de `hours` heures.
    pub fn garage61_recover_due(&self, hours: i64) -> bool {
        let last = self.db().get_setting("garage61_last_recover").ok().flatten().and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
        now_secs() - last >= hours * 3600
    }

    /// Vrai si la dernière recherche complète Garage 61 date de plus de `hours` heures (ou n'a jamais eu lieu).
    pub fn garage61_full_scan_due(&self, hours: i64) -> bool {
        let last = self.db().get_setting("garage61_last_full_scan").ok().flatten().and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
        now_secs() - last >= hours * 3600
    }

    /// Dossier Téléchargements surveillé : réglage s'il est rempli, sinon le dossier Téléchargements
    /// réel de Windows (il peut avoir été déplacé, par exemple sur D:\Téléchargements).
    pub fn downloads_dir(&self) -> Option<PathBuf> {
        if let Some(d) = self.settings().downloads_dir.filter(|d| !d.trim().is_empty()) {
            let p = PathBuf::from(d.trim());
            return p.is_dir().then_some(p);
        }
        system_downloads_dir()
    }

    /// Importe les tours exportés de Garage 61 arrivés dans Téléchargements (une seule fois chacun).
    pub fn sync_downloads(&self) -> Result<Vec<ExternalImportReport>> {
        match self.downloads_dir() {
            Some(dir) => self.sync_downloads_in(&dir),
            None => Ok(vec![]),
        }
    }

    pub fn sync_downloads_in(&self, dir: &Path) -> Result<Vec<ExternalImportReport>> {
        let mut out = vec![];
        let entries = std::fs::read_dir(dir).map_err(|e| ServiceError::User(format!("Téléchargements : {e}")))?;
        for e in entries.flatten() {
            let p = e.path();
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if !name.to_ascii_lowercase().ends_with(".csv") || crate::ingest::garage61::parse_file_name(&name).is_none() {
                continue;
            }
            let Ok(meta) = std::fs::metadata(&p) else { continue };
            let size = meta.len() as i64;
            let mtime = meta.modified().ok().and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
            let key = p.to_string_lossy().into_owned();
            if !self.should_process(&key, size, mtime)? {
                continue;
            }
            let (sid, err) = match self.import_garage61(&p) {
                Ok(r) => {
                    out.push(ExternalImportReport { file: name, result: Ok(r.clone()) });
                    (Some(r.session_id), None)
                }
                Err(e) => {
                    out.push(ExternalImportReport { file: name, result: Err(e.to_string()) });
                    (None, Some(e.to_string()))
                }
            };
            self.db().record_file(&key, size, mtime, sid, err.as_deref())?;
        }
        Ok(out)
    }

    /// Tous les circuits roulés (circuit + configuration + voiture), du plus récent au plus ancien.
    pub fn circuits(&self) -> Result<Vec<CircuitOverview>> {
        let sessions = self.list_sessions(&SessionFilter::default())?;
        let externals = self.list_sessions(&SessionFilter { external: Some(true), ..Default::default() })?;
        type CircuitKey<'a> = (String, String, Option<String>, String, Option<&'a str>);
        let mut map: std::collections::BTreeMap<CircuitKey, Vec<&SessionSummary>> = Default::default();
        for s in &sessions {
            map.entry((s.sim.as_str().to_string(), s.track.clone(), s.track_config.clone(), s.car.clone(), s.setup.map(|x| x.as_str()))).or_default().push(s);
        }
        let db = self.db();
        let mut out = vec![];
        for ((_, track, config, car, _), ss) in map {
            let first = ss[0];
            let meta = db.session_meta(first.id)?;
            let mut laps = 0;
            let mut timed = 0;
            let mut clean = 0;
            let mut best_clean: Option<f64> = None;
            let mut best_any: Option<f64> = None;
            for s in &ss {
                for l in db.laps(s.id)? {
                    laps += 1;
                    if l.meta.kind == LapKind::Flying {
                        if let Some(t) = l.meta.lap_time_s {
                            timed += 1;
                            best_any = Some(best_any.map_or(t, |b| b.min(t)));
                            if l.clean {
                                clean += 1;
                                best_clean = Some(best_clean.map_or(t, |b| b.min(t)));
                            }
                        }
                    }
                }
            }
            let names = if !crate::tracks::builtin_corner_names(&meta).is_empty() {
                "exact"
            } else if crate::tracks::has_geometry(&meta) {
                "aligned"
            } else if db
                .laps(first.id)?
                .into_iter()
                .find(|l| l.meta.kind == LapKind::Flying && l.meta.lap_time_s.is_some())
                .and_then(|l| db.trace(l.id).ok())
                .and_then(|t| self.bank_for(&db, &meta, &t))
                .is_some()
            {
                "bank"
            } else {
                "none"
            };
            let ext = externals
                .iter()
                .filter(|e| e.sim == first.sim && e.track == track && e.track_config == config && e.car == car && SetupType::compatible(e.setup, first.setup))
                .filter_map(|e| Some((e.external_driver.clone()?, e.best_lap_s?)))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            out.push(CircuitOverview {
                setup: first.setup,
                sim: first.sim,
                track,
                track_config: config,
                car,
                track_length_m: meta.track_length_m,
                sessions: ss.len(),
                laps,
                timed_laps: timed,
                clean_laps: clean,
                best_clean_s: best_clean,
                best_any_s: best_any,
                first_unix: ss.iter().map(|s| s.started_at_unix).min().unwrap_or(0),
                last_unix: ss.iter().map(|s| s.started_at_unix).max().unwrap_or(0),
                corner_names: names.into(),
                external_reference: ext,
            });
        }
        out.sort_by_key(|c| std::cmp::Reverse(c.last_unix));
        Ok(out)
    }

    /// Tous les tours d'un circuit (circuit + configuration + voiture), du plus ancien au plus récent.
    /// `setup` : type de série (les sessions de type inconnu forment un groupe à part).
    pub fn lap_history(&self, sim: SimId, track: &str, track_config: Option<&str>, car: &str, setup: Option<SetupType>) -> Result<Vec<HistoryLap>> {
        let mut sessions: Vec<SessionSummary> = self
            .list_sessions(&SessionFilter { sim: Some(sim), track: Some(track.to_string()), car: Some(car.to_string()), ..Default::default() })?
            .into_iter()
            .filter(|s| s.track_config.as_deref() == track_config && s.setup == setup)
            .collect();
        sessions.sort_by_key(|s| s.started_at_unix);
        let db = self.db();
        let mut out = vec![];
        for s in &sessions {
            for l in db.laps(s.id)? {
                out.push(HistoryLap {
                    lap_id: l.id,
                    session_id: s.id,
                    session_kind: s.kind,
                    started_at_unix: s.started_at_unix,
                    number: l.meta.number,
                    kind: l.meta.kind,
                    lap_time_s: l.meta.lap_time_s,
                    clean: l.clean,
                    off_track: l.meta.flags.off_track,
                    incident: l.meta.flags.incident,
                    is_reference: l.is_reference,
                    delta_to_best_s: None,
                    personal_best_at_time: false,
                });
            }
        }
        let best = out.iter().filter(|l| l.clean && l.kind == LapKind::Flying).filter_map(|l| l.lap_time_s).fold(f64::INFINITY, f64::min);
        let mut running = f64::INFINITY;
        for l in &mut out {
            if l.kind == LapKind::Flying {
                if let Some(t) = l.lap_time_s {
                    if best.is_finite() {
                        l.delta_to_best_s = Some(t - best);
                    }
                    if l.clean && t < running {
                        running = t;
                        l.personal_best_at_time = true;
                    }
                }
            }
        }
        Ok(out)
    }

    /// Recalcule les sessions décodées par une ancienne version du décodeur, depuis leur
    /// fichier `.ibt` s'il existe encore. Renvoie le nombre de sessions recalculées.
    pub fn reprocess_outdated(&self) -> Result<usize> {
        let list = self.db().outdated_sessions()?;
        let mut n = 0;
        for (id, path) in list {
            let p = Path::new(&path);
            if !p.exists() {
                self.db().mark_parsed(id)?;
                continue;
            }
            match crate::ingest::ibt::import_ibt_file(p) {
                Ok(s) => {
                    self.db().replace_laps(id, &s)?;
                    n += 1;
                }
                Err(e) => {
                    log::warn!("recalcul de la session {id} impossible : {e}");
                    self.db().mark_parsed(id)?;
                }
            }
        }
        Ok(n)
    }

    /// Import manuel de tout le dossier iRacing (les doublons sont ignorés).
    pub fn scan_iracing(&self) -> Result<Vec<ImportReport>> {
        let dir = self.iracing_dir()?;
        let files = self.ibt_files(&dir)?;
        Ok(self.import_paths(&files))
    }

    /// Synchro automatique : importe chaque `.ibt` nouveau ou modifié, une fois terminé.
    ///
    /// Un fichier est considéré terminé quand il n'a pas été modifié depuis `min_age`
    /// (iRacing l'écrit en continu pendant la session). Les fichiers déjà traités
    /// (même taille, même date) ne sont jamais reparsés, y compris ceux en erreur.
    pub fn sync_iracing_folder(&self, min_age: std::time::Duration) -> Result<SyncReport> {
        let dir = self.iracing_dir()?;
        let now = std::time::SystemTime::now();
        let mut rep = SyncReport::default();
        // (fichier à lire, clé = chemin d'origine dans le dossier iRacing, copie de garde éventuelle)
        let mut todo: Vec<(PathBuf, String, Option<PathBuf>)> = vec![];
        for p in self.ibt_files(&dir)? {
            let guard = self.guard_path(&p).filter(|g| g.exists());
            todo.push((p.clone(), p.to_string_lossy().into_owned(), guard));
        }
        // Fichiers effacés du dossier iRacing par un autre logiciel : on importe la copie de garde.
        if let Some(keep) = &self.keep_dir {
            for g in self.ibt_files(keep).unwrap_or_default() {
                let Some(name) = g.file_name() else { continue };
                let orig = dir.join(name);
                if !orig.exists() {
                    todo.push((g.clone(), orig.to_string_lossy().into_owned(), None));
                }
            }
        }
        for (p, key, guard) in todo {
            let Ok(meta) = std::fs::metadata(&p) else { continue };
            let Ok(modified) = meta.modified() else { continue };
            let size = meta.len() as i64;
            let mtime = modified.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
            if now.duration_since(modified).unwrap_or_default() < min_age {
                rep.pending += 1;
                continue;
            }
            if !self.should_process(&key, size, mtime)? {
                // Déjà importé et toujours présent dans le dossier iRacing : la garde ne sert plus.
                if let (Some(g), Ok(Some((_, _, None)))) = (&guard, self.db().file_record(&key)) {
                    let _ = std::fs::remove_file(g);
                }
                continue;
            }
            let r = self.import_paths(std::slice::from_ref(&p)).remove(0);
            let sid = match r.outcome {
                Some(InsertOutcome::Inserted(id)) => {
                    rep.imported += 1;
                    rep.new_sessions.push(id);
                    if let Err(e) = self.export_to_mobile_dir(id) {
                        rep.errors.push(format!("copie pour le mobile : {e}"));
                    }
                    Some(id)
                }
                Some(InsertOutcome::Duplicate(id)) => {
                    rep.duplicates += 1;
                    Some(id)
                }
                None => None,
            };
            if let Some(e) = &r.error {
                rep.errors.push(format!("{} : {e}", p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
            }
            self.db().record_file(&key, size, mtime, sid, r.error.as_deref())?;
            // L'original est toujours là : la copie de garde est supprimée. Si l'original a été effacé,
            // la garde est conservée (seule copie brute, utile pour recalculer la session plus tard).
            if let (Some(g), Some(_), None) = (&guard, sid, &r.error) {
                let _ = std::fs::remove_file(g);
            }
        }
        Ok(rep)
    }

    fn guard_path(&self, p: &Path) -> Option<PathBuf> {
        Some(self.keep_dir.as_ref()?.join(p.file_name()?))
    }

    /// Pose une copie de garde (lien physique, aucune place disque en plus) sur chaque .ibt du dossier
    /// iRacing pas encore importé, dès qu'il apparaît, pendant qu'iRacing l'écrit. Si un autre logiciel
    /// efface le fichier juste après la session, les données restent accessibles par la garde.
    /// À appeler souvent (boucle de surveillance) : coût négligeable une fois les fichiers connus.
    pub fn guard_iracing_files(&self) -> Result<usize> {
        let Some(keep) = &self.keep_dir else { return Ok(0) };
        let dir = self.iracing_dir()?;
        let mut n = 0;
        for p in self.ibt_files(&dir)? {
            let key = p.to_string_lossy().into_owned();
            if self.guarded.lock().unwrap_or_else(|e| e.into_inner()).contains(&key) {
                continue;
            }
            let Ok(meta) = std::fs::metadata(&p) else { continue };
            let mtime = meta.modified().ok().and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
            let done = matches!(self.db().file_record(&key)?, Some((s, m, None)) if s == meta.len() as i64 && m == mtime);
            if !done {
                let Some(g) = self.guard_path(&p) else { continue };
                if !g.exists() {
                    std::fs::create_dir_all(keep).map_err(|e| ServiceError::User(format!("dossier de garde : {e}")))?;
                    if let Err(e) = std::fs::hard_link(&p, &g) {
                        // Autre disque que les données d'Apex, ou fichier pas encore accessible : on réessaie au prochain tour.
                        log::debug!("garde de {} impossible : {e}", p.display());
                        continue;
                    }
                    n += 1;
                }
            }
            self.guarded.lock().unwrap_or_else(|e| e.into_inner()).insert(key);
        }
        Ok(n)
    }

    fn iracing_app_ini(&self) -> Result<PathBuf> {
        let tdir = self.connector_config(SimId::Iracing).telemetry_dir;
        crate::connectors::iracing::app_ini_candidates(tdir.as_deref())
            .into_iter()
            .find(|p| p.exists())
            .ok_or_else(|| ServiceError::User("fichier app.ini d'iRacing introuvable (iRacing installé et lancé au moins une fois ?)".into()))
    }

    /// État de l'enregistrement automatique des sessions iRacing.
    pub fn iracing_autolog(&self) -> Result<AutoLogStatus> {
        let path = self.iracing_app_ini()?;
        let bytes = std::fs::read(&path).map_err(|e| ServiceError::User(format!("lecture de app.ini : {e}")))?;
        Ok(AutoLogStatus { enabled: crate::connectors::iracing::read_autolog(&bytes), path: path.to_string_lossy().into_owned() })
    }

    /// Active/désactive l'enregistrement automatique. Refusé si iRacing tourne
    /// (il réécrit app.ini en quittant). Sauvegarde le fichier d'origine une fois.
    pub fn set_iracing_autolog(&self, enable: bool, iracing_running: bool) -> Result<AutoLogStatus> {
        if iracing_running {
            return Err(ServiceError::User("ferme iRacing d'abord : il réécrit son fichier de réglages en quittant et annulerait la modification".into()));
        }
        let path = self.iracing_app_ini()?;
        let bytes = std::fs::read(&path).map_err(|e| ServiceError::User(format!("lecture de app.ini : {e}")))?;
        let out = crate::connectors::iracing::write_autolog(&bytes, enable).map_err(ServiceError::User)?;
        let backup = path.with_extension("ini.apex-backup");
        if !backup.exists() {
            std::fs::write(&backup, &bytes).map_err(|e| ServiceError::User(format!("sauvegarde de app.ini : {e}")))?;
        }
        std::fs::write(&path, &out).map_err(|e| ServiceError::User(format!("écriture de app.ini : {e}")))?;
        log::info!("irsdkAutoLogDisk={} dans {}", enable as u8, path.display());
        self.iracing_autolog()
    }

    /// Insère une session déjà décodée (outils, tests).
    pub fn insert_imported(&self, s: &ImportedSession) -> Result<InsertOutcome> {
        Ok(self.db().insert_session(s)?)
    }

    pub fn load_demo(&self) -> Result<InsertOutcome> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        let n = self.db().list_sessions(&SessionFilter { sim: Some(SimId::Demo), ..Default::default() })?.len() as u64;
        Ok(self.db().insert_session(&demo_session(1000 + n, now - 3600 * 24 * (3 - n.min(3) as i64)))?)
    }

    pub fn session_detail(&self, id: i64) -> Result<SessionDetail> {
        let db = self.db();
        let meta = db.session_meta(id)?;
        let laps = db.laps(id)?;
        let metas: Vec<LapMeta> = laps.iter().map(|l| l.meta.clone()).collect();
        let best = best_clean_lap(&metas);
        let traces: Vec<(LapMeta, LapTrace)> = laps
            .iter()
            .filter(|l| l.clean)
            .filter_map(|l| db.trace(l.id).ok().map(|t| (l.meta.clone(), t)))
            .collect();
        let pairs: Vec<(&LapMeta, &LapTrace)> = traces.iter().map(|(m, t)| (m, t)).collect();
        let length = meta.track_length_m.or_else(|| traces.first().map(|(_, t)| t.max_dist()));
        let theo = length.and_then(|l| theoretical_best(&pairs, l, 2.0)).map(|(t, _)| t);
        let personal = Self::personal_reference(&db, &meta)?;
        let fastest = laps
            .iter()
            .filter(|l| l.meta.kind == LapKind::Flying && l.meta.lap_time_s.is_some())
            .min_by(|a, b| a.meta.lap_time_s.unwrap().total_cmp(&b.meta.lap_time_s.unwrap()))
            .map(|l| l.id);
        let best_id = best.map(|i| laps[i].id);
        let analysis_ref = personal.or(best_id).or(fastest);

        // Sorties de piste, rattachées aux virages de la référence d'analyse.
        let mut spans = Vec::new();
        let mut ref_trace = None;
        for l in &laps {
            if let Ok(t) = db.trace(l.id) {
                let sp = crate::analysis::offtrack::off_track_spans(&t);
                if !sp.is_empty() {
                    spans.push((l.meta.number.max(0) as u32, sp));
                }
                if Some(l.id) == analysis_ref {
                    ref_trace = Some(t);
                }
            }
        }
        // Fenêtres de virage de la référence d'analyse (nom, entrée, apex, sortie).
        let names = self.names_for(&db, &meta, ref_trace.as_ref(), db.corner_names(&track_key(&meta))?);
        let windows: Vec<crate::analysis::risk::CornerWindow> = match (&ref_trace, length) {
            (Some(t), Some(len)) => crate::analysis::resample::resample(t, len, 1.0)
                .map(|r| {
                    let mut d = crate::analysis::corners::detect_corners(&r, &crate::analysis::corners::CornerConfig::default());
                    crate::analysis::compare::apply_corner_names(&mut d, &names);
                    let mut v: Vec<crate::analysis::risk::CornerWindow> = d.into_iter().map(|c| (c.name, c.entry_m, c.apex_m, c.exit_m)).collect();
                    // Virages nommés non détectés (pris à fond) : fenêtre de ±60 m autour de l'apex.
                    for (a, n) in &names {
                        if !v.iter().any(|c| &c.0 == n) {
                            v.push((n.clone(), a - 60.0, *a, a + 60.0));
                        }
                    }
                    v.sort_by(|a, b| a.2.total_cmp(&b.2));
                    v
                })
                .unwrap_or_default(),
            _ => names.iter().map(|(a, n)| (n.clone(), a - 60.0, *a, a + 60.0)).collect(),
        };
        let off_tracks = if spans.is_empty() {
            vec![]
        } else {
            let corners: Vec<(String, f32, f32)> = windows.iter().map(|(n, e, _, x)| (n.clone(), *e, *x)).collect();
            crate::analysis::offtrack::off_track_zones(&spans, &corners)
        };
        // Carte de risque : tous tes tours lancés sur ce circuit et cette voiture, toutes sessions confondues.
        let (risk, totals) = match (length, windows.is_empty()) {
            (Some(len), false) => {
                let combo = db.list_sessions(&SessionFilter { sim: Some(meta.sim), track: Some(meta.track.clone()), car: Some(meta.car.clone()), ..Default::default() })?;
                let combo: Vec<&SessionSummary> = combo.iter().filter(|s| s.track_config == meta.track_config).collect();
                let n_laps: usize = combo.iter().map(|s| s.lap_count).sum();
                let key = format!("{}|{}|{}|{:?}", track_key(&meta), meta.car, n_laps, analysis_ref);
                let cached = self.risk_cache.lock().unwrap_or_else(|p| p.into_inner()).as_ref().filter(|(k, _, _)| *k == key).map(|(_, r, t)| (r.clone(), *t));
                match cached {
                    Some(v) => v,
                    None => {
                        let mut traces: Vec<(i64, LapTrace)> = vec![];
                        for s in &combo {
                            for l in db.laps(s.id)? {
                                if l.meta.kind == LapKind::Flying && l.meta.lap_time_s.is_some() {
                                    if let Ok(t) = db.trace(l.id) {
                                        traces.push((s.id, t));
                                    }
                                }
                            }
                        }
                        let rl: Vec<crate::analysis::risk::RiskLap> = traces.iter().map(|(sid, t)| crate::analysis::risk::RiskLap { session_id: *sid, trace: t }).collect();
                        let r = crate::analysis::risk::corner_risk(&windows, &rl, len);
                        let t = crate::analysis::risk::lap_totals(&rl);
                        *self.risk_cache.lock().unwrap_or_else(|p| p.into_inner()) = Some((key, r.clone(), t));
                        (r, t)
                    }
                }
            }
            _ => (vec![], Default::default()),
        };
        let risk_headline = crate::analysis::risk::risk_headline(&risk, totals);
        let race = db.race_data(id)?.map(|rd| {
            let spots: Vec<(f32, String)> = windows.iter().map(|(n, _, a, _)| (*a, n.clone())).collect();
            crate::analysis::race::race_report(&rd, &metas, &spots, length.unwrap_or(0.0))
        });
        let reference_label = match personal.filter(|p| !laps.iter().any(|l| l.id == *p)) {
            Some(p) => {
                let l = db.lap(p)?;
                let time = l.meta.lap_time_s.map(fmt_lap).unwrap_or_default();
                let series = match db.session_meta(l.session_id)?.setup {
                    Some(st) => format!(", {}", st.label()),
                    None => ", série inconnue".into(),
                };
                Some(match db.external_driver(l.session_id)? {
                    Some(d) => format!("{d} (référence externe{series}), {time}"),
                    None => format!("Ta référence perso (autre session), {time}"),
                })
            }
            None => None,
        };
        let bank = ref_trace.as_ref().and_then(|t| self.bank_for(&db, &meta, t));
        let profile = match db.session_profile(id)? {
            Some(p) => Some(p),
            None => db.latest_profile(&meta.track, &meta.car)?,
        };
        Ok(SessionDetail {
            id,
            bank,
            profile,
            race,
            reference_label,
            risk,
            risk_headline,
            best_lap_id: best_id,
            theoretical_best_s: theo,
            consistency: consistency(&metas),
            personal_reference_lap_id: personal,
            fastest_lap_id: fastest,
            analysis_reference_lap_id: analysis_ref,
            off_tracks,
            meta,
            laps,
        })
    }

    /// Télémétrie brute d'un tour.
    pub fn lap_trace(&self, lap_id: i64) -> Result<LapTrace> {
        Ok(self.db().trace(lap_id)?)
    }

    /// Circuit et configuration reconnus dans la banque OpenStreetMap, d'après la position du circuit
    /// (fiche de session) et la forme de `reference`. Mémorisé par circuit.
    fn bank_for(&self, db: &Store, meta: &SessionMeta, reference: &LapTrace) -> Option<crate::trackdb::BankMatch> {
        let key = format!("{}|{}", meta.track, meta.track_config.clone().unwrap_or_default());
        if let Some(v) = self.bank_cache.lock().unwrap_or_else(|p| p.into_inner()).get(&key) {
            return v.clone();
        }
        let found = (|| {
            let p = db.latest_track_profile(&meta.track).ok()??;
            let (lat, lon, origin) = (p.track.lat?, p.track.lon?, p.track.origin?);
            let len = meta.track_length_m.filter(|l| *l > 100.0).unwrap_or_else(|| reference.max_dist());
            let r = crate::analysis::resample::resample(reference, len, 2.0).ok()?;
            let spine: Vec<[f32; 2]> = r.x?.into_iter().zip(r.y?).map(|(a, b)| [a, b]).collect();
            crate::trackdb::bank_match((lat, lon), origin, &spine)
        })();
        self.bank_cache.lock().unwrap_or_else(|p| p.into_inner()).insert(key, found.clone());
        found
    }

    /// Noms des virages : relevé exact, sinon noms placés par alignement sur la géométrie intégrée,
    /// sinon numéros d'après le vrai tracé (banque OpenStreetMap) ; noms du pilote par-dessus.
    fn names_for(&self, db: &Store, meta: &SessionMeta, reference: Option<&LapTrace>, user: Vec<(f32, String)>) -> Vec<(f32, String)> {
        let base = crate::tracks::corner_names_for(meta, reference, vec![]);
        if !base.is_empty() {
            return crate::tracks::merge_names(base, user);
        }
        let Some(r) = reference else { return user };
        let Some(bank) = self.bank_for(db, meta, r) else { return user };
        let numbered = (|| {
            let len = meta.track_length_m.filter(|l| *l > 100.0).unwrap_or_else(|| r.max_dist());
            let rs = crate::analysis::resample::resample(r, len, 1.0).ok()?;
            let xy: Vec<[f32; 2]> = rs.x.clone()?.into_iter().zip(rs.y.clone()?).map(|(a, b)| [a, b]).collect();
            let official = db.latest_track_profile(&meta.track).ok().flatten().and_then(|p| p.track.turns);
            let turns = crate::trackdb::bank_turns_on_lap(&bank, &xy, &rs.dist, official);
            let defs = crate::analysis::corners::detect_corners(&rs, &crate::analysis::corners::CornerConfig::default());
            // Chaque virage détecté prend le numéro du virage du tracé le plus proche (40 m au plus).
            Some(
                defs.iter()
                    .filter_map(|d| {
                        let (at, k) = turns.iter().min_by(|a, b| (a.0 - d.apex_m).abs().total_cmp(&(b.0 - d.apex_m).abs()))?;
                        ((at - d.apex_m).abs() <= 40.0).then(|| (d.apex_m, format!("Virage {k}")))
                    })
                    .collect::<Vec<_>>(),
            )
        })()
        .unwrap_or_default();
        crate::tracks::merge_names(numbered, user)
    }

    /// Relevé de la piste pour la carte : vibreurs, lignes, hors-piste et bords, à partir de tous tes
    /// tours (et des références) sur ce circuit et cette configuration, toutes voitures confondues.
    /// `spine_lap` : tour qui sert de trajectoire de référence pour les bords (celui de la carte).
    pub fn track_overlay(&self, session_id: i64, spine_lap: Option<i64>) -> Result<crate::analysis::surface::TrackOverlay> {
        use crate::analysis::surface::{combine_edges, surface_points, SurfaceLap, TrackOverlay};
        let db = self.db();
        let meta = db.session_meta(session_id)?;
        let f = SessionFilter { sim: Some(meta.sim), track: Some(meta.track.clone()), ..Default::default() };
        let ext = SessionFilter { external: Some(true), ..f.clone() };
        let all: Vec<SessionSummary> = db.list_sessions(&f)?.into_iter().chain(db.list_sessions(&ext)?).filter(|s| s.track_config == meta.track_config).collect();
        let key = format!("{}|{:?}|{:?}|{}|{}", meta.track, meta.track_config, spine_lap, all.len(), all.iter().map(|s| s.lap_count).sum::<usize>());
        if let Some((k, ov)) = self.overlay_cache.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            if *k == key {
                return Ok(ov.clone());
            }
        }
        let mut traces = vec![];
        let mut sessions = 0;
        for s in all {
            let before = traces.len();
            for l in db.laps(s.id)? {
                if l.meta.lap_time_s.is_none() {
                    continue;
                }
                if let Ok(t) = db.trace(l.id) {
                    if t.pos_x.is_some() {
                        traces.push(t);
                    }
                }
            }
            if traces.len() > before {
                sessions += 1;
            }
        }
        let laps: Vec<SurfaceLap> = traces.iter().map(|t| SurfaceLap { trace: t }).collect();
        let (kerbs, lines, off) = surface_points(&laps);
        // Trajectoire de référence : tour demandé, sinon meilleur tour de la session ; points tous les 2 m.
        let spine_id = spine_lap.or_else(|| db.laps(session_id).ok()?.into_iter().filter(|l| l.clean).min_by(|a, b| a.meta.lap_time_s.unwrap_or(f64::MAX).total_cmp(&b.meta.lap_time_s.unwrap_or(f64::MAX))).map(|l| l.id));
        let edges = spine_id.and_then(|id| {
            let t = db.trace(id).ok()?;
            let len = meta.track_length_m?;
            let r = crate::analysis::resample::resample(&t, len, 2.0).ok()?;
            let (x, y) = (r.x?, r.y?);
            let spine: Vec<[f32; 2]> = x.iter().zip(y.iter()).map(|(a, b)| [*a, *b]).collect();
            // Largeur mesurée (base TUM) si le circuit y est et que le tracé colle, sinon bords estimés.
            let measured = crate::trackdb::measured_edges(&meta.track, &spine);
            combine_edges(&spine, &laps, 3.min(laps.len()).max(1), measured.as_ref())
        });
        let ov = TrackOverlay { kerbs, lines, off, edges, laps: laps.len(), sessions };
        *self.overlay_cache.lock().unwrap_or_else(|p| p.into_inner()) = Some((key, ov.clone()));
        Ok(ov)
    }

    /// Voix naturelles disponibles : (identifiant, libellé, installée).
    pub fn tts_voices(&self) -> Vec<(String, String, bool)> {
        let st = self.settings();
        let azure_ready = st.azure_tts_key.as_deref().is_some_and(|k| !k.trim().is_empty()) && st.azure_tts_region.as_deref().is_some_and(|r| !r.trim().is_empty());
        crate::tts::AZURE_VOICES
            .iter()
            .map(|(id, _, label)| (id.to_string(), label.to_string(), azure_ready))
            .chain(crate::tts::VOICES.iter().map(|v| (v.id.to_string(), v.label.to_string(), self.tts.as_ref().is_some_and(|t| t.installed(v.id)))))
            .collect()
    }

    /// Télécharge le moteur vocal et la voix si besoin.
    pub fn tts_install(&self, id: &str) -> Result<()> {
        let t = self.tts.as_ref().ok_or_else(|| ServiceError::User("voix naturelles indisponibles".into()))?;
        t.install(id).map_err(|e| ServiceError::User(e.to_string()))
    }

    /// Message prononcé par une voix naturelle (fichier WAV).
    pub fn tts_speak(&self, id: &str, text: &str) -> Result<Vec<u8>> {
        if let Some((_, name, _)) = crate::tts::AZURE_VOICES.iter().find(|v| v.0 == id) {
            // Les phrases reviennent souvent (« Toujours virage 19… ») : cache en mémoire, quota préservé.
            let key = format!("{id}|{text}");
            if let Some(w) = self.tts_cache.lock().unwrap_or_else(|p| p.into_inner()).get(&key) {
                return Ok(w.clone());
            }
            let st = self.settings();
            let wav = crate::tts::azure_synthesize(st.azure_tts_key.as_deref().unwrap_or(""), st.azure_tts_region.as_deref().unwrap_or(""), name, text).map_err(|e| ServiceError::User(e.to_string()))?;
            let mut c = self.tts_cache.lock().unwrap_or_else(|p| p.into_inner());
            if c.len() > 300 {
                c.clear();
            }
            c.insert(key, wav.clone());
            return Ok(wav);
        }
        let t = self.tts.as_ref().ok_or_else(|| ServiceError::User("voix naturelles indisponibles".into()))?;
        t.synthesize(id, text).map_err(|e| ServiceError::User(e.to_string()))
    }

    pub fn voice_installed(&self) -> bool {
        self.voice.as_ref().is_some_and(|v| v.installed())
    }

    /// Télécharge la reconnaissance vocale (bibliothèque + modèle français) si besoin.
    pub fn voice_install(&self) -> Result<()> {
        let v = self.voice.as_ref().ok_or_else(|| ServiceError::User("commandes vocales indisponibles".into()))?;
        v.install().map_err(|e| ServiceError::User(e.to_string()))
    }

    /// Mots-clés reconnus dans un enregistrement 16 kHz.
    pub fn voice_recognize(&self, pcm16k: &[i16]) -> Result<String> {
        let v = self.voice.as_ref().ok_or_else(|| ServiceError::User("commandes vocales indisponibles".into()))?;
        v.recognize(pcm16k).map_err(|e| ServiceError::User(e.to_string()))
    }

    /// Noms de virages donnés par le pilote pour ce circuit.
    pub fn user_corner_names(&self, meta: &SessionMeta) -> Vec<(f32, String)> {
        self.db().corner_names(&track_key(meta)).unwrap_or_default()
    }

    /// Référence du coach en direct pour ce circuit et cette voiture : ta référence perso (externe ou
    /// à toi), sinon ton meilleur tour propre ici. `None` si tu n'as jamais roulé cette combinaison.
    pub fn live_reference(&self, meta: &SessionMeta) -> Result<Option<crate::live_coach::ReferenceLap>> {
        let db = self.db();
        let lap_id = match Self::personal_reference(&db, meta)? {
            Some(id) => Some(id),
            None => {
                let f = SessionFilter { sim: Some(meta.sim), track: Some(meta.track.clone()), car: Some(meta.car.clone()), ..Default::default() };
                // Meilleur tour propre du même type de série ; à défaut, d'une session de type inconnu.
                let mut best: [Option<(f64, i64)>; 2] = [None, None];
                for s in db.list_sessions(&f)?.into_iter().filter(|s| s.track_config == meta.track_config && SetupType::compatible(s.setup, meta.setup)) {
                    let slot = &mut best[usize::from(s.setup != meta.setup)];
                    for l in db.laps(s.id)? {
                        if let (true, Some(t)) = (l.clean, l.meta.lap_time_s) {
                            if slot.is_none_or(|(b, _)| t < b) {
                                *slot = Some((t, l.id));
                            }
                        }
                    }
                }
                best[0].or(best[1]).map(|b| b.1)
            }
        };
        let Some(id) = lap_id else { return Ok(None) };
        let l = db.lap(id)?;
        let trace = db.trace(id)?;
        let label = match db.external_driver(l.session_id)? {
            Some(d) => format!("{d}{}, {}", meta.setup.map(|s| format!(" en {}", s.label())).filter(|_| db.session_meta(l.session_id).is_ok_and(|m| m.setup == meta.setup)).unwrap_or_default(), spoken_lap(l.meta.lap_time_s)),
            None if l.is_reference => format!("ta référence perso, {}", spoken_lap(l.meta.lap_time_s)),
            None => format!("ton meilleur tour ici, {}", spoken_lap(l.meta.lap_time_s)),
        };
        let names = self.names_for(&db, meta, Some(&trace), db.corner_names(&track_key(meta))?);
        Ok(Some(crate::live_coach::ReferenceLap { label, meta: l.meta, trace, names }))
    }

    /// Référence perso (à toi ou externe) du circuit et de la voiture, pour le type de série de `meta` :
    /// d'abord une référence du même type (fixed / open), sinon une référence de type inconnu.
    /// Une référence de l'autre type n'est jamais utilisée (vitesses de pointe et freinages différents).
    fn personal_reference(db: &Store, meta: &SessionMeta) -> Result<Option<i64>> {
        let f = SessionFilter { sim: Some(meta.sim), track: Some(meta.track.clone()), car: Some(meta.car.clone()), ..Default::default() };
        let ext = SessionFilter { external: Some(true), ..f.clone() };
        let mut fallback = None;
        for s in db.list_sessions(&f)?.into_iter().chain(db.list_sessions(&ext)?) {
            if s.track_config != meta.track_config || !SetupType::compatible(s.setup, meta.setup) {
                continue;
            }
            // Seul un tour lancé complet peut servir de référence (un ancien choix sur un tour incomplet est ignoré).
            if let Some(l) = db.laps(s.id)?.into_iter().find(|l| l.is_reference && l.meta.kind == LapKind::Flying && l.meta.lap_time_s.is_some()) {
                if s.setup == meta.setup {
                    return Ok(Some(l.id));
                }
                fallback.get_or_insert(l.id);
            }
        }
        Ok(fallback)
    }

    pub fn compare(&self, ref_lap: i64, cmp_lap: i64, step_m: f32) -> Result<Comparison> {
        if !(0.5..=10.0).contains(&step_m) {
            return Err(ServiceError::User("pas de comparaison : 0,5 à 10 m".into()));
        }
        let db = self.db();
        let (lr, lc) = (db.lap(ref_lap)?, db.lap(cmp_lap)?);
        let (mr, mc) = (db.session_meta(lr.session_id)?, db.session_meta(lc.session_id)?);
        let (tr, tc) = (db.trace(ref_lap)?, db.trace(cmp_lap)?);
        let names = self.names_for(&db, &mr, Some(&tr), db.corner_names(&track_key(&mr))?);
        let external = db.external_driver(lr.session_id)?;
        let mut c = compare_laps_named(
            &LapInput { lap_id: ref_lap, session: &mr, lap: &lr.meta, trace: &tr },
            &LapInput { lap_id: cmp_lap, session: &mc, lap: &lc.meta, trace: &tc },
            step_m,
            &names,
        )
        .map_err(|e| ServiceError::User(format!("comparaison impossible : {e}")))?;
        // Carte de risque : un virage à sécuriser passe avant tout conseil de vitesse sur ce virage.
        if db.external_driver(lc.session_id)?.is_none() {
            drop(db);
            let detail = self.session_detail(lc.session_id)?;
            apply_risk_to_insights(&mut c, &detail.risk);
        }
        if let Some(d) = external {
            c.reference.driver = Some(d.clone());
            c.warnings.retain(|w| !w.starts_with("Météo inconnue"));
            let what = if mr.setup.is_some() { "Son carburant et la météo" } else { "Son type de série (fixed / open), son carburant et la météo" };
            c.warnings.insert(0, format!("Référence : tour de {d}. {what} ne sont pas connus : un écart peut aussi venir de là."));
        }
        if let Some(w) = setup_warning(mr.setup, mc.setup) {
            c.warnings.insert(0, w);
        }
        Ok(c)
    }

    /// Fixe l'objectif de session à partir d'une suggestion (conseil ou tableau de bord).
    pub fn create_goal(&self, session_id: i64, goal: &GoalSuggestion) -> Result<GoalView> {
        let meta = self.db().session_meta(session_id)?;
        let json = serde_json::to_string(&goal.metric).map_err(|e| ServiceError::User(e.to_string()))?;
        self.db().create_goal(&track_key(&meta), &goal.title, &json)?;
        self.active_goal()?.ok_or_else(|| ServiceError::User("objectif non enregistré".into()))
    }

    pub fn archive_goal(&self, id: i64) -> Result<()> {
        Ok(self.db().archive_goal(id)?)
    }

    /// Objectif actif et son évaluation sur la session la plus récente du même circuit roulée après sa création.
    pub fn active_goal(&self) -> Result<Option<GoalView>> {
        let Some((id, key, created, title, json)) = self.db().active_goal()? else { return Ok(None) };
        let metric: GoalMetric = serde_json::from_str(&json).map_err(|e| ServiceError::User(format!("objectif illisible : {e}")))?;
        let sessions = self.list_sessions(&SessionFilter { from_unix: Some(created), ..Default::default() })?;
        let mut result = None;
        let mut evaluated = None;
        for s in sessions {
            let meta = self.db().session_meta(s.id)?;
            if track_key(&meta) != key {
                continue;
            }
            let db = self.db();
            let rows = db.laps(s.id)?;
            let traces: Vec<(LapMeta, LapTrace)> = rows.iter().filter_map(|r| db.trace(r.id).ok().map(|t| (r.meta.clone(), t))).collect();
            drop(db);
            let pairs: Vec<(&LapMeta, &LapTrace)> = traces.iter().map(|(m, t)| (m, t)).collect();
            let length = meta.track_length_m.or_else(|| traces.iter().map(|(_, t)| t.max_dist()).reduce(f32::max)).unwrap_or(0.0);
            result = Some(evaluate_goal(&metric, &pairs, length));
            evaluated = Some(s.id);
            break; // list_sessions est trié du plus récent au plus ancien
        }
        Ok(Some(GoalView { id, title, track_key: key, created_at_unix: created, result, evaluated_session_id: evaluated }))
    }

    /// Nomme un virage pour tout le circuit (toutes les sessions du même combo).
    pub fn set_corner_name(&self, session_id: i64, apex_m: f32, name: &str) -> Result<()> {
        if name.chars().count() > 40 {
            return Err(ServiceError::User("nom de virage trop long (40 caractères max)".into()));
        }
        let meta = self.db().session_meta(session_id)?;
        Ok(self.db().set_corner_name(&track_key(&meta), apex_m, name)?)
    }

    pub fn set_reference(&self, lap_id: i64) -> Result<()> {
        let l = self.db().lap(lap_id)?;
        if l.meta.kind != LapKind::Flying || l.meta.lap_time_s.is_none() {
            return Err(ServiceError::User("seul un tour lancé complet peut servir de référence (pas un tour de sortie, d'entrée ou interrompu)".into()));
        }
        Ok(self.db().set_reference(lap_id)?)
    }

    /// Précise le type de série (fixed / open) d'une session, typiquement une référence Garage 61 importée
    /// depuis un fichier .csv (le fichier ne le dit pas).
    pub fn set_session_setup(&self, id: i64, setup: Option<SetupType>) -> Result<()> {
        self.db().set_session_setup(id, setup)?;
        *self.risk_cache.lock().unwrap_or_else(|p| p.into_inner()) = None;
        Ok(())
    }

    pub fn delete_session(&self, id: i64) -> Result<()> {
        Ok(self.db().delete_session(id)?)
    }

    pub fn delete_all(&self) -> Result<()> {
        Ok(self.db().delete_all()?)
    }

    /// Pertes récurrentes des tours propres d'une séance face à son meilleur tour.
    fn zones_for(&self, detail: &SessionDetail) -> Result<Vec<ProgressZone>> {
        let db = self.db();
        let user_names = db.corner_names(&track_key(&detail.meta))?;
        let traces: Vec<(i64, LapMeta, LapTrace)> = detail
            .laps
            .iter()
            .filter(|l| l.clean)
            .filter_map(|l| db.trace(l.id).ok().map(|t| (l.id, l.meta.clone(), t)))
            .collect();
        let names = detail
            .best_lap_id
            .and_then(|b| traces.iter().find(|t| t.0 == b))
            .map(|t| self.names_for(&db, &detail.meta, Some(&t.2), user_names.clone()))
            .unwrap_or_default();
        drop(db);
        let laps: Vec<SessionLap> = traces.iter().map(|(id, m, t)| SessionLap { lap_id: *id, meta: m, trace: t }).collect();
        Ok(match detail.best_lap_id.and_then(|b| laps.iter().position(|l| l.lap_id == b)) {
            Some(bi) => {
                progress_zones_named(&detail.meta, &laps[bi], &laps, 2.0, &names)
            }
            None => vec![],
        })
    }

    /// Programme d'entraînement de ce circuit et de cette voiture (créé à partir de la dernière séance
    /// s'il n'y en a pas), avec l'évaluation de chaque objectif sur la séance roulée après sa création.
    pub fn training_plan(&self, session_id: i64) -> Result<PlanView> {
        let meta = self.db().session_meta(session_id)?;
        let key = track_key(&meta);
        if self.db().active_plan(&key, &meta.car)?.is_none() {
            self.generate_plan(&meta)?;
        }
        self.plan_view(&meta)
    }

    /// Nouveau programme à partir de la dernière séance (le précédent est archivé).
    pub fn new_training_plan(&self, session_id: i64) -> Result<PlanView> {
        let meta = self.db().session_meta(session_id)?;
        self.generate_plan(&meta)?;
        self.plan_view(&meta)
    }

    fn combo_sessions(&self, meta: &SessionMeta) -> Result<Vec<SessionSummary>> {
        Ok(self
            .list_sessions(&SessionFilter { sim: Some(meta.sim), track: Some(meta.track.clone()), car: Some(meta.car.clone()), ..Default::default() })?
            .into_iter()
            .filter(|s| s.track_config == meta.track_config)
            .collect())
    }

    fn generate_plan(&self, meta: &SessionMeta) -> Result<()> {
        let combo = self.combo_sessions(meta)?;
        let last = combo.first().ok_or_else(|| ServiceError::User("aucune séance sur ce circuit avec cette voiture".into()))?;
        let detail = self.session_detail(last.id)?;
        let zones = self.zones_for(&detail)?;
        let timed = detail.laps.iter().filter(|l| l.meta.kind == LapKind::Flying && l.meta.lap_time_s.is_some()).count();
        let clean = detail.laps.iter().filter(|l| l.clean).count();
        let objectives = crate::analysis::plan::build_plan(&crate::analysis::plan::PlanInput { risk: &detail.risk, zones: &zones, timed_laps: timed, clean_laps: clean, stddev_s: last.stddev_clean_s });
        let json = serde_json::to_string(&objectives).map_err(|e| ServiceError::User(e.to_string()))?;
        self.db().create_plan(&track_key(meta), &meta.car, last.id, &json)?;
        Ok(())
    }

    fn plan_view(&self, meta: &SessionMeta) -> Result<PlanView> {
        let (id, created, source, json) = self.db().active_plan(&track_key(meta), &meta.car)?.ok_or_else(|| ServiceError::User("pas de programme".into()))?;
        let objectives: Vec<crate::analysis::plan::PlanObjective> = serde_json::from_str(&json).map_err(|e| ServiceError::User(format!("programme illisible : {e}")))?;
        // Séance évaluée : la première roulée après la création du programme.
        // Importée après la séance source du programme ET roulée après sa création (à un fuseau horaire
        // près : iRacing horodate en heure locale) : une vieille séance récupérée de Garage 61 ne compte pas.
        let after: Vec<SessionSummary> = self.combo_sessions(meta)?.into_iter().filter(|s| s.id > source && s.started_at_unix >= created - 14 * 3600).collect();
        let evaluated = after.last().cloned();
        let results: Vec<Option<GoalResult>> = match &evaluated {
            Some(s) => {
                let m = self.db().session_meta(s.id)?;
                let db = self.db();
                let rows = db.laps(s.id)?;
                let traces: Vec<(LapMeta, LapTrace)> = rows.iter().filter_map(|r| db.trace(r.id).ok().map(|t| (r.meta.clone(), t))).collect();
                drop(db);
                let pairs: Vec<(&LapMeta, &LapTrace)> = traces.iter().map(|(m, t)| (m, t)).collect();
                let length = m.track_length_m.or_else(|| traces.iter().map(|(_, t)| t.max_dist()).reduce(f32::max)).unwrap_or(0.0);
                objectives.iter().map(|o| Some(evaluate_goal(&o.metric, &pairs, length))).collect()
            }
            None => objectives.iter().map(|_| None).collect(),
        };
        Ok(PlanView {
            id,
            created_at_unix: created,
            track: meta.track.clone(),
            car: meta.car.clone(),
            spoken: crate::analysis::plan::spoken_plan(&objectives),
            objectives: objectives.into_iter().zip(results).map(|(objective, result)| PlanItem { objective, result }).collect(),
            evaluated_session_id: evaluated.map(|s| s.id),
        })
    }

    /// Ajoute un conseil au programme de ce circuit (bouton « Ajouter au programme »).
    pub fn add_plan_objective(&self, session_id: i64, goal: &GoalSuggestion) -> Result<PlanView> {
        let meta = self.db().session_meta(session_id)?;
        let key = track_key(&meta);
        if self.db().active_plan(&key, &meta.car)?.is_none() {
            self.generate_plan(&meta)?;
        }
        let (_, _, source, json) = self.db().active_plan(&key, &meta.car)?.ok_or_else(|| ServiceError::User("pas de programme".into()))?;
        let objectives: Vec<crate::analysis::plan::PlanObjective> = serde_json::from_str(&json).map_err(|e| ServiceError::User(e.to_string()))?;
        let objectives = crate::analysis::plan::add_objective(objectives, goal.clone(), "Ajouté depuis un conseil de l'analyse.".into());
        let json = serde_json::to_string(&objectives).map_err(|e| ServiceError::User(e.to_string()))?;
        self.db().create_plan(&key, &meta.car, source, &json)?;
        self.plan_view(&meta)
    }

    /// Programme actif pour le coach en direct : texte radio et virages à privilégier (sans en créer).
    pub fn live_plan(&self, meta: &SessionMeta) -> Option<(String, Vec<f32>)> {
        let (_, _, _, json) = self.db().active_plan(&track_key(meta), &meta.car).ok()??;
        let objectives: Vec<crate::analysis::plan::PlanObjective> = serde_json::from_str(&json).ok()?;
        Some((crate::analysis::plan::spoken_plan(&objectives), objectives.iter().filter_map(|o| o.apex_m).collect()))
    }

    pub fn dashboard(&self) -> Result<Dashboard> {
        let all = self.list_sessions(&SessionFilter::default())?;
        let Some(last) = all.first().cloned() else {
            return Ok(Dashboard { active_goal: None, next_goal_suggestion: None, last_session: None, theoretical_best_s: None, progress_zones: vec![], consistency_trend: vec![], next_goal: None, total_sessions: 0 });
        };
        let detail = self.session_detail(last.id)?;
        let zones = self.zones_for(&detail)?;
        let mut trend: Vec<(i64, f64)> = all
            .iter()
            .filter(|s| s.sim == last.sim && s.track == last.track && s.car == last.car)
            .filter_map(|s| s.stddev_clean_s.map(|d| (s.started_at_unix, d)))
            .collect();
        trend.sort_by_key(|t| t.0);
        let beginner = self.settings().beginner_mode;
        let flying = last.lap_count.saturating_sub(2).max(1);
        // Mode débutant : rester sur la piste, puis être régulier, avant de chercher les dixièmes.
        let mut suggestion = None;
        let beginner_goal = if !beginner {
            None
        } else if last.incident_laps * 3 >= flying && last.incident_laps >= 2 {
            suggestion = Some(GoalSuggestion { title: "Enchaîner 5 tours propres".into(), metric: GoalMetric::CleanLaps { target: 5 } });
            Some(format!(
                "Priorité : rester sur la piste. {} tour(s) sur {} ont eu une sortie ou un incident. Roule à 95 % de ton rythme jusqu'à enchaîner 5 tours propres : un tour propre et régulier fait plus progresser qu'un tour rapide raté.",
                last.incident_laps, last.lap_count
            ))
        } else if let (Some(sd), Some(best)) = (last.stddev_clean_s, last.best_lap_s) {
            if sd > 0.5 {
                suggestion = Some(GoalSuggestion { title: "Régularité : écart-type ≤ 0,5 s".into(), metric: GoalMetric::Consistency { target_sd_s: 0.5 } });
            }
            (sd > 0.5).then(|| format!(
                "Priorité : la régularité. Tes tours propres varient de ±{:.1} s. Objectif : 5 tours d'affilée à moins de 0,5 s de ton meilleur tour ({}), avec les mêmes points de freinage à chaque tour.",
                sd, fmt_lap(best)
            ))
        } else {
            None
        };
        // Carte de risque : un virage à sécuriser prime sur toute zone de progrès chronométrique.
        let risk_goal = detail.risk.iter().find(|r| matches!(r.verdict, crate::analysis::risk::RiskVerdict::Secure | crate::analysis::risk::RiskVerdict::Cuts)).map(|r| {
            suggestion = Some(GoalSuggestion {
                title: format!("{} : 90 % de passages sans sortie", r.name),
                metric: GoalMetric::CornerClean { apex_m: r.apex_m, target_rate: 0.9 },
            });
            format!("{} : {}", r.name, r.message)
        });
        let next_goal = risk_goal.or(beginner_goal).or(match (zones.first(), last.mean_clean_s) {
            (Some(z), Some(mean)) => {
                // Les pertes sont mesurées contre le meilleur tour : corriger ce défaut
                // fait baisser la moyenne, pas le meilleur tour lui-même.
                let target = mean - z.avg_loss_s as f64 * z.occurrences as f64 / z.laps_compared.max(1) as f64;
                Some(format!(
                    "{} : {} sur {}/{} tours propres (≈{:.2} s perdus à chaque fois). Corriger ce défaut ramènerait ta moyenne de {} à ≈{}.",
                    z.segment,
                    z.typical_issue.split(" : ").nth(1).unwrap_or(&z.typical_issue),
                    z.occurrences,
                    z.laps_compared,
                    z.avg_loss_s,
                    fmt_lap(mean),
                    fmt_lap(target)
                ))
            }
            (None, Some(_)) => detail.theoretical_best_s.map(|t| {
                format!("Aucune perte récurrente détectée. Prochaine étape : enchaîner tes meilleurs segments pour viser le théorique {}.", fmt_lap(t))
            }),
            _ => Some("Pas assez de tours propres pour identifier une zone de progrès : roule au moins 3 tours lancés sans incident.".into()),
        });
        if suggestion.is_none() {
            suggestion = zones.first().and_then(|z| z.example.goal.clone());
        }
        Ok(Dashboard {
            active_goal: self.active_goal()?,
            next_goal_suggestion: suggestion,
            theoretical_best_s: detail.theoretical_best_s,
            last_session: Some(last),
            progress_zones: zones,
            consistency_trend: trend,
            next_goal,
            total_sessions: all.len(),
        })
    }

    /// Export complet (métadonnées + traces) au format JSON.
    pub fn export_all(&self, path: &Path) -> Result<usize> {
        #[derive(Serialize)]
        struct ExportLap {
            #[serde(flatten)]
            row: LapRow,
            trace: LapTrace,
        }
        #[derive(Serialize)]
        struct ExportSession {
            meta: SessionMeta,
            laps: Vec<ExportLap>,
        }
        let db = self.db();
        let sessions = db.list_sessions(&SessionFilter::default())?;
        let mut out = Vec::with_capacity(sessions.len());
        for s in &sessions {
            let laps = db
                .laps(s.id)?
                .into_iter()
                .map(|row| Ok(ExportLap { trace: db.trace(row.id)?, row }))
                .collect::<std::result::Result<Vec<_>, StoreError>>()?;
            out.push(ExportSession { meta: db.session_meta(s.id)?, laps });
        }
        let f = std::fs::File::create(path).map_err(|e| ServiceError::User(format!("export : {e}")))?;
        serde_json::to_writer(std::io::BufWriter::new(f), &serde_json::json!({ "format": "apex-companion-export", "version": 1, "sessions": out }))
            .map_err(|e| ServiceError::User(format!("export : {e}")))?;
        Ok(sessions.len())
    }

    /// Export d'une session pour analyse externe (fichier JSON compressé gzip).
    /// Contient méta, tours, traces brutes et conseils ; le chemin d'origine est réduit
    /// au nom de fichier (pas de nom d'utilisateur Windows).
    /// Copie automatique d'une session vers le dossier « mobile » choisi dans les réglages.
    pub fn export_to_mobile_dir(&self, id: i64) -> Result<Option<PathBuf>> {
        let Some(dir) = self.settings().mobile_export_dir.filter(|d| !d.trim().is_empty()) else { return Ok(None) };
        let dir = PathBuf::from(dir);
        if !dir.is_dir() {
            return Err(ServiceError::User(format!("dossier mobile introuvable : {}", dir.display())));
        }
        let meta = self.db().session_meta(id)?;
        let path = dir.join(export_file_name(&meta));
        self.export_session(id, &path, env!("CARGO_PKG_VERSION"))?;
        Ok(Some(path))
    }

    pub fn export_session(&self, id: i64, path: &Path, app_version: &str) -> Result<ExportSummary> {
        #[derive(Serialize)]
        struct ExportLap {
            #[serde(flatten)]
            row: LapRow,
            trace: LapTrace,
        }
        let detail = self.session_detail(id)?;
        let mut meta = detail.meta.clone();
        meta.source_path = meta.source_path.as_deref().map(|p| p.rsplit(['\\', '/']).next().unwrap_or(p).to_string());
        let laps: Vec<ExportLap> = {
            let db = self.db();
            detail
                .laps
                .iter()
                .map(|row| Ok(ExportLap { trace: db.trace(row.id)?, row: row.clone() }))
                .collect::<std::result::Result<_, StoreError>>()?
        };
        // Analyses précalculées (mêmes calculs que sur le PC) : l'app mobile se contente de les afficher.
        // La trace de référence n'est écrite qu'une fois pour toute la session.
        let ref_id = detail.analysis_reference_lap_id;
        let mut reference_trace = serde_json::Value::Null;
        let comparisons: Vec<serde_json::Value> = match ref_id {
            Some(rid) => detail
                .laps
                .iter()
                .filter(|l| l.meta.lap_time_s.is_some() && l.meta.kind == LapKind::Flying && l.id != rid)
                .filter_map(|l| self.compare(rid, l.id, 1.0).ok())
                // Tour tronqué (arrêt, reset) : comparaison sans objet.
                .filter(|c| c.compared.coverage >= 0.95)
                .filter_map(|c| {
                    let mut v = serde_json::to_value(&c).ok()?;
                    if let Some(o) = v.as_object_mut() {
                        let rt = o.remove("ref_trace").unwrap_or_default();
                        if reference_trace.is_null() {
                            reference_trace = rt;
                        }
                    }
                    Some(v)
                })
                .collect(),
            None => vec![],
        };
        let doc = serde_json::json!({
            "format": "apex-session-export",
            "version": 2,
            "app_version": app_version,
            "exported_at": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
            "session": meta,
            "best_lap_id": detail.best_lap_id,
            "reference_lap_id": ref_id,
            "reference_label": detail.reference_label,
            "risk": detail.risk,
            "risk_headline": detail.risk_headline,
            "fastest_lap_id": detail.fastest_lap_id,
            "off_tracks": detail.off_tracks,
            "theoretical_best_s": detail.theoretical_best_s,
            "consistency": detail.consistency,
            "laps": laps,
            "reference_trace": reference_trace,
            "comparisons": comparisons,
        });
        let f = std::fs::File::create(path).map_err(|e| ServiceError::User(format!("export : {e}")))?;
        let mut gz = flate2::write::GzEncoder::new(std::io::BufWriter::new(f), flate2::Compression::default());
        serde_json::to_writer(&mut gz, &doc).map_err(|e| ServiceError::User(format!("export : {e}")))?;
        gz.finish().and_then(|mut w| std::io::Write::flush(&mut w)).map_err(|e| ServiceError::User(format!("export : {e}")))?;
        Ok(ExportSummary {
            path: path.to_string_lossy().into_owned(),
            bytes: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
            laps: detail.laps.len(),
        })
    }

    /// Diagnostic exportable sans données personnelles (ni chemins, ni noms, ni télémétrie).
    pub fn diagnostics(&self, app_version: &str) -> Result<Diagnostics> {
        Ok(Diagnostics {
            app_version: app_version.into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            sessions: self.list_sessions(&SessionFilter::default())?.len(),
            connectors: self.connectors.iter().map(|c| (c.display_name().to_string(), c.capabilities())).collect(),
            note: "Aucun chemin, nom de pilote ni donnée de télémétrie inclus.".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garage61_reference_is_kept_apart_and_used_as_reference() {
        let svc = AppService::in_memory().unwrap();
        let mut s = demo_session(5, 1_760_000_000);
        s.meta.sim = SimId::Iracing;
        let InsertOutcome::Inserted(sid) = svc.insert_imported(&s).unwrap() else { panic!() };
        let len = s.meta.track_length_m.unwrap();
        // Tour « externe » fabriqué à partir d'un tour de la démo, au format CSV de Garage 61.
        let lap = &s.laps.iter().find(|l| l.meta.is_clean()).unwrap().trace;
        let mut csv = String::from("Speed,LapDistPct,Brake,Throttle,Gear,PositionType\n");
        let t_end = *lap.time_s.last().unwrap();
        let mut t = 0.0f32;
        while t < t_end {
            let i = lap.time_s.partition_point(|x| *x < t).min(lap.len() - 1);
            csv += &format!("{},{},{},{},{},3\n", lap.speed_mps[i], (lap.dist_m[i] / len).min(0.99999), lap.brake[i], lap.throttle[i], lap.gear.as_ref().map_or(3.0, |g| g[i]));
            t += 1.0 / 60.0;
        }
        let dir = tempfile::tempdir().unwrap();
        let car = s.meta.car.clone();
        let p = dir.path().join(format!("Garage 61 - Pilote Rapide - {car} - Circuit Démo Complet - 0{}.{:06.3} - ABC.csv", (t_end / 60.0) as u32, t_end % 60.0));
        std::fs::write(&p, csv).unwrap();
        // Arrivée dans « Téléchargements » : importé une seule fois.
        std::fs::write(dir.path().join("notes.csv"), "a,b\n").unwrap();
        let rep = svc.sync_downloads_in(dir.path()).unwrap();
        assert_eq!(rep.len(), 1, "seul le fichier Garage 61 est pris");
        assert!(rep[0].result.is_ok(), "{:?}", rep[0].result);
        assert!(svc.sync_downloads_in(dir.path()).unwrap().is_empty(), "pas de réimport");
        let _ = &p;
        // Pas mélangé aux sessions du pilote, mais utilisé comme référence d'analyse.
        assert_eq!(svc.list_sessions(&SessionFilter::default()).unwrap().len(), 1);
        assert_eq!(svc.list_sessions(&SessionFilter { external: Some(true), ..Default::default() }).unwrap()[0].external_driver.as_deref(), Some("Pilote Rapide"));
        let d = svc.session_detail(sid).unwrap();
        assert!(d.reference_label.unwrap().starts_with("Pilote Rapide"));
        let c = svc.compare(d.analysis_reference_lap_id.unwrap(), d.laps[1].id, 1.0).unwrap();
        assert!(c.warnings[0].contains("Pilote Rapide"));
        // Fichier hors circuit connu : refus explicite.
        let q = dir.path().join("Garage 61 - X - Voiture - Monza Grand Prix - 01.40.000 - Z.csv");
        std::fs::write(&q, "Speed,LapDistPct,Brake,Throttle\n").unwrap();
        assert!(svc.import_garage61(&q).unwrap_err().to_string().contains("Monza"));
    }

    /// Reproduit le cas réel de Miami : nom de circuit Garage 61 avec parenthèses, voiture sans le « 324 »,
    /// fichier dans un dossier de téléchargements quelconque.
    #[test]
    fn garage61_miami_file_with_parentheses_is_imported() {
        let svc = AppService::in_memory().unwrap();
        let mut s = demo_session(9, 1_760_000_000);
        s.meta.sim = SimId::Iracing;
        s.meta.track = "Miami International Autodrome".into();
        s.meta.track_config = Some("Grand Prix".into());
        s.meta.car = "Super Formula Lights 324".into();
        s.meta.source_fingerprint = "ibt:miami".into();
        let InsertOutcome::Inserted(sid) = svc.insert_imported(&s).unwrap() else { panic!() };
        // Une autre session sur un autre circuit ne doit pas gêner.
        let mut other = demo_session(10, 1_760_100_000);
        other.meta.sim = SimId::Iracing;
        other.meta.track = "Circuit of the Americas".into();
        other.meta.source_fingerprint = "ibt:cota".into();
        svc.insert_imported(&other).unwrap();
        let len = s.meta.track_length_m.unwrap();
        let lap = &s.laps.iter().find(|l| l.meta.is_clean()).unwrap().trace;
        let mut csv = String::from("Speed,LapDistPct,Lat,Lon,Brake,Throttle,RPM,SteeringWheelAngle,Gear,Clutch,ABSActive,DRSActive,P2PActive,LatAccel,LongAccel,VertAccel,Yaw,YawRate,PositionType\n");
        let t_end = *lap.time_s.last().unwrap();
        let mut t = 0.0f32;
        while t < t_end {
            let i = lap.time_s.partition_point(|x| *x < t).min(lap.len() - 1);
            csv += &format!("{},{},0,0,{},{},6000,0,3,1,false,false,false,0,0,9.8,0,0,3\n", lap.speed_mps[i], (lap.dist_m[i] / len).min(0.99999), lap.brake[i], lap.throttle[i]);
            t += 1.0 / 60.0;
        }
        let dir = tempfile::tempdir().unwrap();
        let name = format!("Garage 61 - Yohann Enette - Super Formula Lights - Miami International Autodrome (Grand Prix) - 0{}.{:06.3} - 01ABC.csv", (t_end / 60.0) as u32, t_end % 60.0);
        std::fs::write(dir.path().join(&name), csv).unwrap();
        let rep = svc.sync_downloads_in(dir.path()).unwrap();
        assert_eq!(rep.len(), 1);
        let r = rep[0].result.as_ref().unwrap();
        assert_eq!(r.driver, "Yohann Enette");
        assert!(r.car_matches && r.set_as_reference);
        let d = svc.session_detail(sid).unwrap();
        assert!(d.reference_label.as_deref().unwrap().starts_with("Yohann Enette"), "{:?}", d.reference_label);
        assert_eq!(d.analysis_reference_lap_id, Some(r.lap_id));
        // Comparaison complète d'un de tes tours avec cette référence.
        let mine = d.laps.iter().find(|l| l.meta.kind == LapKind::Flying && l.meta.lap_time_s.is_some()).unwrap();
        let c = svc.compare(r.lap_id, mine.id, 1.0).unwrap();
        assert_eq!(c.reference.driver.as_deref(), Some("Yohann Enette"));
        assert!(!c.corners.is_empty());
        // Message utile si le circuit n'existe pas dans tes sessions.
        let bad = dir.path().join("Garage 61 - X - Super Formula Lights - Road Atlanta (Full Course) - 01.20.000 - Z.csv");
        std::fs::write(&bad, "Speed,LapDistPct,Brake,Throttle\n").unwrap();
        let e = svc.import_garage61(&bad).unwrap_err().to_string();
        assert!(e.contains("Road Atlanta") && e.contains("Miami International Autodrome (Grand Prix)"), "{e}");
    }

    /// Séries fixed et open : une référence de l'autre type n'est jamais utilisée ; une référence de type
    /// inconnu ne sert qu'à défaut, avec un avertissement ; historique séparé par type.
    #[test]
    fn fixed_and_open_series_use_their_own_reference() {
        let svc = AppService::in_memory().unwrap();
        let mk = |seed: u64, t0: i64, setup: Option<SetupType>, fp: &str| {
            let mut s = demo_session(seed, t0);
            s.meta.sim = SimId::Iracing;
            s.meta.setup = setup;
            s.meta.source_fingerprint = fp.into();
            s
        };
        let fixed = mk(5, 1_760_000_000, Some(SetupType::Fixed), "ibt:fixed");
        let open = mk(6, 1_760_100_000, Some(SetupType::Open), "ibt:open");
        let InsertOutcome::Inserted(fid) = svc.insert_imported(&fixed).unwrap() else { panic!() };
        let InsertOutcome::Inserted(oid) = svc.insert_imported(&open).unwrap() else { panic!() };
        let len = fixed.meta.track_length_m.unwrap();
        let lap = fixed.laps.iter().find(|l| l.meta.is_clean()).unwrap().trace.clone();
        let t_end = *lap.time_s.last().unwrap();
        let csv = {
            let mut c = String::from("Speed,LapDistPct,Brake,Throttle,Gear,PositionType\n");
            let mut t = 0.0f32;
            while t < t_end {
                let i = lap.time_s.partition_point(|x| *x < t).min(lap.len() - 1);
                c += &format!("{},{},{},{},3,3\n", lap.speed_mps[i], (lap.dist_m[i] / len).min(0.99999), lap.brake[i], lap.throttle[i]);
                t += 1.0 / 60.0;
            }
            c
        };
        let car = fixed.meta.car.clone();
        let import = |driver: &str, id: &str, setup: Option<SetupType>| {
            let fname = format!("Garage 61 - {driver} - {car} - Circuit Démo Complet - 0{}.{:06.3} - {id}.csv", (t_end / 60.0) as u32, t_end % 60.0);
            let name = crate::ingest::garage61::parse_file_name(&fname).unwrap();
            svc.import_garage61_text(name, &csv, fname, setup).unwrap()
        };
        // Référence de type inconnu seule : utilisée partout, avec un avertissement.
        let nathan = import("Nathan Open", "N1", None);
        let d = svc.session_detail(fid).unwrap();
        assert_eq!(d.analysis_reference_lap_id, Some(nathan.lap_id));
        assert!(d.reference_label.as_deref().unwrap().contains("série inconnue"), "{:?}", d.reference_label);
        let c = svc.compare(nathan.lap_id, d.laps[1].id, 1.0).unwrap();
        assert!(c.warnings.iter().any(|w| w.contains("Type de série de la référence inconnu")), "{:?}", c.warnings);
        // Cas réel : la référence fixed a été importée avant la mise à jour (type inconnu) et a pris la place
        // de Nathan. Le pilote précise ensuite les deux types : chacun redevient la référence de sa série.
        let anthony = import("Anthony Fixed", "A1", None);
        assert!(!svc.db().lap(nathan.lap_id).unwrap().is_reference);
        svc.set_session_setup(nathan.session_id, Some(SetupType::Open)).unwrap();
        svc.set_session_setup(anthony.session_id, Some(SetupType::Fixed)).unwrap();
        // Les deux références coexistent : une par type de série.
        let refs: Vec<i64> = [nathan.lap_id, anthony.lap_id].into_iter().filter(|id| svc.db().lap(*id).unwrap().is_reference).collect();
        assert_eq!(refs.len(), 2, "une référence fixed et une open");
        let df = svc.session_detail(fid).unwrap();
        assert_eq!(df.analysis_reference_lap_id, Some(anthony.lap_id), "session fixed → référence fixed");
        assert!(df.reference_label.as_deref().unwrap().contains("Fixed"));
        let dop = svc.session_detail(oid).unwrap();
        assert_eq!(dop.analysis_reference_lap_id, Some(nathan.lap_id), "session open → référence open");
        // Coach en direct : même règle.
        let lf = svc.live_reference(&fixed.meta).unwrap().unwrap();
        assert!(lf.label.starts_with("Anthony Fixed"), "{}", lf.label);
        let lo = svc.live_reference(&open.meta).unwrap().unwrap();
        assert!(lo.label.starts_with("Nathan Open"), "{}", lo.label);
        // Comparaison forcée entre types différents : avertissement explicite.
        let c = svc.compare(nathan.lap_id, df.laps[1].id, 1.0).unwrap();
        assert!(c.warnings[0].contains("référence roulée en Open, ton tour en Fixed"), "{:?}", c.warnings);
        // Seule la référence open disparaît : la session fixed retombe sur sa propre référence, jamais sur l'open.
        svc.delete_session(anthony.session_id).unwrap();
        let df = svc.session_detail(fid).unwrap();
        assert_ne!(df.analysis_reference_lap_id, Some(nathan.lap_id));
        let lf = svc.live_reference(&fixed.meta).unwrap().unwrap();
        assert!(lf.label.starts_with("ton meilleur tour"), "{}", lf.label);
        // Historique : un groupe par type de série, sans mélange des temps.
        let cs = svc.circuits().unwrap();
        assert_eq!(cs.len(), 2);
        for c in &cs {
            let h = svc.lap_history(c.sim, &c.track, c.track_config.as_deref(), &c.car, c.setup).unwrap();
            let sid = if c.setup == Some(SetupType::Fixed) { fid } else { oid };
            assert!(h.iter().all(|l| l.session_id == sid));
            assert_eq!(c.external_reference.as_ref().map(|e| e.0.as_str()), if c.setup == Some(SetupType::Open) { Some("Nathan Open") } else { None });
        }
    }

    #[test]
    fn group_setup_follows_the_majority() {
        let lap = |id: &str| RemoteLap { id: id.into(), start: 0, lap_time: 90.0, clean: true, number: None };
        let g = vec![lap("a"), lap("b"), lap("c")];
        let mut tags = std::collections::HashMap::new();
        assert_eq!(group_setup(&g, &tags), None);
        tags.insert("a".to_string(), SetupType::Open);
        tags.insert("b".to_string(), SetupType::Fixed);
        tags.insert("c".to_string(), SetupType::Fixed);
        assert_eq!(group_setup(&g, &tags), Some(SetupType::Fixed));
        assert!(setup_warning(Some(SetupType::Fixed), Some(SetupType::Fixed)).is_none());
        assert!(setup_warning(None, None).is_none());
    }

    #[test]
    fn sessions_recovered_from_garage61_are_own_sessions() {
        let svc = AppService::in_memory().unwrap();
        let mut s = demo_session(9, 1_760_000_000);
        s.meta.sim = SimId::Iracing;
        s.meta.track = "Miami International Autodrome".into();
        s.meta.track_config = Some("Grand Prix".into());
        s.meta.car = "Super Formula Lights 324".into();
        s.meta.source_fingerprint = "ibt:miami".into();
        svc.insert_imported(&s).unwrap();
        let len = s.meta.track_length_m.unwrap();
        // Télémétrie Garage 61 de chaque tour (format CSV réel), à partir des tours de la session de démo.
        let flying: Vec<&ImportedLap> = s.laps.iter().filter(|l| l.meta.kind == LapKind::Flying && l.meta.lap_time_s.is_some()).collect();
        let csv_of = |lap: &LapTrace| {
            let mut csv = String::from("Speed,LapDistPct,Lat,Lon,Brake,Throttle,RPM,SteeringWheelAngle,Gear,Clutch,ABSActive,DRSActive,P2PActive,LatAccel,LongAccel,VertAccel,Yaw,YawRate,PositionType\n");
            let mut t = 0.0f32;
            while t < *lap.time_s.last().unwrap() {
                let i = lap.time_s.partition_point(|x| *x < t).min(lap.len() - 1);
                csv += &format!("{},{},0,0,{},{},6000,0,3,1,false,false,false,0,0,9.8,0,0,3\n", lap.speed_mps[i], (lap.dist_m[i] / len).min(0.99999), lap.brake[i], lap.throttle[i]);
                t += 1.0 / 60.0;
            }
            csv
        };
        let start = 1_760_090_000;
        let mut t = start;
        let remote: Vec<RemoteLap> = flying
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let r = RemoteLap { id: format!("L{i}"), start: t, lap_time: l.meta.lap_time_s.unwrap(), clean: l.meta.is_clean(), number: Some(i as i64 + 1) };
                t += r.lap_time.ceil() as i64;
                r
            })
            .collect();
        let groups = group_into_sessions(remote.clone(), 1200);
        assert_eq!(groups.len(), 1);
        let meta0 = s.meta.clone();
        let rs = recovered_session(&meta0, len, Some(SetupType::Fixed), &groups[0], |id| Ok(csv_of(&flying[id[1..].parse::<usize>().unwrap()].trace))).unwrap().unwrap();
        assert_eq!(rs.laps.len(), flying.len());
        assert_eq!(rs.meta.started_at_unix, start);
        let InsertOutcome::Inserted(id) = svc.insert_imported(&rs).unwrap() else { panic!() };
        // Session à toi (pas une référence externe) : comptée dans l'historique et les records.
        let d = svc.session_detail(id).unwrap();
        assert!(d.reference_label.is_none_or(|l| !l.contains("Garage 61 (récupérée)")));
        assert!(svc.list_sessions(&SessionFilter::default()).unwrap().iter().any(|x| x.id == id && x.external_driver.is_none()));
        let h = svc.lap_history(SimId::Iracing, "Miami International Autodrome", Some("Grand Prix"), "Super Formula Lights 324", Some(SetupType::Fixed)).unwrap();
        assert_eq!(h.iter().filter(|l| l.session_id == id).count(), flying.len());
        // Les tours invalides restent invalides (carte de risque juste).
        assert_eq!(rs.laps.iter().filter(|l| l.meta.validity == Validity::Invalid).count(), flying.iter().filter(|l| !l.meta.is_clean()).count());
        // Deuxième récupération : même empreinte, pas de doublon.
        assert!(matches!(svc.insert_imported(&rs).unwrap(), InsertOutcome::Duplicate(_)));
        // Téléchargement refusé (limite de débit) : on s'arrête net, sans session partielle.
        assert!(recovered_session(&meta0, len, None, &groups[0], |_| Err("trop de requêtes".into())).is_err());
    }

    #[test]
    fn training_plan_is_created_then_evaluated_on_the_next_session() {
        let svc = AppService::in_memory().unwrap();
        let now = now_secs();
        let InsertOutcome::Inserted(a) = svc.insert_imported(&demo_session(21, now - 3600)).unwrap() else { panic!() };
        let p = svc.training_plan(a).unwrap();
        assert!(!p.objectives.is_empty() && p.objectives.len() <= 3, "{:?}", p.objectives.iter().map(|o| &o.objective.title).collect::<Vec<_>>());
        assert!(p.evaluated_session_id.is_none(), "pas encore de séance après le programme");
        assert!(p.spoken.starts_with("Plan du jour. Un, "), "{}", p.spoken);
        // Même programme tant qu'on ne demande pas de nouveau.
        assert_eq!(svc.training_plan(a).unwrap().id, p.id);
        let meta_a = svc.db().session_meta(a).unwrap();
        assert!(svc.live_plan(&meta_a).is_some());
        // Séance suivante sur le même circuit : chaque objectif est évalué.
        let InsertOutcome::Inserted(b) = svc.insert_imported(&demo_session(22, now)).unwrap() else { panic!() };
        let v = svc.training_plan(b).unwrap();
        assert_eq!(v.id, p.id);
        assert_eq!(v.evaluated_session_id, Some(b));
        assert!(v.objectives.iter().all(|o| o.result.is_some()));
        // Nouveau programme : le précédent est archivé.
        let n = svc.new_training_plan(b).unwrap();
        assert_ne!(n.id, p.id);
        assert!(n.evaluated_session_id.is_none());
    }

    #[test]
    fn duplicate_sessions_from_same_file_are_merged() {
        let mut st = Store::open_in_memory().unwrap();
        let mut s = demo_session(3, 1_760_000_000);
        s.meta.sim = SimId::Iracing;
        s.meta.source_path = Some("C:/telemetry/a.ibt".into());
        let InsertOutcome::Inserted(a) = st.insert_session(&s).unwrap() else { panic!() };
        // Même fichier, empreinte différente (ancienne version du décodeur) : refusé comme doublon.
        let mut s2 = s.clone();
        s2.meta.source_fingerprint = "autre".into();
        assert_eq!(st.insert_session(&s2).unwrap(), InsertOutcome::Duplicate(a));
        // Doublon déjà présent en base (cas réel : sessions 1/7, 2/8… d'Interlagos) : fusionné au démarrage.
        st.conn_for_tests().execute("UPDATE sessions SET source_path = 'x' WHERE id = ?1", [a]).unwrap();
        let InsertOutcome::Inserted(b) = st.insert_session(&s2).unwrap() else { panic!() };
        st.conn_for_tests().execute("UPDATE sessions SET source_path = 'C:/telemetry/a.ibt' WHERE id = ?1", [a]).unwrap();
        let lap = st.laps(b).unwrap()[2].id;
        st.set_reference(lap).unwrap();
        assert_eq!(st.dedupe_sessions().unwrap(), 1);
        assert!(st.session_meta(b).is_err());
        assert!(st.laps(a).unwrap()[2].is_reference, "la référence est reportée sur la session gardée");
    }

    #[test]
    fn history_and_circuits_overview() {
        let svc = AppService::in_memory().unwrap();
        svc.load_demo().unwrap();
        svc.load_demo().unwrap();
        let c = svc.circuits().unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].sessions, 2);
        let h = svc.lap_history(c[0].sim, &c[0].track, c[0].track_config.as_deref(), &c[0].car, c[0].setup).unwrap();
        assert_eq!(h.len(), c[0].laps);
        let best = h.iter().filter(|l| l.clean && l.lap_time_s.is_some()).map(|l| l.lap_time_s.unwrap()).fold(f64::INFINITY, f64::min);
        assert!((c[0].best_clean_s.unwrap() - best).abs() < 1e-9);
        assert!(h.iter().any(|l| l.personal_best_at_time));
        assert!(h.iter().filter(|l| l.clean).all(|l| l.delta_to_best_s.unwrap() >= -1e-9));
        // Records successifs strictement décroissants.
        let pbs: Vec<f64> = h.iter().filter(|l| l.personal_best_at_time).map(|l| l.lap_time_s.unwrap()).collect();
        assert!(pbs.windows(2).all(|w| w[1] < w[0]));
    }

    #[test]
    fn risk_map_aggregates_all_sessions_of_the_combo() {
        let svc = AppService::in_memory().unwrap();
        let InsertOutcome::Inserted(a) = svc.load_demo().unwrap() else { panic!() };
        let InsertOutcome::Inserted(b) = svc.load_demo().unwrap() else { panic!() };
        let (da, db_) = (svc.session_detail(a).unwrap(), svc.session_detail(b).unwrap());
        assert!(!da.risk.is_empty());
        let flying = |d: &SessionDetail| d.laps.iter().filter(|l| l.meta.kind == LapKind::Flying && l.meta.lap_time_s.is_some()).count();
        assert_eq!(da.risk[0].passes, flying(&da) + flying(&db_), "les deux sessions comptent");
        assert_eq!(da.risk[0].sessions, 2);
        // Même session redemandée : le cache sert la seconde demande (résultat identique).
        assert_eq!(da.risk, svc.session_detail(a).unwrap().risk);
        assert_eq!(db_.risk[0].passes, da.risk[0].passes);
        // Un objectif « passages sans sortie » s'évalue.
        let r = svc.session_detail(a).unwrap();
        let g = svc.create_goal(a, &GoalSuggestion { title: "test".into(), metric: GoalMetric::CornerClean { apex_m: r.risk[0].apex_m, target_rate: 0.9 } }).unwrap();
        assert!(g.title.contains("test"));
    }

    #[test]
    fn export_file_name_is_readable() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(1_760_738_700), (2025, 10, 17));
        let mut m = demo_session(1, 1_760_738_700).meta;
        m.track = "Autódromo José Carlos Pace".into();
        m.car = "Super Formula Lights 324".into();
        assert_eq!(export_file_name(&m), "2025-10-17 22h05 Autódromo José Carlos Pace - Super Formula Lights 324.apex.json.gz");
    }

    #[test]
    fn export_v2_contains_precomputed_comparisons() {
        let svc = AppService::in_memory().unwrap();
        let InsertOutcome::Inserted(sid) = svc.load_demo().unwrap() else { panic!() };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.apex.json.gz");
        svc.export_session(sid, &path, "test").unwrap();
        let doc: serde_json::Value = serde_json::from_reader(flate2::read::GzDecoder::new(std::fs::File::open(&path).unwrap())).unwrap();
        assert_eq!(doc["version"], 2);
        let n_timed = doc["laps"].as_array().unwrap().iter().filter(|l| !l["lap_time_s"].is_null() && l["kind"] == "flying").count();
        let comps = doc["comparisons"].as_array().unwrap();
        assert_eq!(comps.len(), n_timed - 1);
        assert!(comps.iter().all(|c| c.get("ref_trace").is_none() && c["cmp_trace"]["speed_kmh"].is_array()));
        assert!(doc["reference_trace"]["dist"].is_array());
        // Copie automatique vers le dossier mobile.
        svc.save_settings(AppSettings { mobile_export_dir: Some(dir.path().to_string_lossy().into()), ..Default::default() }).unwrap();
        let p = svc.export_to_mobile_dir(sid).unwrap().unwrap();
        assert!(p.exists() && p.to_string_lossy().ends_with(".apex.json.gz"));
    }

    #[test]
    fn demo_flow_end_to_end() {
        let svc = AppService::in_memory().unwrap();
        let InsertOutcome::Inserted(sid) = svc.load_demo().unwrap() else { panic!() };
        let d = svc.session_detail(sid).unwrap();
        assert!(d.best_lap_id.is_some() && d.theoretical_best_s.is_some());
        let other = d.laps.iter().find(|l| l.clean && Some(l.id) != d.best_lap_id).unwrap();
        let c = svc.compare(d.best_lap_id.unwrap(), other.id, 2.0).unwrap();
        assert!(!c.delta_s.is_empty());
        svc.set_reference(d.best_lap_id.unwrap()).unwrap();
        assert_eq!(svc.session_detail(sid).unwrap().personal_reference_lap_id, d.best_lap_id);
        svc.save_settings(AppSettings { beginner_mode: false, ..Default::default() }).unwrap();
        let dash = svc.dashboard().unwrap();
        assert_eq!(dash.progress_zones[0].segment, "V1");
        assert!(dash.next_goal.unwrap().starts_with("V1"));
        // Démo : σ ≈ 0,3 s, une seule sortie → pas d'alerte débutant, on retombe sur la zone de progrès.
        svc.save_settings(AppSettings::default()).unwrap();
        assert!(svc.dashboard().unwrap().next_goal.unwrap().starts_with("V1"));
    }

    #[test]
    fn settings_validation_and_cloud_locked() {
        let svc = AppService::in_memory().unwrap();
        let s = AppSettings { cloud_enabled: true, ..Default::default() };
        assert!(!svc.save_settings(s).unwrap().cloud_enabled);
        let bad = AppSettings { sample_rate_hz: 1000, ..Default::default() };
        assert!(svc.save_settings(bad).is_err());
    }

    #[test]
    fn unsupported_file_is_reported_not_crashing() {
        let svc = AppService::in_memory().unwrap();
        let r = svc.import_paths(&[PathBuf::from("x.csv")]);
        assert!(r[0].error.as_ref().unwrap().contains("non pris en charge"));
    }

    #[test]
    fn folder_sync_imports_once_and_waits_for_finished_files() {
        let svc = AppService::in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        svc.save_settings(AppSettings { iracing_telemetry_dir: Some(dir.path().to_string_lossy().into()), ..Default::default() }).unwrap();
        std::fs::write(dir.path().join("session.ibt"), crate::ingest::ibt::writer::synthetic()).unwrap();
        std::fs::write(dir.path().join("casse.ibt"), b"pas un ibt").unwrap();
        // Fichier tout juste écrit : considéré en cours d'écriture.
        let r = svc.sync_iracing_folder(std::time::Duration::from_secs(3600)).unwrap();
        assert_eq!((r.imported, r.pending), (0, 2));
        let r = svc.sync_iracing_folder(std::time::Duration::ZERO).unwrap();
        assert_eq!(r.imported, 1);
        assert_eq!(r.errors.len(), 1, "{:?}", r.errors);
        // Deuxième passage : rien n'est reparsé, ni le bon fichier ni celui en erreur.
        let r = svc.sync_iracing_folder(std::time::Duration::ZERO).unwrap();
        assert_eq!(r, SyncReport::default());
        assert_eq!(svc.list_sessions(&SessionFilter::default()).unwrap().len(), 1);
        // Nouveau lancement (mémoire des essais vidée) : le fichier en erreur est retenté une fois, le bon non.
        svc.retried.lock().unwrap().clear();
        let r = svc.sync_iracing_folder(std::time::Duration::ZERO).unwrap();
        assert_eq!((r.imported, r.errors.len()), (0, 1));
        assert_eq!(svc.sync_iracing_folder(std::time::Duration::ZERO).unwrap(), SyncReport::default());
        // Erreur passagère (fichier verrouillé par un autre programme, cas réel du 22/10) : retentée à chaque passage.
        let key = dir.path().join("casse.ibt").to_string_lossy().into_owned();
        let meta = std::fs::metadata(dir.path().join("casse.ibt")).unwrap();
        let mtime = meta.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        svc.db().record_file(&key, meta.len() as i64, mtime, None, Some("import : lecture fichier : (os error 32)")).unwrap();
        assert!(svc.should_process(&key, meta.len() as i64, mtime).unwrap());
        assert!(svc.should_process(&key, meta.len() as i64, mtime).unwrap());
    }

    fn guarded_service(root: &Path) -> (AppService, PathBuf) {
        let mut svc = AppService::in_memory().unwrap();
        svc.keep_dir = Some(root.join("garde"));
        let tel = root.join("telemetry");
        std::fs::create_dir_all(&tel).unwrap();
        svc.save_settings(AppSettings { iracing_telemetry_dir: Some(tel.to_string_lossy().into()), ..Default::default() }).unwrap();
        (svc, tel)
    }

    #[test]
    fn file_deleted_by_another_program_is_still_imported() {
        // Cas réel du 22-23/10 : l'agent Garage 61 efface le .ibt 2 à 12 s après la fin de la session.
        let root = tempfile::tempdir().unwrap();
        let (svc, tel) = guarded_service(root.path());
        let ibt = crate::ingest::ibt::writer::synthetic();
        let f = tel.join("superformulalights324_miami gp 2026-10-23 11-00-14.ibt");
        // iRacing commence à écrire : la garde est posée pendant l'écriture.
        std::fs::write(&f, &ibt[..ibt.len() / 2]).unwrap();
        assert_eq!(svc.guard_iracing_files().unwrap(), 1);
        assert_eq!(svc.guard_iracing_files().unwrap(), 0, "une seule garde par fichier");
        {
            use std::io::Write;
            let mut w = std::fs::OpenOptions::new().append(true).open(&f).unwrap();
            w.write_all(&ibt[ibt.len() / 2..]).unwrap();
        }
        std::fs::remove_file(&f).unwrap();
        let r = svc.sync_iracing_folder(std::time::Duration::ZERO).unwrap();
        assert_eq!((r.imported, r.errors.len()), (1, 0), "{:?}", r.errors);
        let g = root.path().join("garde").join(f.file_name().unwrap());
        assert_eq!(std::fs::read(&g).unwrap(), ibt, "la garde contient le fichier complet");
        assert!(g.exists(), "seule copie brute : conservée");
        assert_eq!(svc.sync_iracing_folder(std::time::Duration::ZERO).unwrap(), SyncReport::default());
    }

    #[test]
    fn guard_is_removed_when_the_original_stays() {
        let root = tempfile::tempdir().unwrap();
        let (svc, tel) = guarded_service(root.path());
        let f = tel.join("s.ibt");
        std::fs::write(&f, crate::ingest::ibt::writer::synthetic()).unwrap();
        assert_eq!(svc.guard_iracing_files().unwrap(), 1);
        assert_eq!(svc.sync_iracing_folder(std::time::Duration::ZERO).unwrap().imported, 1);
        assert!(!root.path().join("garde").join("s.ibt").exists());
        // Fichier déjà importé : jamais regardé à nouveau, même après redémarrage.
        svc.guarded.lock().unwrap().clear();
        assert_eq!(svc.guard_iracing_files().unwrap(), 0);
    }

    /// Comportement Windows réel : iRacing écrit le fichier en ne partageant que la lecture ; la garde
    /// doit pouvoir être posée pendant l'écriture et survivre à la suppression de l'original.
    #[cfg(windows)]
    #[test]
    fn windows_guard_while_iracing_writes_then_original_deleted() {
        use std::io::Write;
        use std::os::windows::fs::OpenOptionsExt;
        let root = tempfile::tempdir().unwrap();
        let (svc, tel) = guarded_service(root.path());
        let ibt = crate::ingest::ibt::writer::synthetic();
        let f = tel.join("w.ibt");
        let mut w = std::fs::OpenOptions::new().write(true).create(true).share_mode(1 /* FILE_SHARE_READ */).open(&f).unwrap();
        w.write_all(&ibt[..ibt.len() / 2]).unwrap();
        w.flush().unwrap();
        assert_eq!(svc.guard_iracing_files().unwrap(), 1, "garde posée pendant l'écriture");
        w.write_all(&ibt[ibt.len() / 2..]).unwrap();
        drop(w);
        std::fs::remove_file(&f).unwrap();
        assert_eq!(std::fs::read(root.path().join("garde").join("w.ibt")).unwrap(), ibt);
        assert_eq!(svc.sync_iracing_folder(std::time::Duration::ZERO).unwrap().imported, 1);
    }

    #[test]
    fn remote_laps_are_grouped_into_sessions() {
        let l = |id: &str, start: i64| RemoteLap { id: id.into(), start, lap_time: 111.0, clean: true, number: None };
        // Deux relais de la même séance (arrêt de 6 min) puis une autre séance 1 h plus tard.
        let g = group_into_sessions(vec![l("c", 1000 + 111 * 2), l("a", 1000), l("b", 1111), l("d", 1000 + 111 * 3 + 360), l("e", 9000)], 1200);
        assert_eq!(g.iter().map(|s| s.iter().map(|x| x.id.as_str()).collect::<String>()).collect::<Vec<_>>(), vec!["abcd", "e"]);
        assert!(local_offset_at(1_792_746_014).abs() <= 14 * 3600);
        // Récupération automatique : due au premier lancement, puis une fois par jour.
        let svc = AppService::in_memory().unwrap();
        assert!(svc.garage61_recover_due(24));
        svc.db().set_setting("garage61_last_recover", &now_secs().to_string()).unwrap();
        assert!(!svc.garage61_recover_due(24));
    }

    #[test]
    fn autolog_toggle_with_backup_and_running_guard() {
        let svc = AppService::in_memory().unwrap();
        let root = tempfile::tempdir().unwrap();
        let tel = root.path().join("telemetry");
        std::fs::create_dir_all(&tel).unwrap();
        let ini = root.path().join("app.ini");
        std::fs::write(&ini, "[Telemetry]\r\nirsdkAutoLogDisk=0 ; auto\r\n").unwrap();
        svc.save_settings(AppSettings { iracing_telemetry_dir: Some(tel.to_string_lossy().into()), ..Default::default() }).unwrap();
        assert_eq!(svc.iracing_autolog().unwrap().enabled, Some(false));
        assert!(svc.set_iracing_autolog(true, true).is_err(), "refus si iRacing tourne");
        assert_eq!(svc.set_iracing_autolog(true, false).unwrap().enabled, Some(true));
        assert!(root.path().join("app.ini.apex-backup").exists());
        assert_eq!(std::fs::read_to_string(root.path().join("app.ini.apex-backup")).unwrap(), "[Telemetry]\r\nirsdkAutoLogDisk=0 ; auto\r\n");
    }

    #[test]
    fn session_export_is_compact_and_anonymised() {
        let svc = AppService::in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        svc.save_settings(AppSettings { iracing_telemetry_dir: Some(dir.path().to_string_lossy().into()), ..Default::default() }).unwrap();
        std::fs::write(dir.path().join("spa.ibt"), crate::ingest::ibt::writer::synthetic()).unwrap();
        svc.sync_iracing_folder(std::time::Duration::ZERO).unwrap();
        let sid = svc.list_sessions(&SessionFilter::default()).unwrap()[0].id;
        let out = dir.path().join("export.apex.json.gz");
        let r = svc.export_session(sid, &out, "0.4.0").unwrap();
        assert!(r.bytes > 1000);
        let mut txt = String::new();
        std::io::Read::read_to_string(&mut flate2::read::GzDecoder::new(std::fs::File::open(&out).unwrap()), &mut txt).unwrap();
        let v: serde_json::Value = serde_json::from_str(&txt).unwrap();
        assert_eq!(v["format"], "apex-session-export");
        assert_eq!(v["session"]["source_path"], "spa.ibt", "chemin réduit au nom de fichier");
        assert!(!v["laps"].as_array().unwrap().is_empty());
        assert!(!txt.contains(&*dir.path().to_string_lossy()));
    }

    #[test]
    fn corner_names_apply_to_comparison_and_insights() {
        let svc = AppService::in_memory().unwrap();
        let InsertOutcome::Inserted(sid) = svc.load_demo().unwrap() else { panic!() };
        let d = svc.session_detail(sid).unwrap();
        let best = d.best_lap_id.unwrap();
        let other = d.laps.iter().find(|l| l.meta.number == 1).unwrap().id; // tour avec freinage précoce en V1
        let c = svc.compare(best, other, 1.0).unwrap();
        let v1 = c.corners[0].corner.apex_m;
        svc.set_corner_name(sid, v1 + 12.0, "La Source").unwrap();
        let c = svc.compare(best, other, 1.0).unwrap();
        assert_eq!(c.corners[0].corner.name, "La Source");
        assert!(c.insights[0].title.starts_with("La Source :"), "{}", c.insights[0].title);
        // Renommer remplace, nom vide efface.
        svc.set_corner_name(sid, v1, "Virage 1").unwrap();
        assert_eq!(svc.compare(best, other, 1.0).unwrap().corners[0].corner.name, "Virage 1");
        svc.set_corner_name(sid, v1, "").unwrap();
        assert_eq!(svc.compare(best, other, 1.0).unwrap().corners[0].corner.name, "V1");
    }

    #[test]
    fn goal_from_insight_is_evaluated_on_next_session() {
        let svc = AppService::in_memory().unwrap();
        let InsertOutcome::Inserted(sid) = svc.load_demo().unwrap() else { panic!() };
        let d = svc.session_detail(sid).unwrap();
        let lap1 = d.laps.iter().find(|l| l.meta.number == 1).unwrap().id;
        let c = svc.compare(d.best_lap_id.unwrap(), lap1, 1.0).unwrap();
        let g = c.insights[0].goal.clone().expect("le freinage précoce propose un objectif");
        assert!(matches!(g.metric, GoalMetric::BrakePoint { .. }));
        let view = svc.create_goal(sid, &g).unwrap();
        assert!(view.result.is_none(), "aucune session après la création");
        // Session suivante sur le même circuit : 2 tours sur 4 freinent encore trop tôt.
        let later = crate::mock::demo_session(77, view.created_at_unix + 60);
        svc.db().insert_session(&later).unwrap();
        let view = svc.active_goal().unwrap().unwrap();
        let r = view.result.unwrap();
        assert_eq!((r.laps_ok, r.laps_evaluated, r.achieved), (2, 4, false), "{}", r.summary);
        assert!(svc.dashboard().unwrap().active_goal.is_some());
        svc.archive_goal(view.id).unwrap();
        assert!(svc.active_goal().unwrap().is_none());
    }

    #[test]
    fn export_and_diagnostics() {
        let svc = AppService::in_memory().unwrap();
        svc.load_demo().unwrap();
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(svc.export_all(&dir.path().join("e.json")).unwrap(), 1);
        let d = svc.diagnostics("0.1.0").unwrap();
        let json = serde_json::to_string(&d).unwrap();
        assert!(!json.contains("Circuit Démo"));
    }
}
