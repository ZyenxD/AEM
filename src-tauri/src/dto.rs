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
            size_mb: pkg
                .archive_for(host)
                .map(|a| a.size_bytes / 1_048_576)
                .unwrap_or(0),
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
/// Payload of the `install-progress` event.
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

/// Payload of the `install-finished` event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallFinishedDto {
    pub ok: bool,
    pub error: Option<String>,
}
