use std::fs::{self, File};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

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
///
/// The device's system image may live in a different SDK than ours (a device
/// made by Android Studio uses Android Studio's images), so the SDK root handed
/// to the emulator is the one that actually holds the image.
pub fn launch(sdk: &SdkLayout, avd: &Avd, options: &LaunchOptions) -> Result<u32> {
    let emulator = sdk
        .emulator_bin()
        .ok_or_else(|| EngineError::NotFound("the emulator package is not installed yet".into()))?;

    let image_sdk = sdk_for_avd(avd, sdk, &SdkLayout::discover()).ok_or_else(|| {
        EngineError::NotFound(format!(
            "the Android version {} uses is not installed yet ({})",
            avd.display_name(),
            avd.config.get("image.sysdir.1").unwrap_or("unknown image"),
        ))
    })?;

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

    // The emulator's output goes to a log in the temp folder (never into the
    // device's own folder), so a failed start can be explained.
    let log_path = std::env::temp_dir().join(format!("avdhub-emulator-{}.log", avd.name));
    let (stdout, stderr) = match File::create(&log_path) {
        Ok(file) => match file.try_clone() {
            Ok(copy) => (Stdio::from(file), Stdio::from(copy)),
            Err(_) => (Stdio::null(), Stdio::null()),
        },
        Err(_) => (Stdio::null(), Stdio::null()),
    };

    // The emulator resolves the device's image relative to the SDK root.
    command
        .env("ANDROID_SDK_ROOT", &image_sdk.root)
        .env("ANDROID_HOME", &image_sdk.root)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);

    hide_console(&mut command);

    let mut child = command
        .spawn()
        .map_err(|e| EngineError::Process(format!("could not start the emulator: {e}")))?;

    // A broken setup makes the emulator quit within a second or two. Wait
    // briefly so that failure is reported instead of a silent no-op.
    for _ in 0..EARLY_EXIT_POLLS {
        std::thread::sleep(Duration::from_millis(250));
        if let Ok(Some(status)) = child.try_wait() {
            let reason = fs::read_to_string(&log_path)
                .ok()
                .and_then(|log| first_fatal_line(&log))
                .unwrap_or_else(|| format!("exit status {status}"));
            return Err(EngineError::Process(format!(
                "the emulator closed right after starting: {reason}"
            )));
        }
    }
    Ok(child.id())
}

/// 12 x 250 ms: long enough to catch a fatal start-up error.
const EARLY_EXIT_POLLS: u32 = 12;

/// The SDK holding this device's system image: `preferred` when it has it,
/// otherwise the first of `candidates` that does. `None` if none has it.
pub fn sdk_for_avd(
    avd: &Avd,
    preferred: &SdkLayout,
    candidates: &[SdkLayout],
) -> Option<SdkLayout> {
    let Some(sysdir) = avd.config.get("image.sysdir.1") else {
        // Nothing to check against: keep the old behaviour.
        return Some(preferred.clone());
    };
    std::iter::once(preferred)
        .chain(candidates)
        .find(|s| s.root.join(sysdir).is_dir())
        .cloned()
}

fn first_fatal_line(log: &str) -> Option<String> {
    log.lines()
        .find(|l| l.contains("FATAL") || l.contains("ERROR"))
        .map(|l| {
            l.split_once('|')
                .map_or(l, |(_, rest)| rest)
                .trim()
                .to_string()
        })
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

/// Start the adb server up front so later calls are fast. Errors are ignored:
/// the first real adb call will report them.
pub fn warm_up(sdk: &SdkLayout) {
    if let Some(adb) = sdk.adb_bin() {
        let mut command = Command::new(adb);
        command
            .arg("start-server")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        hide_console(&mut command);
        let _ = command.status();
    }
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
pub(crate) fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub(crate) fn hide_console(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avd::config_ini::IniFile;

    fn avd_with_sysdir(sysdir: &str) -> Avd {
        let mut config = IniFile::default();
        config.set("image.sysdir.1", sysdir);
        Avd {
            name: "Test".into(),
            dir: "/tmp/Test.avd".into(),
            config,
        }
    }

    #[test]
    fn picks_the_sdk_that_holds_the_image() {
        let base = std::env::temp_dir().join("avdhub-sdk-pick");
        fs::remove_dir_all(&base).ok();
        let ours = SdkLayout::new(base.join("ours"), true);
        let studio = SdkLayout::new(base.join("studio"), false);
        fs::create_dir_all(
            ours.root
                .join("system-images/android-36/google_apis_playstore/arm64-v8a"),
        )
        .unwrap();
        fs::create_dir_all(
            studio
                .root
                .join("system-images/android-35/google_apis_playstore/arm64-v8a"),
        )
        .unwrap();
        let candidates = [ours.clone(), studio.clone()];

        // Only Android Studio has API 35: use its SDK.
        let avd = avd_with_sysdir("system-images/android-35/google_apis_playstore/arm64-v8a/");
        assert_eq!(
            sdk_for_avd(&avd, &ours, &candidates).unwrap().root,
            studio.root
        );

        // Ours has API 36: keep ours.
        let avd = avd_with_sysdir("system-images/android-36/google_apis_playstore/arm64-v8a/");
        assert_eq!(
            sdk_for_avd(&avd, &ours, &candidates).unwrap().root,
            ours.root
        );

        // Nobody has API 34.
        let avd = avd_with_sysdir("system-images/android-34/google_apis/arm64-v8a/");
        assert!(sdk_for_avd(&avd, &ours, &candidates).is_none());

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn reads_the_reason_from_the_emulator_log() {
        let log = "INFO | Android emulator version 37\nFATAL        | Broken AVD system path. Check your ANDROID_SDK_ROOT value [/x]!\n";
        assert_eq!(
            first_fatal_line(log).unwrap(),
            "Broken AVD system path. Check your ANDROID_SDK_ROOT value [/x]!"
        );
        assert!(first_fatal_line("INFO | all good").is_none());
    }
}
