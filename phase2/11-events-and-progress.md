# 11 — Events: Streaming Progress to the UI

**Goal:** run a multi-gigabyte install in the background and stream progress to Angular,
without freezing the window or blocking other commands.

**Prerequisite:** document 10.

---

## 11.1 Why commands are not enough

Every command so far follows one shape: Angular asks, Rust answers, done. That works for
"list my devices". It does not work for "install Android 15", because:

- It takes minutes, and a command that returns after minutes gives the UI nothing to show
  in between.
- The user must be able to keep using the app — browse devices, read settings — while it
  runs.
- Several packages install in sequence; the UI wants to know which one is active.

So installs use a different shape: **a command that starts a job and returns immediately,
plus events emitted as the job progresses.**

```
Angular                         Rust
   │ invoke('start_install')      │
   │─────────────────────────────>│ spawn background task
   │<─── returns () immediately ──│      │
   │                              │      ├─ emit 'install-progress'
   │<── event ────────────────────│      ├─ emit 'install-progress'
   │<── event ────────────────────│      ├─ emit 'install-progress'
   │<── event ────────────────────│      └─ emit 'install-finished'
```

The `Progress` enum from document 05 was designed for exactly this moment. The engine does
not change at all.

---

## 11.2 The event payload

Add to `src-tauri/src/dto.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgressDto {
    /// Which package is being worked on right now.
    pub package_path: String,
    pub label: String,
    /// 1-based position in the queue, and its length.
    pub index: usize,
    pub total: usize,
    /// "downloading" | "verifying" | "unpacking" | "finished"
    pub stage: String,
    pub done_bytes: u64,
    pub total_bytes: u64,
    /// 0-100 for the current package.
    pub percent: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallFinishedDto {
    pub ok: bool,
    pub error: Option<String>,
}
```

Two decisions here:

**`stage` is a string, not an enum.** Rust enums serialise to JSON in shapes that are
awkward to narrow in TypeScript. A plain string that TypeScript types as a union
(`'downloading' | 'verifying' | ...`) is simpler on both sides. This is a boundary type; it
is allowed to be dumber than the engine's `Progress`.

**`percent` is computed in Rust.** Otherwise every template doing the division risks
dividing by zero when `total_bytes` is unknown.

`index` and `total` let the UI say "Downloading 2 of 3" without tracking the queue itself.

---

## 11.3 The install job

Create `src-tauri/src/install.rs`:

```rust
use tauri::{AppHandle, Emitter};

use engine::downloader::Progress;
use engine::host::HostInfo;
use engine::repository::Catalog;
use engine::sdk_layout::SdkLayout;

use crate::dto::{InstallFinishedDto, InstallProgressDto};

pub const PROGRESS_EVENT: &str = "install-progress";
pub const FINISHED_EVENT: &str = "install-finished";

/// Runs on a background thread. Emits progress; never returns anything.
pub fn run_install(
    app: AppHandle,
    sdk: SdkLayout,
    host: HostInfo,
    catalog: Catalog,
    package_paths: Vec<String>,
) {
    let total = package_paths.len();
    let mut failure: Option<String> = None;

    for (position, path) in package_paths.iter().enumerate() {
        let Some(pkg) = catalog.find(path) else {
            failure = Some(format!("unknown package: {path}"));
            break;
        };
        let label = pkg.friendly_name();
        let index = position + 1;

        let app_for_closure = app.clone();
        let path_for_closure = path.clone();
        let label_for_closure = label.clone();

        let result = engine::downloader::install_package(pkg, &host, &sdk, move |progress| {
            let dto = to_dto(
                &path_for_closure,
                &label_for_closure,
                index,
                total,
                progress,
            );
            let _ = app_for_closure.emit(PROGRESS_EVENT, dto);
        });

        if let Err(e) = result {
            failure = Some(e.to_string());
            break;
        }
    }

    let _ = app.emit(
        FINISHED_EVENT,
        InstallFinishedDto {
            ok: failure.is_none(),
            error: failure,
        },
    );
}

fn to_dto(
    package_path: &str,
    label: &str,
    index: usize,
    total: usize,
    progress: Progress,
) -> InstallProgressDto {
    let (stage, done_bytes, total_bytes) = match progress {
        Progress::Started { total_bytes } => ("downloading", 0, total_bytes),
        Progress::Downloading { done_bytes, total_bytes } => ("downloading", done_bytes, total_bytes),
        Progress::Verifying => ("verifying", 0, 0),
        Progress::Unpacking => ("unpacking", 0, 0),
        Progress::Finished { .. } => ("finished", 0, 0),
    };

    let percent = if total_bytes > 0 {
        ((done_bytes * 100) / total_bytes).min(100) as u8
    } else if stage == "finished" {
        100
    } else {
        0
    };

    InstallProgressDto {
        package_path: package_path.to_string(),
        label: label.to_string(),
        index,
        total,
        stage: stage.to_string(),
        done_bytes,
        total_bytes,
        percent,
    }
}
```

Points worth absorbing:

**`use tauri::Emitter;`** — in Tauri v2, `emit` lives on the `Emitter` trait. Without this
import, `app.emit(...)` does not exist and the error message ("no method named `emit`") does
not mention the trait. Same class of mistake as forgetting `use std::io::Read` in document
05.

**`AppHandle` is the cloneable handle to the app.** Unlike `State`, it *is* `Send`, so it
can be moved into background threads and closures. Clone it freely — that is what it is
for.

**The three `_for_closure` clones** exist because the progress callback outlives the loop
iteration. The closure is `move`, so it takes ownership; cloning first keeps the originals
usable for the next iteration. Verbose, but the alternative is a lifetime fight you do not
need (primer §3).

**`let _ = app.emit(...)`** deliberately ignores the result. Emitting fails only if the
window is gone — and if the user closed the window mid-download, there is nothing useful to
do about it. Crashing the background thread would be worse.

**The loop breaks on first failure** and reports it once, rather than pressing on through
three broken downloads.

---

## 11.4 The command that starts it

Add to `src-tauri/src/commands.rs`:

```rust
use tauri::AppHandle;

#[tauri::command]
pub async fn start_install(
    app: AppHandle,
    state: State<'_, AppState>,
    package_paths: Vec<String>,
) -> Result<(), String> {
    let sdk = state.sdk()?;
    let host = state.host.clone();
    let catalog = {
        let guard = state.catalog.lock().map_err(|_| "internal lock error".to_string())?;
        guard.clone().ok_or_else(|| "the package list has not been loaded".to_string())?
    };

    // Refuse to start while any licence is unaccepted.
    let outstanding = {
        let packages: Vec<&engine::repository::RemotePackage> =
            package_paths.iter().filter_map(|p| catalog.find(p)).collect();
        engine::licenses::pending(&catalog, &packages, &sdk).len()
    };
    if outstanding > 0 {
        return Err("the licence must be accepted before installing".to_string());
    }

    tauri::async_runtime::spawn_blocking(move || {
        crate::install::run_install(app, sdk, host, catalog, package_paths);
    });

    Ok(())
}
```

**`app: AppHandle` as a parameter.** Tauri injects it automatically — the frontend does not
pass it. Same for `State`. Any other parameters come from the JavaScript call.

**Note there is no `.await`.** We spawn the task and return immediately; that is the whole
point. The job outlives this function, which is why everything moved into it must be owned.

**The licence guard** is the enforcement promised in document 10. The UI is supposed to
call `pending_licenses` and `accept_license` first; this makes "supposed to" into
"must". Defence in depth: the rule is documented, implemented in the UI flow, *and* checked
at the boundary.

Register it in `lib.rs`:

```rust
            commands::start_install,
```

and add `mod install;` at the top.

---

## 11.5 Installing what the user actually needs

The create-device screen should not make the user think about "platform-tools". Add one
more command that works out the full set:

```rust
#[tauri::command]
pub async fn packages_needed_for(
    state: State<'_, AppState>,
    package_path: String,
) -> Result<Vec<String>, String> {
    let sdk = state.sdk()?;
    let host = state.host.clone();
    let catalog = {
        let guard = state.catalog.lock().map_err(|_| "internal lock error".to_string())?;
        guard.clone().ok_or_else(|| "the package list has not been loaded".to_string())?
    };

    tauri::async_runtime::spawn_blocking(move || {
        let mut needed = Vec::new();

        if sdk.emulator_bin().is_none() {
            if let Some(p) = catalog.latest_tool("emulator", &host) {
                needed.push(p.path.clone());
            }
        }
        if sdk.adb_bin().is_none() {
            if let Some(p) = catalog.latest_tool("platform-tools", &host) {
                needed.push(p.path.clone());
            }
        }
        let already = sdk.installed_images().iter().any(|i| i.package_path == package_path);
        if !already {
            needed.push(package_path);
        }
        needed
    })
    .await
    .map_err(|e| e.to_string())
}
```

The UI calls this, shows "3 items, 2.4 GB", asks for licence acceptance, then passes the
same list to `start_install`. A user who already has the tools installs only the image.

This is the kind of decision that belongs in Rust rather than TypeScript: it depends on
what is on disk, and it is the same logic the future CLI would need.

---

## 11.6 A note on cancellation

Real download managers let you cancel. Ours does not, and adding it properly means
threading an `AtomicBool` through `install_package`'s callback so the engine can abort
between chunks — a change to the engine's signature.

That is a legitimate Phase 4 issue, not a Phase 3 requirement. What matters now is that you
know it is missing, and that the UI should therefore not show a Cancel button it cannot
honour. Design around the limitation instead of faking it.

File it as an issue while you remember:

> **Cancellable installs.** `install_package` should accept a cancellation flag and return
> a distinct error when it is set; `run_install` should stop the queue and emit
> `install-finished` with a cancelled state.

---

## 11.7 Checkpoint

- [ ] `cargo check` passes in `src-tauri`.
- [ ] `install.rs` is declared with `mod install;` in `lib.rs`.
- [ ] `start_install` and `packages_needed_for` are in `generate_handler!`.
- [ ] You can explain why `AppHandle` can cross into a background thread but `State` cannot.

```bash
git add -A && git commit -m "Add background install jobs with progress events"
```

---

## 11.8 Understanding check

1. Why does `start_install` return `Ok(())` before the work is finished?
2. What does `use tauri::Emitter;` provide, and what is the symptom of omitting it?
3. Why is the progress `stage` a `String` here when the engine models it as an enum?
4. Why does `run_install` ignore the result of every `emit` call?
5. The licence check exists in the UI flow *and* in `start_install`. Is that redundant?
