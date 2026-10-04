use tauri::{AppHandle, State};

use engine::host::HostInfo;
use engine::repository::{Catalog, RemotePackage};
use engine::sdk_layout::SdkLayout;

use crate::dto::*;
use crate::state::AppState;

fn lock_error<T>(_: T) -> String {
    "internal lock error".to_string()
}

fn avd_home() -> Result<std::path::PathBuf, String> {
    engine::sdk_layout::avd_home().ok_or_else(|| "could not locate the device folder".to_string())
}

/// The cached catalog, fetching it first if this is the first use.
async fn catalog_or_fetch(state: &State<'_, AppState>) -> Result<Catalog, String> {
    if let Ok(catalog) = state.cached_catalog() {
        return Ok(catalog);
    }
    let catalog = tauri::async_runtime::spawn_blocking(engine::repository::fetch_catalog)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    *state.catalog.lock().map_err(lock_error)? = Some(catalog.clone());
    Ok(catalog)
}

// ---- setup ----

#[tauri::command]
pub async fn get_setup_status(state: State<'_, AppState>) -> Result<SetupStatusDto, String> {
    let sdk = state.sdk()?;
    let host = state.host.clone();
    tauri::async_runtime::spawn_blocking(move || SetupStatusDto::build(&sdk, &host))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_profiles() -> Vec<ProfileDto> {
    engine::avd::profiles::builtin()
        .iter()
        .map(ProfileDto::from)
        .collect()
}

// ---- catalog ----

#[tauri::command]
pub async fn refresh_catalog(state: State<'_, AppState>) -> Result<Vec<ImageDto>, String> {
    let sdk = state.sdk()?;
    let host = state.host.clone();

    let catalog = tauri::async_runtime::spawn_blocking(engine::repository::fetch_catalog)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    let images = image_list(&catalog, &sdk, &host);

    *state.catalog.lock().map_err(lock_error)? = Some(catalog);
    Ok(images)
}

#[tauri::command]
pub async fn list_images(state: State<'_, AppState>) -> Result<Vec<ImageDto>, String> {
    let catalog = catalog_or_fetch(&state).await?;
    let sdk = state.sdk()?;
    Ok(image_list(&catalog, &sdk, &state.host))
}

// ---- devices ----

#[tauri::command]
pub async fn list_devices(state: State<'_, AppState>) -> Result<Vec<DeviceDto>, String> {
    let sdk = state.sdk()?;
    tauri::async_runtime::spawn_blocking(move || {
        let avd_home = avd_home()?;
        let devices = engine::avd::list(&avd_home).map_err(|e| e.to_string())?;
        let running = engine::process::running(&sdk).unwrap_or_default();

        Ok(devices
            .iter()
            .map(|avd| {
                let match_run = running
                    .iter()
                    .find(|r| r.avd_name.as_deref() == Some(avd.name.as_str()));
                DeviceDto::from_avd(avd, match_run)
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn create_device(
    state: State<'_, AppState>,
    display_name: String,
    profile_id: String,
    package_path: String,
) -> Result<DeviceDto, String> {
    let sdk = state.sdk()?;
    tauri::async_runtime::spawn_blocking(move || {
        let avd_home = avd_home()?;
        std::fs::create_dir_all(&avd_home).map_err(|e| e.to_string())?;

        let profile = engine::avd::profiles::by_id(&profile_id)
            .ok_or_else(|| format!("unknown device type: {profile_id}"))?;

        let image = sdk
            .installed_images()
            .into_iter()
            .find(|i| i.package_path == package_path)
            .ok_or_else(|| "that Android version is not installed yet".to_string())?;

        let avd = engine::avd::create(&avd_home, &sdk, &display_name, &profile, &image)
            .map_err(|e| e.to_string())?;
        Ok(DeviceDto::from_avd(&avd, None))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn delete_device(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let _ = state.sdk()?;
    tauri::async_runtime::spawn_blocking(move || {
        let avd_home = avd_home()?;
        let avd = engine::avd::find(&avd_home, &name).map_err(|e| e.to_string())?;
        engine::avd::delete(&avd_home, &avd).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn update_device(
    state: State<'_, AppState>,
    name: String,
    ram_mb: Option<u32>,
    storage_mb: Option<u32>,
    display_name: Option<String>,
) -> Result<DeviceDto, String> {
    let _ = state.sdk()?;
    tauri::async_runtime::spawn_blocking(move || {
        let avd_home = avd_home()?;
        let mut avd = engine::avd::find(&avd_home, &name).map_err(|e| e.to_string())?;
        engine::avd::update(
            &mut avd,
            engine::avd::AvdPatch {
                ram_mb,
                storage_mb,
                display_name,
                ..Default::default()
            },
        )
        .map_err(|e| e.to_string())?;
        Ok(DeviceDto::from_avd(&avd, None))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn launch_device(
    state: State<'_, AppState>,
    name: String,
    cold_boot: bool,
    wipe_data: bool,
) -> Result<u32, String> {
    let sdk = state.sdk()?;
    tauri::async_runtime::spawn_blocking(move || {
        let avd_home = avd_home()?;
        let avd = engine::avd::find(&avd_home, &name).map_err(|e| e.to_string())?;
        log::info!(
            "launching {name} with {:?} (cold boot: {cold_boot}, wipe: {wipe_data})",
            sdk.emulator_bin()
        );
        engine::process::launch(
            &sdk,
            &avd,
            &engine::process::LaunchOptions {
                cold_boot,
                wipe_data,
                extra_args: vec![],
            },
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn stop_device(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let sdk = state.sdk()?;
    tauri::async_runtime::spawn_blocking(move || {
        let running = engine::process::running(&sdk).map_err(|e| e.to_string())?;
        let target = running
            .iter()
            .find(|r| r.avd_name.as_deref() == Some(name.as_str()))
            .ok_or_else(|| "that device is not running".to_string())?;
        engine::process::stop(&sdk, target).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- licences ----

#[tauri::command]
pub async fn pending_licenses(
    state: State<'_, AppState>,
    package_paths: Vec<String>,
) -> Result<Vec<LicenseDto>, String> {
    let sdk = state.sdk()?;
    let catalog = catalog_or_fetch(&state).await?;

    tauri::async_runtime::spawn_blocking(move || {
        let packages: Vec<&RemotePackage> = package_paths
            .iter()
            .filter_map(|p| catalog.find(p))
            .collect();
        engine::licenses::pending(&catalog, &packages, &sdk)
            .into_iter()
            .map(|l| LicenseDto {
                id: l.id.clone(),
                text: l.text.clone(),
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Must only be called because the user pressed Accept with the licence text on screen.
#[tauri::command]
pub async fn accept_license(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let sdk = state.sdk()?;
    let catalog = state.cached_catalog()?;

    tauri::async_runtime::spawn_blocking(move || {
        let license = catalog
            .licenses
            .iter()
            .find(|l| l.id == id)
            .ok_or_else(|| format!("unknown licence: {id}"))?;
        engine::licenses::accept(license, &sdk).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- installing ----

/// Every package missing for `image` to run: the core tools, plus the image itself.
/// `image: None` asks only for the core tools (used by the first-run screen).
fn needed_packages(
    sdk: &SdkLayout,
    host: &HostInfo,
    catalog: &Catalog,
    image: Option<String>,
) -> Vec<String> {
    let mut needed = Vec::new();

    if sdk.emulator_bin().is_none() {
        if let Some(p) = catalog.latest_tool("emulator", host) {
            needed.push(p.path.clone());
        }
    }
    if sdk.adb_bin().is_none() {
        if let Some(p) = catalog.latest_tool("platform-tools", host) {
            needed.push(p.path.clone());
        }
    }
    if let Some(image) = image {
        let already = sdk
            .installed_images()
            .iter()
            .any(|i| i.package_path == image);
        if !already {
            needed.push(image);
        }
    }
    needed
}

#[tauri::command]
pub async fn packages_needed_for(
    state: State<'_, AppState>,
    package_path: String,
) -> Result<Vec<String>, String> {
    let sdk = state.sdk()?;
    let host = state.host.clone();
    let catalog = catalog_or_fetch(&state).await?;

    tauri::async_runtime::spawn_blocking(move || {
        let image = Some(package_path).filter(|p| !p.is_empty());
        needed_packages(&sdk, &host, &catalog, image)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn core_packages_needed(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let sdk = state.sdk()?;
    let host = state.host.clone();
    let catalog = catalog_or_fetch(&state).await?;

    tauri::async_runtime::spawn_blocking(move || needed_packages(&sdk, &host, &catalog, None))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_install(
    app: AppHandle,
    state: State<'_, AppState>,
    package_paths: Vec<String>,
) -> Result<(), String> {
    let sdk = state.sdk()?;
    let host = state.host.clone();
    let catalog = state.cached_catalog()?;

    // Refuse to start while any licence is unaccepted.
    let outstanding = {
        let packages: Vec<&RemotePackage> = package_paths
            .iter()
            .filter_map(|p| catalog.find(p))
            .collect();
        engine::licenses::pending(&catalog, &packages, &sdk).len()
    };
    if outstanding > 0 {
        return Err("the licence must be accepted before installing".to_string());
    }

    // A previous cancellation must not abort this new job.
    state.cancel.reset();
    let cancel = state.cancel.clone();
    log::info!("starting install of {package_paths:?}");

    // No `.await`: the job keeps running after this command returns.
    tauri::async_runtime::spawn_blocking(move || {
        crate::install::run_install(app, sdk, host, catalog, package_paths, cancel);
    });

    Ok(())
}

/// Synchronous on purpose: setting an atomic flag takes nanoseconds.
#[tauri::command]
pub fn cancel_install(state: State<'_, AppState>) {
    log::info!("cancel requested");
    state.cancel.cancel();
}

#[tauri::command]
pub async fn check_space(
    state: State<'_, AppState>,
    package_paths: Vec<String>,
) -> Result<SpaceCheckDto, String> {
    let sdk = state.sdk()?;
    let host = state.host.clone();
    let catalog = catalog_or_fetch(&state).await?;

    tauri::async_runtime::spawn_blocking(move || {
        let packages: Vec<&RemotePackage> = package_paths
            .iter()
            .filter_map(|p| catalog.find(p))
            .collect();
        let needed = engine::downloader::space_needed(&packages, &host);
        let available = engine::host::free_disk_bytes(&sdk.root);

        SpaceCheckDto {
            needed_mb: needed / 1_048_576,
            available_mb: available.map(|b| b / 1_048_576),
            // Unknown free space: let it try rather than block on a failed measurement.
            enough: available.map(|a| a >= needed).unwrap_or(true),
        }
    })
    .await
    .map_err(|e| e.to_string())
}

// ---- acceleration ----

#[tauri::command]
pub async fn check_acceleration(
    state: State<'_, AppState>,
) -> Result<engine::accel::AccelReport, String> {
    let sdk = state.sdk()?;
    tauri::async_runtime::spawn_blocking(move || {
        let report = engine::accel::check(&sdk).map_err(|e| e.to_string())?;
        log::info!("accel check: {:?} ({})", report.status, report.raw_message);
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn open_windows_features() -> Result<(), String> {
    engine::accel::open_windows_features().map_err(|e| e.to_string())
}
