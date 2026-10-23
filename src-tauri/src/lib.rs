//! Shell Tauri : fenêtre, commandes IPC, thread de surveillance des simulateurs.
//! Toute la logique métier vit dans `apex-core` (testée sans WebView).

mod commands;
mod monitor;
mod ptt_loop;
mod updater;

use apex_core::service::AppService;
use std::sync::{Arc, Mutex};
use tauri::Manager;

pub struct AppState {
    pub svc: Arc<AppService>,
    pub status: Arc<Mutex<monitor::StatusPayload>>,
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            // Journal local uniquement (dossier de logs de l'appli), jamais envoyé.
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .max_file_size(2_000_000)
                .build(),
        )
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let svc = Arc::new(AppService::open(&dir.join("apex.db")).map_err(|e| e.to_string())?);
            let status = Arc::new(Mutex::new(monitor::StatusPayload::default()));
            monitor::spawn(app.handle().clone(), svc.clone(), status.clone());
            ptt_loop::spawn(app.handle().clone(), svc.clone(), status.clone());
            app.manage(AppState { svc, status });
            log::info!("Apex Companion démarré, données : {}", dir.display());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::list_connectors,
            commands::list_sessions,
            commands::get_session,
            commands::import_files,
            commands::scan_iracing,
            commands::load_demo,
            commands::compare_laps,
            commands::set_reference,
            commands::delete_session,
            commands::delete_all_data,
            commands::export_data,
            commands::get_diagnostics,
            commands::get_dashboard,
            commands::get_settings,
            commands::save_settings,
            commands::check_update,
            commands::install_update,
            commands::get_iracing_autolog,
            commands::set_iracing_autolog,
            commands::export_session,
            commands::export_to_mobile,
            commands::garage61_check,
            commands::list_circuits,
            commands::lap_history,
            commands::find_garage61_reference,
            commands::find_garage61_references_all,
            commands::recover_garage61_sessions,
            commands::tts_voices,
            commands::tts_install,
            commands::tts_speak,
            commands::ptt_learn,
            commands::voice_installed,
            commands::voice_install,
            commands::set_coach_muted,
            commands::set_corner_name,
            commands::create_goal,
            commands::archive_goal,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement d'Apex Companion");
}
