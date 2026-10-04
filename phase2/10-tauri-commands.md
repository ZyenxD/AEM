# 10 — Tauri Commands: The Bridge

**Goal:** expose the engine to the UI as a set of commands Angular can call, without
letting a single line of UI concern leak into the engine.

**Prerequisite:** Phase 2 complete (12 tests green, `first_boot` works).

**A note on verification:** every line of Phase 2 was compiled and tested before you saw
it. Phase 3's code was not — my environment lost its Rust toolchain. The engine calls
underneath are the same ones you already exercised, but expect to fix a type mismatch or
two here. That is normal work, and by now you can read the compiler's errors.

---

## 10.1 The shape of the bridge

```
Angular component
   ↓ invoke('list_devices')
Tauri command  (src-tauri/src/commands.rs)
   ↓ calls
engine::avd::list(...)          ← unchanged from Phase 2
   ↑ returns Vec<Avd>
DTO conversion (src-tauri/src/dto.rs)
   ↑ returns Vec<DeviceDto>  → JSON → TypeScript
```

Three rules that keep this clean:

1. **Commands are thin.** A command locks some state, calls one or two engine functions,
   converts the result, and returns. If a command grows logic, that logic belongs in the
   engine.
2. **Engine types do not cross the boundary.** `Avd` holds an `IniFile` with forty
   arbitrary keys; Angular wants `ramMb` and `displayName`. The DTO layer flattens.
3. **Errors become strings at the boundary, not before.** The engine returns rich
   `EngineError` values; the command turns them into a message the UI can display.

---

## 10.2 Why DTOs rather than sending engine types directly

Every engine type already derives `Serialize`, so you *could* return `Vec<Avd>` and let
Angular dig through `config.entries["hw.ramSize"]`. Do not. Three reasons:

- **The UI would depend on ini key spellings.** Change how the engine stores something and
  every Angular template breaks.
- **Computed values have nowhere to live.** "Is this device running?" comes from
  `process::running()`, not from the `Avd`. The DTO is where the two are joined.
- **snake_case vs camelCase.** `#[serde(rename_all = "camelCase")]` on the DTO gives
  TypeScript the names it expects, without contorting the Rust.

---

## 10.3 The DTO module

Create `src-tauri/src/dto.rs`.

### Piece 1 — the device

```rust
use serde::{Deserialize, Serialize};

use engine::avd::profiles::DeviceProfile;
use engine::avd::Avd;
use engine::host::HostInfo;
use engine::process::RunningEmulator;
use engine::repository::{Catalog, RemotePackage};
use engine::sdk_layout::SdkLayout;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDto {
    pub name: String,
    pub display_name: String,
    pub api_level: Option<u32>,
    pub android_name: Option<String>,
    pub abi: Option<String>,
    pub tag: Option<String>,
    pub ram_mb: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub play_store: bool,
    pub running: bool,
    pub booted: bool,
    pub serial: Option<String>,
}

impl DeviceDto {
    pub fn from_avd(avd: &Avd, running: Option<&RunningEmulator>) -> DeviceDto {
        let resolution = avd.resolution();
        DeviceDto {
            name: avd.name.clone(),
            display_name: avd.display_name(),
            api_level: avd.api_level(),
            android_name: avd
                .api_level()
                .map(|a| engine::repository::android_version_name(a).to_string()),
            abi: avd.abi().map(|s| s.to_string()),
            tag: avd.config.get("tag.display").map(|s| s.to_string()),
            ram_mb: avd.ram_mb(),
            width: resolution.map(|r| r.0),
            height: resolution.map(|r| r.1),
            play_store: avd.config.get("PlayStore.enabled") == Some("true"),
            running: running.is_some(),
            booted: running.map(|r| r.booted).unwrap_or(false),
            serial: running.map(|r| r.serial.clone()),
        }
    }
}
```

Note `from_avd` takes `running: Option<&RunningEmulator>` — the join happens here. The
caller fetches the running list once and matches each device against it, rather than
shelling out to adb per device.

`android_name` is precomputed so the template writes `{{ device.androidName }}` instead of
mapping API levels in TypeScript. Keep that table in one language.

### Piece 2 — images, profiles, licences

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageDto {
    pub package_path: String,
    pub api_level: Option<u32>,
    pub android_name: String,
    pub tag_id: Option<String>,
    pub tag_label: Option<String>,
    pub size_mb: u64,
    pub installed: bool,
    pub recommended: bool,
}

impl ImageDto {
    pub fn from_package(
        pkg: &RemotePackage,
        host: &HostInfo,
        installed: bool,
        recommended: bool,
    ) -> ImageDto {
        ImageDto {
            package_path: pkg.path.clone(),
            api_level: pkg.api_level,
            android_name: pkg
                .api_level
                .map(|a| engine::repository::android_version_name(a).to_string())
                .unwrap_or_else(|| pkg.display_name.clone()),
            tag_id: pkg.tag_id.clone(),
            tag_label: pkg.tag_display.clone(),
            size_mb: pkg.archive_for(host).map(|a| a.size_bytes / 1_048_576).unwrap_or(0),
            installed,
            recommended,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDto {
    pub id: String,
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub ram_mb: u32,
}

impl From<&DeviceProfile> for ProfileDto {
    fn from(p: &DeviceProfile) -> ProfileDto {
        ProfileDto {
            id: p.id.clone(),
            label: p.label.clone(),
            width: p.width,
            height: p.height,
            ram_mb: p.ram_mb,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseDto {
    pub id: String,
    pub text: String,
}
```

`impl From<&DeviceProfile> for ProfileDto` is the idiomatic Rust conversion trait; it lets
you write `.map(ProfileDto::from)` in an iterator chain.

`size_mb` is pre-divided: the UI shows "1638 MB", and doing the arithmetic here means one
place to change it if you later want GB.

`installed` and `recommended` are **decisions**, not data — exactly the kind of thing that
would otherwise end up duplicated in TypeScript.

### Piece 3 — setup status and the image list

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatusDto {
    pub sdk_root: String,
    pub managed_by_us: bool,
    pub has_emulator: bool,
    pub has_adb: bool,
    pub installed_image_count: usize,
    pub abi_label: String,
    pub os_label: String,
    pub free_disk_gb: Option<u64>,
    pub ready: bool,
}

impl SetupStatusDto {
    pub fn build(sdk: &SdkLayout, host: &HostInfo) -> SetupStatusDto {
        let has_emulator = sdk.emulator_bin().is_some();
        let has_adb = sdk.adb_bin().is_some();
        let images = sdk.installed_images().len();
        SetupStatusDto {
            sdk_root: sdk.root.display().to_string(),
            managed_by_us: sdk.managed_by_us,
            has_emulator,
            has_adb,
            installed_image_count: images,
            abi_label: engine::repository::abi_label(host.preferred_abi).to_string(),
            os_label: format!("{:?}", host.os),
            free_disk_gb: engine::host::free_disk_bytes(&sdk.root).map(|b| b / 1_073_741_824),
            ready: has_emulator && has_adb && images > 0,
        }
    }
}

/// Build the image list the "create device" screen shows.
pub fn image_list(catalog: &Catalog, sdk: &SdkLayout, host: &HostInfo) -> Vec<ImageDto> {
    let installed = sdk.installed_images();
    let mut out = Vec::new();
    for (index, pkg) in catalog.system_images(host).into_iter().enumerate() {
        let is_installed = installed.iter().any(|i| i.package_path == pkg.path);
        let recommended = index == 0 || pkg.tag_id.as_deref() == Some("google_apis_playstore");
        out.push(ImageDto::from_package(pkg, host, is_installed, recommended));
    }
    out
}
```

`ready` is the single boolean the first-run wizard branches on. Computing it in Rust means
the rule ("emulator + adb + at least one image") exists once.

`.enumerate()` gives `(index, item)` pairs, so `index == 0` marks the newest image as
recommended.

---

## 10.4 Application state

The engine is stateless — every function takes the SDK path as an argument. But the app
needs to remember three things between calls: which SDK we are using, the machine's
details, and the catalog (a multi-second download you do not want to repeat per click).

Create `src-tauri/src/state.rs`:

```rust
use std::sync::Mutex;

use engine::host::HostInfo;
use engine::repository::Catalog;
use engine::sdk_layout::SdkLayout;

pub struct AppState {
    pub host: HostInfo,
    pub sdk: Mutex<SdkLayout>,
    pub catalog: Mutex<Option<Catalog>>,
}

impl AppState {
    pub fn new() -> Result<AppState, String> {
        let host = engine::host::detect();

        // Reuse an existing SDK if we find one, otherwise create ours.
        let sdk = engine::sdk_layout::SdkLayout::discover()
            .into_iter()
            .next()
            .map(Ok)
            .unwrap_or_else(engine::sdk_layout::SdkLayout::create_managed)
            .map_err(|e| e.to_string())?;

        Ok(AppState {
            host,
            sdk: Mutex::new(sdk),
            catalog: Mutex::new(None),
        })
    }

    /// Snapshot of the SDK, so callers never hold the lock while doing slow work.
    pub fn sdk(&self) -> Result<SdkLayout, String> {
        self.sdk
            .lock()
            .map(|g| g.clone())
            .map_err(|_| "internal lock error".to_string())
    }
}
```

**Why `Mutex`.** Tauri shares state across threads, so anything managed must be `Send +
Sync`. `Mutex` provides that. `host` needs no mutex — it never changes after detection.

**Why `sdk()` returns a clone rather than a guard.** This is the most important line in the
file, and it prevents a deadlock you would otherwise hit within a day. If a command held
the lock while running a five-minute download, every other command would block. Cloning a
`SdkLayout` copies a path and a bool — free — and the lock is released immediately.

`.map(Ok).unwrap_or_else(SdkLayout::create_managed)` reads oddly. It means: if `discover()`
found an SDK, wrap it in `Ok`; otherwise call `create_managed()` (which itself returns a
`Result`). Both branches end up as `Result<SdkLayout, EngineError>`, then `map_err` turns
the error into a string.

---

## 10.5 The threading trap

This is the single most consequential thing in Phase 3.

> **In Tauri v2, a `#[tauri::command]` declared as a plain `fn` runs on the main thread —
> the same thread that draws your window.** Async commands run on Tauri's async runtime
> instead.

Your engine is blocking by design (document 01: we chose `ureq` to avoid teaching async on
day one). So a synchronous command calling `fetch_catalog()` freezes the entire window for
several seconds. Calling `install_package()` freezes it for minutes. The app appears
crashed; on macOS you get the spinning beachball.

The fix is mechanical, and the pattern is:

```rust
#[tauri::command]
pub async fn something(state: State<'_, AppState>) -> Result<T, String> {
    let sdk = state.sdk()?;             // 1. pull data out of State first
    tauri::async_runtime::spawn_blocking(move || {
        engine::do_blocking_work(&sdk)  // 2. blocking work on a pool thread
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?        // 3. the join error (task panicked)
}
```

Three details worth understanding:

**Step 1 is mandatory, not stylistic.** `State<'_, AppState>` is not `Send`, so it cannot
be moved into the closure. Extract what you need — a cloned `SdkLayout`, a cloned
`HostInfo` — before spawning. If you try to use `state` inside the closure, the compiler
will tell you, in a long message whose key phrase is "cannot be sent between threads
safely".

**`.await` returns a nested result.** The outer `Result` reports whether the task itself
ran (it fails only if the closure panicked); the inner one is your function's result.
Hence `.map_err(...)?` followed by the inner value.

**`move ||` transfers ownership** of the cloned values into the closure, which must own
everything it touches because it outlives the current function (primer §3, §10).

Every command that touches the network, the disk, or a subprocess follows this shape. The
only commands that may stay synchronous are ones that read a field from state and return
it immediately.

---

## 10.6 The commands

Create `src-tauri/src/commands.rs`.

### Piece 4 — reading state

```rust
use tauri::State;

use crate::dto::*;
use crate::state::AppState;

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
    engine::avd::profiles::builtin().iter().map(ProfileDto::from).collect()
}
```

`get_setup_status` is async because `installed_images()` and `free_disk_bytes()` touch the
disk — fast, but on a network volume they may not be.

`list_profiles` is the exception that proves the rule: it builds six structs from constants
in microseconds, touching nothing. Synchronous is correct here, and it returns a bare
`Vec` rather than a `Result` because it cannot fail.

### Piece 5 — the catalog

```rust
#[tauri::command]
pub async fn refresh_catalog(state: State<'_, AppState>) -> Result<Vec<ImageDto>, String> {
    let sdk = state.sdk()?;
    let host = state.host.clone();

    let catalog = tauri::async_runtime::spawn_blocking(engine::repository::fetch_catalog)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    let images = image_list(&catalog, &sdk, &host);

    *state.catalog.lock().map_err(|_| "internal lock error".to_string())? = Some(catalog);
    Ok(images)
}

#[tauri::command]
pub async fn list_images(state: State<'_, AppState>) -> Result<Vec<ImageDto>, String> {
    let cached = {
        let guard = state.catalog.lock().map_err(|_| "internal lock error".to_string())?;
        guard.clone()
    };
    match cached {
        Some(catalog) => {
            let sdk = state.sdk()?;
            Ok(image_list(&catalog, &sdk, &state.host))
        }
        None => refresh_catalog(state).await,
    }
}
```

`refresh_catalog` always hits the network; `list_images` uses the cache and only falls back
to a fetch on first use. The UI calls `list_images` normally and `refresh_catalog` from an
explicit refresh button.

**Note the braces around the cache read.** `let cached = { ... guard.clone() };` ends the
borrow of the mutex at the closing brace, *before* the `match`. Without that scope, the
guard would still be alive across the `.await` inside the `None` branch — and a
`MutexGuard` is not `Send`, so it will not compile. Deliberately scoping lock guards is a
habit worth forming now.

`spawn_blocking(engine::repository::fetch_catalog)` passes the function itself instead of a
closure, since it takes no arguments.

### Piece 6 — devices

```rust
#[tauri::command]
pub async fn list_devices(state: State<'_, AppState>) -> Result<Vec<DeviceDto>, String> {
    let sdk = state.sdk()?;
    tauri::async_runtime::spawn_blocking(move || {
        let avd_home = engine::sdk_layout::avd_home()
            .ok_or_else(|| "could not locate the device folder".to_string())?;
        let devices = engine::avd::list(&avd_home).map_err(|e| e.to_string())?;
        let running = engine::process::running(&sdk).unwrap_or_default();

        Ok(devices
            .iter()
            .map(|avd| {
                let match_run = running.iter().find(|r| r.avd_name.as_deref() == Some(avd.name.as_str()));
                DeviceDto::from_avd(avd, match_run)
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}
```

One `adb devices` call for the whole list, then an in-memory match per device — the join
promised in §10.3.

`.unwrap_or_default()` on the running list means "if adb is unavailable, treat everything
as stopped" rather than failing the entire listing. A missing status is better than a
missing device list.

```rust
#[tauri::command]
pub async fn create_device(
    state: State<'_, AppState>,
    display_name: String,
    profile_id: String,
    package_path: String,
) -> Result<DeviceDto, String> {
    let sdk = state.sdk()?;
    tauri::async_runtime::spawn_blocking(move || {
        let avd_home = engine::sdk_layout::avd_home()
            .ok_or_else(|| "could not locate the device folder".to_string())?;
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
```

**Command arguments are camelCase on the JavaScript side.** Angular calls
`invoke('create_device', { displayName, profileId, packagePath })` and Tauri maps those to
the snake_case Rust parameters automatically.

The "not installed yet" error is a real state: the user can reach this screen with an image
selected but not yet downloaded. The message is written for them, not for you.

### Piece 7 — the rest

```rust
#[tauri::command]
pub async fn delete_device(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let _ = state.sdk()?;
    tauri::async_runtime::spawn_blocking(move || {
        let avd_home = engine::sdk_layout::avd_home()
            .ok_or_else(|| "could not locate the device folder".to_string())?;
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
        let avd_home = engine::sdk_layout::avd_home()
            .ok_or_else(|| "could not locate the device folder".to_string())?;
        let mut avd = engine::avd::find(&avd_home, &name).map_err(|e| e.to_string())?;
        engine::avd::update(
            &mut avd,
            engine::avd::AvdPatch { ram_mb, storage_mb, display_name, ..Default::default() },
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
        let avd_home = engine::sdk_layout::avd_home()
            .ok_or_else(|| "could not locate the device folder".to_string())?;
        let avd = engine::avd::find(&avd_home, &name).map_err(|e| e.to_string())?;
        engine::process::launch(
            &sdk,
            &avd,
            &engine::process::LaunchOptions { cold_boot, wipe_data, extra_args: vec![] },
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
```

`AvdPatch { ram_mb, storage_mb, display_name, ..Default::default() }` uses field init
shorthand *and* struct update syntax together — the parameter names match the field names,
and the remaining fields default to `None`, meaning "do not touch" (document 07).

`let _ = state.sdk()?;` in the device-only commands looks pointless but validates that the
SDK is accessible before doing work, and keeps every command's shape identical.

### Piece 8 — licences

```rust
#[tauri::command]
pub async fn pending_licenses(
    state: State<'_, AppState>,
    package_paths: Vec<String>,
) -> Result<Vec<LicenseDto>, String> {
    let sdk = state.sdk()?;
    let catalog = {
        let guard = state.catalog.lock().map_err(|_| "internal lock error".to_string())?;
        guard.clone().ok_or_else(|| "the package list has not been loaded".to_string())?
    };

    tauri::async_runtime::spawn_blocking(move || {
        let packages: Vec<&engine::repository::RemotePackage> = package_paths
            .iter()
            .filter_map(|p| catalog.find(p))
            .collect();
        engine::licenses::pending(&catalog, &packages, &sdk)
            .into_iter()
            .map(|l| LicenseDto { id: l.id.clone(), text: l.text.clone() })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn accept_license(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let sdk = state.sdk()?;
    let catalog = {
        let guard = state.catalog.lock().map_err(|_| "internal lock error".to_string())?;
        guard.clone().ok_or_else(|| "the package list has not been loaded".to_string())?
    };

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
```

**`accept_license` exists as its own command for a reason.** The rule from document 06 —
acceptance only ever follows a user pressing Accept — is enforced by making it a separate
call the UI must deliberately make. The install command will refuse to run if licences are
still pending.

*(This will be a borrow puzzle: `packages` holds references into `catalog`, and both are
inside the closure, which is fine, but if the compiler objects, the fix is to collect owned
`License` values before the spawn instead.)*

---

## 10.7 Registering everything

Open `src-tauri/src/lib.rs`. Replace the template's contents:

```rust
mod commands;
mod dto;
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running application");
}
```

- `.manage(app_state)` hands the state to Tauri. Any command with a `State<'_, AppState>`
  parameter now receives it.
- `generate_handler![...]` builds the dispatch table. **A command missing from this list is
  invisible to the frontend** — you get a runtime error "command not found", which is the
  most common Phase 3 mistake. Add to this list every time you add a command.
- Keep whichever `.plugin(...)` lines the template generated.

Check it compiles:

```bash
cd src-tauri
cargo check
```

Expect a handful of errors on the first attempt — a missing `use`, a `Send` complaint, a
borrow in the licences command. Read them; each one is the compiler explaining a rule from
§10.5.

---

## 10.8 Checkpoint

- [ ] `cargo check` passes in `src-tauri`.
- [ ] `cargo test -p engine` still shows 12 passing — the engine did not change.
- [ ] Every command is listed in `generate_handler!`.
- [ ] You can explain why `state.sdk()` returns a clone instead of a guard.

```bash
git add -A && git commit -m "Add Tauri command layer and DTOs"
```

---

## 10.9 Understanding check

1. What happens to the window if you remove `async` from `refresh_catalog`?
2. Why must you extract values from `State` *before* calling `spawn_blocking`?
3. Why is `list_profiles` allowed to be synchronous?
4. Why do `pending_licenses` and `accept_license` exist as two commands rather than the
   install command handling licences itself?
5. What is the symptom of forgetting to add a command to `generate_handler!`?
