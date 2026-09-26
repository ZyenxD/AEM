use std::path::Path;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::avd::Avd;
use crate::error::{EngineError, Result};
use crate::sdk_layout::SdkLayout;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunningEmulator {
    /// adb serial such as "emulator-5554"
    pub serial: String,
    /// AVD name, when adb could tell us
    pub avd_name: Option<String>,
    /// true once Android has finished booting
    pub booted: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LaunchOptions {
    /// Ignore the saved snapshot and boot from scratch.
    pub cold_boot: bool,
    /// Erase user data before booting.
    pub wipe_data: bool,
    /// Extra flags for power users.
    pub extra_args: Vec<String>,
}

/// Start an emulator. The child is detached: closing our app leaves it running.
pub fn launch(sdk: &SdkLayout, avd: &Avd, options: &LaunchOptions) -> Result<u32> {
    let emulator = sdk
        .emulator_bin()
        .ok_or_else(|| EngineError::NotFound("the emulator package is not installed yet".into()))?;

    let mut command = Command::new(&emulator);
    command.arg("-avd").arg(&avd.name);
    if options.cold_boot {
        command.arg("-no-snapshot-load");
    }
    if options.wipe_data {
        command.arg("-wipe-data");
    }
    for arg in &options.extra_args {
        command.arg(arg);
    }

    // The emulator resolves its own files relative to the SDK root.
    command
        .env("ANDROID_SDK_ROOT", &sdk.root)
        .env("ANDROID_HOME", &sdk.root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    hide_console(&mut command);

    let child = command
        .spawn()
        .map_err(|e| EngineError::Process(format!("could not start the emulator: {e}")))?;
    Ok(child.id())
}

/// Emulators adb can currently see.
pub fn running(sdk: &SdkLayout) -> Result<Vec<RunningEmulator>> {
    let Some(adb) = sdk.adb_bin() else {
        return Ok(Vec::new());
    };

    let output = run_capture(&adb, &["devices"])?;
    let mut list = Vec::new();

    for line in output.lines().skip(1) {
        let mut parts = line.split_whitespace();
        let (Some(serial), Some(state)) = (parts.next(), parts.next()) else {
            continue;
        };
        if !serial.starts_with("emulator-") {
            continue;
        }

        let online = state == "device";
        let avd_name = if online {
            run_capture(&adb, &["-s", serial, "emu", "avd", "name"])
                .ok()
                .and_then(|out| out.lines().next().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty() && s != "OK")
        } else {
            None
        };
        let booted = online
            && run_capture(
                &adb,
                &["-s", serial, "shell", "getprop", "sys.boot_completed"],
            )
            .map(|o| o.trim() == "1")
            .unwrap_or(false);

        list.push(RunningEmulator {
            serial: serial.to_string(),
            avd_name,
            booted,
        });
    }
    Ok(list)
}

pub fn is_running(sdk: &SdkLayout, avd_name: &str) -> bool {
    running(sdk)
        .map(|list| list.iter().any(|e| e.avd_name.as_deref() == Some(avd_name)))
        .unwrap_or(false)
}

/// Ask an emulator to shut down cleanly.
pub fn stop(sdk: &SdkLayout, emulator: &RunningEmulator) -> Result<()> {
    let adb = sdk
        .adb_bin()
        .ok_or_else(|| EngineError::NotFound("platform-tools are not installed".into()))?;
    run_capture(&adb, &["-s", &emulator.serial, "emu", "kill"])?;
    Ok(())
}

fn run_capture(program: &Path, args: &[&str]) -> Result<String> {
    let mut command = Command::new(program);
    command.args(args);
    hide_console(&mut command);
    let output = command
        .output()
        .map_err(|e| EngineError::Process(format!("{} failed: {e}", program.display())))?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Stop Windows from flashing a console window for every adb call.
#[cfg(windows)]
fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_command: &mut Command) {}
