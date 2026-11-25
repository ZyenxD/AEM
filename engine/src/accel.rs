use serde::{Deserialize, Serialize};

use crate::error::{EngineError, Result};
use crate::host::HostOs;
use crate::sdk_layout::SdkLayout;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccelStatus {
    /// Hardware acceleration is available. Emulators will run at full speed.
    Ready,
    /// The machine supports it, but the Windows feature is switched off.
    HypervisorDisabled,
    /// The CPU's virtualisation extensions are off in the BIOS/UEFI.
    VirtualizationDisabled,
    /// The emulator package is not installed, so we cannot ask.
    EmulatorMissing,
    /// We ran the check but could not classify the answer.
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixStep {
    pub title: String,
    pub detail: String,
    /// A command the user can copy, if one applies.
    pub command: Option<String>,
    pub requires_admin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccelReport {
    pub status: AccelStatus,
    /// Short sentence for the user.
    pub summary: String,
    /// The emulator's own message, for the details view.
    pub raw_message: String,
    pub fix_steps: Vec<FixStep>,
    pub requires_reboot: bool,
}

impl AccelReport {
    pub fn is_ready(&self) -> bool {
        self.status == AccelStatus::Ready
    }
}

/// Ask the emulator whether hardware acceleration is available.
pub fn check(sdk: &SdkLayout) -> Result<AccelReport> {
    let Some(emulator) = sdk.emulator_bin() else {
        return Ok(AccelReport {
            status: AccelStatus::EmulatorMissing,
            summary: "The Android emulator is not installed yet.".to_string(),
            raw_message: String::new(),
            fix_steps: vec![],
            requires_reboot: false,
        });
    };

    let mut command = std::process::Command::new(&emulator);
    command.arg("-accel-check");
    crate::process::hide_console(&mut command);

    let output = command
        .output()
        .map_err(|e| EngineError::Process(format!("could not run the emulator: {e}")))?;

    // Different emulator versions print the diagnostic to different streams.
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(classify_with_firmware(
        &text,
        HostOs::current(),
        firmware_virtualization_enabled(),
    ))
}

/// Turn the emulator's output into a verdict. Pure function: unit-testable.
pub fn classify(output: &str, os: HostOs) -> AccelReport {
    classify_with_firmware(output, os, None)
}

/// As `classify`, plus what the CPU says about firmware virtualisation.
/// `firmware = Some(false)` outranks the message: the BIOS is the real blocker,
/// whatever the emulator chose to complain about.
pub fn classify_with_firmware(output: &str, os: HostOs, firmware: Option<bool>) -> AccelReport {
    let lowered = output.to_lowercase();
    let message = extract_message(output);

    // The line after "accel:" is 0 when acceleration is usable. The substring
    // check catches format drift; anything else fails toward "needs attention".
    let accel_ok = output
        .lines()
        .map(str::trim)
        .skip_while(|l| !l.starts_with("accel:"))
        .nth(1)
        .map(|l| l == "0")
        .unwrap_or(false)
        || lowered.contains("is installed and usable");

    if accel_ok {
        return AccelReport {
            status: AccelStatus::Ready,
            summary: "Hardware acceleration is working.".to_string(),
            raw_message: message,
            fix_steps: vec![],
            requires_reboot: false,
        };
    }

    match os {
        HostOs::Windows => {
            if firmware == Some(false) {
                return windows_firmware_report(message);
            }
            classify_windows(&lowered, message)
        }
        HostOs::MacOs => AccelReport {
            status: AccelStatus::Unknown,
            summary: "Hardware acceleration does not appear to be available.".to_string(),
            raw_message: message,
            fix_steps: vec![FixStep {
                title: "Restart your Mac".to_string(),
                detail: "macOS provides acceleration automatically. A restart usually \
                         resolves this; if it persists, please report it."
                    .to_string(),
                command: None,
                requires_admin: false,
            }],
            requires_reboot: false,
        },
        HostOs::Linux => AccelReport {
            status: AccelStatus::HypervisorDisabled,
            summary: "KVM is not available.".to_string(),
            raw_message: message,
            fix_steps: vec![FixStep {
                title: "Enable KVM".to_string(),
                detail: "Install KVM and add your user to the kvm group, then log out and \
                         back in."
                    .to_string(),
                command: Some("sudo usermod -aG kvm $USER".to_string()),
                requires_admin: true,
            }],
            requires_reboot: false,
        },
    }
}

fn extract_message(output: &str) -> String {
    output
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && *l != "accel:" && *l != "accel" && l.parse::<i32>().is_err())
        .unwrap_or("")
        .to_string()
}

/// Does the CPU report virtualisation enabled in firmware?
///
/// `None` when we cannot tell (not Windows, or the query failed), which the
/// classifier treats as "no opinion" rather than as a verdict.
#[cfg(windows)]
pub fn firmware_virtualization_enabled() -> Option<bool> {
    let mut command = std::process::Command::new("powershell");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "(Get-CimInstance Win32_Processor | Select-Object -First 1).VirtualizationFirmwareEnabled",
    ]);
    crate::process::hide_console(&mut command);

    let output = command.output().ok()?;
    match String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[cfg(not(windows))]
pub fn firmware_virtualization_enabled() -> Option<bool> {
    None
}

fn classify_windows(lowered: &str, message: String) -> AccelReport {
    // The CPU's virtualisation extensions are switched off in firmware.
    let firmware_off = lowered.contains("vt-x")
        || lowered.contains("svm")
        || lowered.contains("bios")
        || lowered.contains("not supported by the host cpu")
        || lowered.contains("virtualization is not enabled");

    if firmware_off {
        return windows_firmware_report(message);
    }

    // Otherwise: the Windows Hypervisor Platform feature is not enabled.
    AccelReport {
        status: AccelStatus::HypervisorDisabled,
        summary: "Windows needs one feature switched on before emulators can run at full speed."
            .to_string(),
        raw_message: message,
        fix_steps: vec![
            FixStep {
                title: "Turn on Windows Hypervisor Platform".to_string(),
                detail: "Open \"Turn Windows features on or off\", tick \"Windows \
                         Hypervisor Platform\", and press OK. Windows will ask to restart."
                    .to_string(),
                command: None,
                requires_admin: true,
            },
            FixStep {
                title: "Or run this command as administrator".to_string(),
                detail: "Right-click the Start button, choose Terminal (Admin), and paste \
                         this command."
                    .to_string(),
                command: Some(
                    "DISM /Online /Enable-Feature /All /FeatureName:HypervisorPlatform".to_string(),
                ),
                requires_admin: true,
            },
            FixStep {
                title: "Restart your computer".to_string(),
                detail: "The change only takes effect after a restart.".to_string(),
                command: None,
                requires_admin: false,
            },
        ],
        requires_reboot: true,
    }
}

fn windows_firmware_report(message: String) -> AccelReport {
    AccelReport {
        status: AccelStatus::VirtualizationDisabled,
        summary: "Your computer's virtualisation feature is switched off.".to_string(),
        raw_message: message,
        fix_steps: vec![
            FixStep {
                title: "Check Task Manager".to_string(),
                detail: "Press Ctrl+Shift+Esc, open the Performance tab, select CPU, \
                         and look for \"Virtualization\". If it says Disabled, it must \
                         be switched on in your computer's firmware."
                    .to_string(),
                command: None,
                requires_admin: false,
            },
            FixStep {
                title: "Turn on virtualisation in the BIOS/UEFI".to_string(),
                detail: "Restart and open the firmware settings (usually F2, F10 or \
                         Del during startup). Look for \"Intel VT-x\", \"AMD-V\", \
                         \"SVM Mode\" or \"Virtualization Technology\" and enable it. \
                         The exact name depends on your computer's manufacturer."
                    .to_string(),
                command: None,
                requires_admin: false,
            },
        ],
        requires_reboot: true,
    }
}

/// Open the Windows "Turn Windows features on or off" dialog.
/// Does nothing on other platforms. Never runs anything elevated itself.
pub fn open_windows_features() -> Result<()> {
    if HostOs::current() != HostOs::Windows {
        return Ok(());
    }
    let mut command = std::process::Command::new("optionalfeatures.exe");
    crate::process::hide_console(&mut command);
    command
        .spawn()
        .map_err(|e| EngineError::Process(format!("could not open Windows features: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real output from emulator 37.4.1.0 (build 16916650) on Windows 10 19045.
    // Captured 2026-11-19 (WHPX off) and 2026-11-21 (WHPX on).
    const WINDOWS_OK: &str = include_str!("../tests/fixtures/accel_windows_ok.txt");
    const WINDOWS_NO_WHPX: &str = include_str!("../tests/fixtures/accel_windows_no_whpx.txt");
    // Still invented: nobody has turned VT-x off in a BIOS yet. See Step A6.
    const WINDOWS_NO_VTX: &str =
        "accel:\n1\nVT-x is not supported by the host CPU or is disabled in the BIOS\naccel\n";
    const LINUX_OK: &str = "accel:\n0\nKVM (version 12) is installed and usable.\n";

    // Real output captured on macOS 27, emulator 37.3.1.0.
    const MAC_OK: &str = "accel:\n0\nHypervisor.Framework OS X Version 27.0\naccel\n";

    #[test]
    fn detects_working_acceleration_on_real_mac_output() {
        let report = classify(MAC_OK, HostOs::MacOs);
        assert!(report.is_ready());
        assert_eq!(report.raw_message, "Hypervisor.Framework OS X Version 27.0");
    }

    #[test]
    fn detects_working_acceleration() {
        let report = classify(WINDOWS_OK, HostOs::Windows);
        assert_eq!(report.status, AccelStatus::Ready);
        assert!(report.fix_steps.is_empty());
        assert!(report.raw_message.contains("WHPX"));

        assert_eq!(classify(LINUX_OK, HostOs::Linux).status, AccelStatus::Ready);
    }

    #[test]
    fn detects_missing_hypervisor_feature() {
        let report = classify(WINDOWS_NO_WHPX, HostOs::Windows);
        assert_eq!(report.status, AccelStatus::HypervisorDisabled);
        assert!(report.requires_reboot);
        assert!(report.fix_steps.iter().any(|s| {
            s.command
                .as_deref()
                .is_some_and(|c| c.contains("HypervisorPlatform"))
        }));
    }

    #[test]
    fn detects_firmware_virtualization_off() {
        let report = classify(WINDOWS_NO_VTX, HostOs::Windows);
        assert_eq!(report.status, AccelStatus::VirtualizationDisabled);
        assert!(report.fix_steps[0].title.contains("Task Manager"));
    }

    #[test]
    fn real_whpx_off_message_names_aehd_and_still_gives_whpx_steps() {
        // The emulator blames its own driver, not WHPX. The verdict is correct
        // only because nothing matches the firmware keywords and we fall through.
        assert!(WINDOWS_NO_WHPX.to_lowercase().contains("hypervisor driver"));
        assert!(!WINDOWS_NO_WHPX.to_lowercase().contains("whpx"));

        let report = classify(WINDOWS_NO_WHPX, HostOs::Windows);
        assert_eq!(report.status, AccelStatus::HypervisorDisabled);
        assert!(report.requires_reboot);
        assert!(
            report.fix_steps.iter().any(|s| s
                .command
                .as_deref()
                .is_some_and(|c| c.contains("HypervisorPlatform"))),
            "the user must be pointed at WHPX, never at AEHD (it sunsets 2026-12-31)"
        );
    }

    #[test]
    fn firmware_off_outranks_the_emulator_message() {
        // Even the AEHD message means "fix your BIOS" when the CPU says so.
        let report = classify_with_firmware(WINDOWS_NO_WHPX, HostOs::Windows, Some(false));
        assert_eq!(report.status, AccelStatus::VirtualizationDisabled);
        assert!(report.fix_steps[0].title.contains("Task Manager"));

        // With firmware on, or unknown, the message decides as before.
        assert_eq!(
            classify_with_firmware(WINDOWS_NO_WHPX, HostOs::Windows, Some(true)).status,
            AccelStatus::HypervisorDisabled
        );
        assert_eq!(
            classify_with_firmware(WINDOWS_NO_WHPX, HostOs::Windows, None).status,
            AccelStatus::HypervisorDisabled
        );
    }

    #[test]
    fn unknown_output_is_not_reported_as_ready() {
        let report = classify("something unexpected", HostOs::Windows);
        assert_ne!(report.status, AccelStatus::Ready);
    }

    #[test]
    fn status_serialises_as_camel_case_strings() {
        let json = serde_json::to_string(&AccelStatus::HypervisorDisabled).unwrap();
        assert_eq!(json, "\"hypervisorDisabled\"");
        let report = serde_json::to_value(classify(WINDOWS_NO_WHPX, HostOs::Windows)).unwrap();
        assert!(report.get("fixSteps").is_some());
        assert!(report.get("requiresReboot").is_some());
        assert!(report["fixSteps"][0].get("requiresAdmin").is_some());
    }
}
