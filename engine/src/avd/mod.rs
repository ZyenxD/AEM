pub mod config_ini;
pub mod profiles;

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{EngineError, Result, io};
use crate::sdk_layout::{InstalledImage, SdkLayout, ini_path};
use config_ini::IniFile;
use profiles::DeviceProfile;

/// One virtual device on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Avd {
    pub name: String,
    /// The `<name>.avd` folder.
    pub dir: PathBuf,
    pub config: IniFile,
}

impl Avd {
    pub fn display_name(&self) -> String {
        self.config
            .get("avd.ini.displayname")
            .unwrap_or(&self.name)
            .to_string()
    }
    pub fn api_level(&self) -> Option<u32> {
        self.config
            .get("image.sysdir.1")?
            .split('/')
            .find_map(|s| s.strip_prefix("android-"))
            .and_then(|s| s.parse().ok())
    }
    pub fn abi(&self) -> Option<&str> {
        self.config.get("abi.type")
    }
    pub fn ram_mb(&self) -> Option<u32> {
        self.config.get_size_mb("hw.ramSize")
    }
    pub fn resolution(&self) -> Option<(u32, u32)> {
        Some((
            self.config.get_u32("hw.lcd.width")?,
            self.config.get_u32("hw.lcd.height")?,
        ))
    }
    pub fn config_path(&self) -> PathBuf {
        self.dir.join("config.ini")
    }
}

/// Fields the user may change after creation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AvdPatch {
    pub display_name: Option<String>,
    pub ram_mb: Option<u32>,
    pub heap_mb: Option<u32>,
    pub storage_mb: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub density: Option<u32>,
}

/// Every AVD in the given avd home folder.
pub fn list(avd_home: &Path) -> Result<Vec<Avd>> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(avd_home) else {
        return Ok(out); // no folder yet simply means no devices
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("ini") || !path.is_file() {
            continue;
        }
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let pointer = IniFile::read(&path)?;
        let dir = pointer
            .get("path")
            .map(PathBuf::from)
            .unwrap_or_else(|| avd_home.join(format!("{name}.avd")));

        let config_path = dir.join("config.ini");
        if !config_path.exists() {
            continue; // stale pointer
        }
        out.push(Avd {
            name,
            dir,
            config: IniFile::read(&config_path)?,
        });
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}

pub fn find(avd_home: &Path, name: &str) -> Result<Avd> {
    list(avd_home)?
        .into_iter()
        .find(|a| a.name == name)
        .ok_or_else(|| EngineError::NotFound(format!("device {name}")))
}

/// AVD names may only contain letters, digits, dots, dashes and underscores.
pub fn sanitize_name(input: &str) -> String {
    let cleaned: String = input
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "Device".to_string()
    } else {
        cleaned
    }
}

/// Create a new virtual device: writes `<name>.ini` and `<name>.avd/config.ini`.
pub fn create(
    avd_home: &Path,
    sdk: &SdkLayout,
    display_name: &str,
    profile: &DeviceProfile,
    image: &InstalledImage,
) -> Result<Avd> {
    let name = sanitize_name(display_name);
    let dir = avd_home.join(format!("{name}.avd"));
    if dir.exists() {
        return Err(EngineError::InvalidConfig(format!(
            "a device named {name} already exists"
        )));
    }
    fs::create_dir_all(&dir).map_err(|e| io(&dir, e))?;

    let play_store = image.tag == "google_apis_playstore";

    let mut config = IniFile::default();
    config
        .set("AvdId", name.clone())
        .set("avd.ini.displayname", display_name.trim())
        .set("avd.ini.encoding", "UTF-8")
        .set("abi.type", image.abi.clone())
        .set("tag.id", image.tag.clone())
        .set("tag.display", friendly_tag(&image.tag))
        .set("image.sysdir.1", image.sysdir_relative())
        .set(
            "hw.cpu.arch",
            if image.abi.starts_with("arm") {
                "arm64"
            } else {
                "x86_64"
            },
        )
        .set("hw.ramSize", profile.ram_mb.to_string())
        .set("vm.heapSize", profile.heap_mb.to_string())
        .set(
            "disk.dataPartition.size",
            format!("{}M", profile.storage_mb),
        )
        .set("hw.lcd.width", profile.width.to_string())
        .set("hw.lcd.height", profile.height.to_string())
        .set("hw.lcd.density", profile.density.to_string())
        .set("hw.device.name", profile.device_name.clone())
        .set("hw.device.manufacturer", profile.manufacturer.clone())
        .set("hw.gpu.enabled", "yes")
        .set("hw.gpu.mode", "auto")
        .set("hw.keyboard", "yes")
        .set("hw.audioInput", "yes")
        .set("hw.camera.back", "virtualscene")
        .set("hw.camera.front", "emulated")
        .set("hw.sdCard", "yes")
        .set("sdcard.size", "512M")
        .set("fastboot.forceColdBoot", "no")
        .set("runtime.network.latency", "none")
        .set("runtime.network.speed", "full")
        .set(
            "PlayStore.enabled",
            if play_store { "true" } else { "false" },
        );

    config.write(&dir.join("config.ini"))?;

    let mut pointer = IniFile::default();
    pointer
        .set("avd.ini.encoding", "UTF-8")
        .set("path", ini_path(&dir))
        .set("path.rel", format!("avd/{name}.avd"))
        .set("target", format!("android-{}", image.api_level));
    pointer.write(&avd_home.join(format!("{name}.ini")))?;

    let _ = sdk; // reserved: skins and hardware profiles come from the SDK in Phase 3

    Ok(Avd { name, dir, config })
}

/// Change settings of an existing device, keeping every other key untouched.
pub fn update(avd: &mut Avd, patch: AvdPatch) -> Result<()> {
    if let Some(v) = patch.display_name.as_deref() {
        avd.config.set("avd.ini.displayname", v.trim());
    }
    if let Some(v) = patch.ram_mb {
        avd.config.set("hw.ramSize", v.to_string());
    }
    if let Some(v) = patch.heap_mb {
        avd.config.set("vm.heapSize", v.to_string());
    }
    if let Some(v) = patch.storage_mb {
        avd.config.set("disk.dataPartition.size", format!("{v}M"));
    }
    if let Some(v) = patch.width {
        avd.config.set("hw.lcd.width", v.to_string());
    }
    if let Some(v) = patch.height {
        avd.config.set("hw.lcd.height", v.to_string());
    }
    if let Some(v) = patch.density {
        avd.config.set("hw.lcd.density", v.to_string());
    }
    let path = avd.config_path();
    avd.config.write(&path)
}

/// Remove the device folder and its pointer file.
pub fn delete(avd_home: &Path, avd: &Avd) -> Result<()> {
    if avd.dir.exists() {
        fs::remove_dir_all(&avd.dir).map_err(|e| io(&avd.dir, e))?;
    }
    let pointer = avd_home.join(format!("{}.ini", avd.name));
    if pointer.exists() {
        fs::remove_file(&pointer).map_err(|e| io(&pointer, e))?;
    }
    Ok(())
}

/// Delete the user data so the device boots as if it were new.
pub fn wipe_data(avd: &Avd) -> Result<()> {
    for name in ["userdata-qemu.img", "userdata-qemu.img.qcow2", "cache.img"] {
        let f = avd.dir.join(name);
        if f.exists() {
            fs::remove_file(&f).map_err(|e| io(&f, e))?;
        }
    }
    let snapshots = avd.dir.join("snapshots");
    if snapshots.exists() {
        fs::remove_dir_all(&snapshots).map_err(|e| io(&snapshots, e))?;
    }
    Ok(())
}

fn friendly_tag(tag: &str) -> &'static str {
    match tag {
        "google_apis_playstore" => "Google Play",
        "google_apis" => "Google APIs",
        "android-tv" => "Android TV",
        "default" => "Default Android System Image",
        _ => "System Image",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_image() -> InstalledImage {
        InstalledImage {
            package_path: "system-images;android-35;google_apis_playstore;arm64-v8a".into(),
            api_level: 35,
            tag: "google_apis_playstore".into(),
            abi: "arm64-v8a".into(),
            dir: PathBuf::from("/tmp/sdk/system-images/android-35/google_apis_playstore/arm64-v8a"),
        }
    }

    #[test]
    fn create_list_update_delete_round_trip() {
        let home = std::env::temp_dir().join("avdhub-avd-home");
        fs::remove_dir_all(&home).ok();
        fs::create_dir_all(&home).unwrap();
        let sdk = SdkLayout::new("/tmp/sdk", true);

        let created = create(
            &home,
            &sdk,
            "My Pixel 8",
            &profiles::default_profile(),
            &fake_image(),
        )
        .unwrap();
        assert_eq!(created.name, "My_Pixel_8");
        assert_eq!(created.display_name(), "My Pixel 8");
        assert_eq!(created.config.get("PlayStore.enabled"), Some("true"));

        let devices = list(&home).unwrap();
        assert_eq!(devices.len(), 1);
        let mut found = devices.into_iter().next().unwrap();
        assert_eq!(found.api_level(), Some(35));
        assert_eq!(found.abi(), Some("arm64-v8a"));
        assert_eq!(found.ram_mb(), Some(2048));

        update(
            &mut found,
            AvdPatch {
                ram_mb: Some(4096),
                ..Default::default()
            },
        )
        .unwrap();
        let reread = find(&home, "My_Pixel_8").unwrap();
        assert_eq!(reread.ram_mb(), Some(4096));
        // untouched keys survive
        assert_eq!(reread.config.get("hw.keyboard"), Some("yes"));

        delete(&home, &reread).unwrap();
        assert!(list(&home).unwrap().is_empty());
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn names_are_sanitised() {
        assert_eq!(sanitize_name("My Pixel 8!"), "My_Pixel_8_");
        assert_eq!(sanitize_name("   "), "Device");
    }
}
