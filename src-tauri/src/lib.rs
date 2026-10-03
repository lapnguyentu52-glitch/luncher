mod commands;
mod events;
mod protocol;
mod state;

use log::info;

use state::core_state::CoreState;
use state::storage_mode::StorageMode;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let core = CoreState::resolve(StorageMode::detect());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(core)
        .setup(|app| {
            info!("Antares desktop shell started");
            let handle = app.handle().clone();
            let state = app
                .state::<CoreState>()
                .app
                .clone();
            events::bridge::spawn(handle, state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_ping,
            commands::app_version,
            commands::app_storage_mode,
            commands::app_storage_info,
            commands::app_test_event,
            commands::core_status,
            commands::core_spawn_task,
            commands::core_task_progress,
            commands::core_complete_task,
            commands::core_cancel_task,
            commands::legacy_status,
            commands::legacy_start,
            commands::legacy_call,
            commands::legacy_restart,
            commands::legacy_shutdown,
        ])
        .run(tauri::generate_context!())
        .expect("error while running antares desktop");
}
