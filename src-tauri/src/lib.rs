mod commands;
mod dto;
mod install;
mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = state::AppState::new().expect("could not initialise application state");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running application");
}
