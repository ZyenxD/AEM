mod commands;
mod dto;
mod install;
mod state;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("avdhub".into()),
                    },
                ))
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Stdout,
                ))
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Built here, after the log plugin, so its startup facts are logged.
            let app_state = state::AppState::new()?;
            app.manage(app_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_setup_status,
            commands::refresh_catalog,
            commands::list_images,
            commands::list_profiles,
            commands::list_devices,
            commands::create_device,
            commands::update_device,
            commands::delete_device,
            commands::launch_device,
            commands::stop_device,
            commands::pending_licenses,
            commands::accept_license,
            commands::packages_needed_for,
            commands::core_packages_needed,
            commands::start_install,
            commands::cancel_install,
            commands::check_space,
            commands::check_acceleration,
            commands::open_windows_features,
        ])
        .run(tauri::generate_context!())
        .expect("error while running application");
}
