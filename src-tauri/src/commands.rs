//! Commandes IPC : fines enveloppes autour de `AppService`.
//! Les opérations lourdes passent par `spawn_blocking` pour ne jamais bloquer l'UI.

use crate::monitor::StatusPayload;
use crate::AppState;
use apex_core::analysis::Comparison;
use apex_core::service::{AppService, AppSettings, ConnectorInfo, Dashboard, Diagnostics, ImportReport, ServiceError, SessionDetail};
use apex_core::storage::{InsertOutcome, SessionFilter, SessionSummary};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

type R<T> = Result<T, ServiceError>;

async fn blocking<T: Send + 'static>(svc: Arc<AppService>, f: impl FnOnce(&AppService) -> R<T> + Send + 'static) -> R<T> {
    tauri::async_runtime::spawn_blocking(move || f(&svc))
        .await
        .map_err(|e| ServiceError::User(format!("tâche interrompue : {e}")))?
}

#[derive(serde::Serialize)]
pub struct VoiceInfo {
    pub id: String,
    pub label: String,
    pub installed: bool,
}

#[tauri::command]
pub fn tts_voices(state: State<'_, AppState>) -> Vec<VoiceInfo> {
    state.svc.tts_voices().into_iter().map(|(id, label, installed)| VoiceInfo { id, label, installed }).collect()
}

#[tauri::command]
pub async fn tts_install(state: State<'_, AppState>, voice: String) -> R<()> {
    blocking(state.svc.clone(), move |s| s.tts_install(&voice)).await
}

/// WAV brut (ArrayBuffer côté interface) : pas d'encodage JSON d'un tableau d'octets.
#[tauri::command]
pub async fn tts_speak(state: State<'_, AppState>, voice: String, text: String) -> R<tauri::ipc::Response> {
    let wav = blocking(state.svc.clone(), move |s| s.tts_speak(&voice, &text)).await?;
    Ok(tauri::ipc::Response::new(wav))
}

/// Attend l'appui sur le bouton radio (10 s) et l'enregistre dans les réglages.
#[tauri::command]
pub async fn ptt_learn(state: State<'_, AppState>) -> R<Option<apex_core::ptt::PttBinding>> {
    blocking(state.svc.clone(), |s| {
        let b = apex_core::ptt::learn(std::time::Duration::from_secs(10));
        if let Some(b) = &b {
            let mut st = s.settings();
            st.ptt_binding = Some(b.clone());
            st.voice_commands = true;
            s.save_settings(st)?;
        }
        Ok(b)
    })
    .await
}

/// Appareils de jeu vus par l'app (aide quand un bouton n'est pas détecté).
#[tauri::command]
pub async fn ptt_devices(state: State<'_, AppState>) -> R<Vec<String>> {
    blocking(state.svc.clone(), |_| {
        let _ = apex_core::ptt::snapshot(false);
        std::thread::sleep(std::time::Duration::from_millis(500));
        Ok(apex_core::ptt::devices())
    })
    .await
}

#[tauri::command]
pub fn voice_installed(state: State<'_, AppState>) -> bool {
    state.svc.voice_installed()
}

#[tauri::command]
pub async fn voice_install(state: State<'_, AppState>) -> R<()> {
    blocking(state.svc.clone(), |s| s.voice_install()).await
}

/// Radio coupée / reprise depuis l'interface.
#[tauri::command]
pub fn set_coach_muted(state: State<'_, AppState>, muted: bool) {
    if let Ok(mut s) = state.status.lock() {
        s.muted = muted;
    }
}

#[tauri::command]
pub async fn training_plan(state: State<'_, AppState>, session_id: i64) -> R<apex_core::service::PlanView> {
    blocking(state.svc.clone(), move |s| s.training_plan(session_id)).await
}

#[tauri::command]
pub async fn add_plan_objective(state: State<'_, AppState>, session_id: i64, goal: apex_core::analysis::goals::GoalSuggestion) -> R<apex_core::service::PlanView> {
    blocking(state.svc.clone(), move |s| s.add_plan_objective(session_id, &goal)).await
}

#[tauri::command]
pub async fn new_training_plan(state: State<'_, AppState>, session_id: i64) -> R<apex_core::service::PlanView> {
    blocking(state.svc.clone(), move |s| s.new_training_plan(session_id)).await
}

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> StatusPayload {
    state.status.lock().map(|s| s.clone()).unwrap_or_default()
}

#[tauri::command]
pub fn list_connectors(state: State<'_, AppState>) -> Vec<ConnectorInfo> {
    state.svc.connectors_info()
}

#[tauri::command]
pub async fn list_sessions(state: State<'_, AppState>, filter: Option<SessionFilter>) -> R<Vec<SessionSummary>> {
    blocking(state.svc.clone(), move |s| s.list_sessions(&filter.unwrap_or_default())).await
}

#[tauri::command]
pub async fn get_session(state: State<'_, AppState>, id: i64) -> R<SessionDetail> {
    blocking(state.svc.clone(), move |s| s.session_detail(id)).await
}

#[tauri::command]
pub async fn import_files(state: State<'_, AppState>, paths: Vec<String>) -> R<Vec<ImportReport>> {
    let status = state.status.clone();
    crate::monitor::set_importing(&status, Some(format!("{} fichier(s)", paths.len())));
    let r = blocking(state.svc.clone(), move |s| Ok(s.import_paths(&paths.into_iter().map(PathBuf::from).collect::<Vec<_>>()))).await;
    crate::monitor::set_importing(&status, None);
    r
}

#[tauri::command]
pub async fn scan_iracing(state: State<'_, AppState>) -> R<Vec<ImportReport>> {
    blocking(state.svc.clone(), |s| s.scan_iracing()).await
}

#[tauri::command]
pub async fn load_demo(state: State<'_, AppState>) -> R<InsertOutcome> {
    blocking(state.svc.clone(), |s| s.load_demo()).await
}

#[tauri::command]
pub async fn compare_laps(state: State<'_, AppState>, reference_lap_id: i64, compared_lap_id: i64, step_m: Option<f32>) -> R<Comparison> {
    blocking(state.svc.clone(), move |s| s.compare(reference_lap_id, compared_lap_id, step_m.unwrap_or(1.0))).await
}

#[tauri::command]
pub async fn set_reference(state: State<'_, AppState>, lap_id: i64) -> R<()> {
    blocking(state.svc.clone(), move |s| s.set_reference(lap_id)).await
}

#[tauri::command]
pub async fn set_session_setup(state: State<'_, AppState>, id: i64, setup: Option<apex_core::model::SetupType>) -> R<()> {
    blocking(state.svc.clone(), move |s| s.set_session_setup(id, setup)).await
}

#[tauri::command]
pub async fn delete_session(state: State<'_, AppState>, id: i64) -> R<()> {
    blocking(state.svc.clone(), move |s| s.delete_session(id)).await
}

#[tauri::command]
pub async fn delete_all_data(state: State<'_, AppState>) -> R<()> {
    blocking(state.svc.clone(), |s| s.delete_all()).await
}

#[tauri::command]
pub async fn export_data(state: State<'_, AppState>, path: String) -> R<usize> {
    blocking(state.svc.clone(), move |s| s.export_all(path.as_ref())).await
}

#[tauri::command]
pub async fn get_diagnostics(state: State<'_, AppState>) -> R<Diagnostics> {
    blocking(state.svc.clone(), |s| s.diagnostics(env!("CARGO_PKG_VERSION"))).await
}

#[tauri::command]
pub async fn get_dashboard(state: State<'_, AppState>) -> R<Dashboard> {
    blocking(state.svc.clone(), |s| s.dashboard()).await
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.svc.settings()
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> R<AppSettings> {
    state.svc.save_settings(settings)
}

#[tauri::command]
pub async fn check_update(app: tauri::AppHandle) -> Result<Option<crate::updater::UpdateInfo>, String> {
    let current = app.package_info().version.to_string();
    tauri::async_runtime::spawn_blocking(move || crate::updater::check(&current)).await.map_err(|e| e.to_string())?
}

/// Télécharge et vérifie l'installeur, le lance, puis ferme l'application pour qu'il puisse la remplacer.
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<String, String> {
    let current = app.package_info().version.to_string();
    let v = tauri::async_runtime::spawn_blocking(move || crate::updater::download_and_launch(&current)).await.map_err(|e| e.to_string())??;
    app.exit(0);
    Ok(v)
}

#[tauri::command]
pub async fn get_iracing_autolog(state: State<'_, AppState>) -> R<apex_core::service::AutoLogStatus> {
    blocking(state.svc.clone(), |s| s.iracing_autolog()).await
}

#[tauri::command]
pub async fn set_iracing_autolog(state: State<'_, AppState>, enable: bool) -> R<apex_core::service::AutoLogStatus> {
    let running = state.status.lock().map(|s| s.running.contains(&apex_core::model::SimId::Iracing)).unwrap_or(true);
    blocking(state.svc.clone(), move |s| s.set_iracing_autolog(enable, running)).await
}

#[tauri::command]
pub async fn export_session(state: State<'_, AppState>, id: i64, path: String) -> R<apex_core::service::ExportSummary> {
    blocking(state.svc.clone(), move |s| s.export_session(id, path.as_ref(), env!("CARGO_PKG_VERSION"))).await
}

#[tauri::command]
pub async fn list_circuits(state: State<'_, AppState>) -> R<Vec<apex_core::service::CircuitOverview>> {
    blocking(state.svc.clone(), |s| s.circuits()).await
}

#[tauri::command]
pub async fn lap_history(
    state: State<'_, AppState>,
    sim: apex_core::model::SimId,
    track: String,
    track_config: Option<String>,
    car: String,
    setup: Option<apex_core::model::SetupType>,
) -> R<Vec<apex_core::service::HistoryLap>> {
    blocking(state.svc.clone(), move |s| s.lap_history(sim, &track, track_config.as_deref(), &car, setup)).await
}

#[tauri::command]
pub async fn garage61_check(state: State<'_, AppState>) -> R<String> {
    blocking(state.svc.clone(), |s| s.garage61_check()).await
}

#[tauri::command]
pub async fn find_garage61_references_all(state: State<'_, AppState>) -> R<Vec<String>> {
    blocking(state.svc.clone(), |s| s.find_garage61_references_all()).await
}

#[tauri::command]
pub async fn recover_garage61_sessions(state: State<'_, AppState>, days: i64) -> R<apex_core::service::RecoverReport> {
    blocking(state.svc.clone(), move |s| s.recover_garage61_sessions(days)).await
}

#[tauri::command]
pub async fn find_garage61_reference(state: State<'_, AppState>, session_id: i64) -> R<String> {
    blocking(state.svc.clone(), move |s| s.find_garage61_reference(session_id)).await
}

#[tauri::command]
pub async fn export_to_mobile(state: State<'_, AppState>, id: i64) -> R<Option<String>> {
    blocking(state.svc.clone(), move |s| s.export_to_mobile_dir(id).map(|p| p.map(|p| p.to_string_lossy().into_owned()))).await
}

#[tauri::command]
pub async fn set_corner_name(state: State<'_, AppState>, session_id: i64, apex_m: f32, name: String) -> R<()> {
    blocking(state.svc.clone(), move |s| s.set_corner_name(session_id, apex_m, &name)).await
}

#[tauri::command]
pub async fn create_goal(state: State<'_, AppState>, session_id: i64, goal: apex_core::analysis::goals::GoalSuggestion) -> R<apex_core::service::GoalView> {
    blocking(state.svc.clone(), move |s| s.create_goal(session_id, &goal)).await
}

#[tauri::command]
pub async fn archive_goal(state: State<'_, AppState>, id: i64) -> R<()> {
    blocking(state.svc.clone(), move |s| s.archive_goal(id)).await
}
