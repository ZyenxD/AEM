use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, io};
use crate::host::HostOs;

/// An Android SDK directory we can read from and install into.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkLayout {
    pub root: PathBuf,
    /// true when this folder belongs to us (not to Android Studio).
    pub managed_by_us: bool,
}

/// A system image already unpacked on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledImage {
    /// "system-images;android-35;google_apis;arm64-v8a"
    pub package_path: String,
    pub api_level: u32,
    pub tag: String,
    pub abi: String,
    pub dir: PathBuf,
}

impl InstalledImage {
    /// Value the AVD's config.ini stores in `image.sysdir.1` (relative, forward slashes).
    pub fn sysdir_relative(&self) -> String {
        format!(
            "system-images/android-{}/{}/{}/",
            self.api_level, self.tag, self.abi
        )
    }
}

impl SdkLayout {
    pub fn new(root: impl Into<PathBuf>, managed_by_us: bool) -> Self {
        SdkLayout {
            root: root.into(),
            managed_by_us,
        }
    }
    /// Every SDK we can find, ours first.
    pub fn discover() -> Vec<SdkLayout> {
        let mut found: Vec<SdkLayout> = Vec::new();

        if let Some(ours) = managed_root()
            && ours.exists()
        {
            found.push(SdkLayout::new(ours, true));
        }

        for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
            if let Ok(value) = std::env::var(var) {
                let p = PathBuf::from(value);
                if p.exists() {
                    found.push(SdkLayout::new(p, false));
                }
            }
        }

        for p in studio_default_roots() {
            if p.exists() {
                found.push(SdkLayout::new(p, false));
            }
        }

        // de-duplicate by path
        let mut seen: Vec<PathBuf> = Vec::new();
        found.retain(|s| {
            if seen.contains(&s.root) {
                false
            } else {
                seen.push(s.root.clone());
                true
            }
        });
        found
    }

    /// Create (if needed) the SDK folder this app owns.
    pub fn create_managed() -> Result<SdkLayout> {
        let root = managed_root()
            .ok_or_else(|| crate::EngineError::NotFound("no writable app data folder".into()))?;
        fs::create_dir_all(&root).map_err(|e| io(&root, e))?;
        Ok(SdkLayout::new(root, true))
    }

    /// "system-images;android-35;x;y" -> root/system-images/android-35/x/y
    pub fn install_dir_for(&self, package_path: &str) -> PathBuf {
        let mut dir = self.root.clone();
        for segment in package_path.split(';') {
            dir.push(segment);
        }
        dir
    }

    pub fn emulator_bin(&self) -> Option<PathBuf> {
        let name = if HostOs::current() == HostOs::Windows {
            "emulator.exe"
        } else {
            "emulator"
        };
        existing(self.root.join("emulator").join(name))
    }

    pub fn adb_bin(&self) -> Option<PathBuf> {
        let name = if HostOs::current() == HostOs::Windows {
            "adb.exe"
        } else {
            "adb"
        };
        existing(self.root.join("platform-tools").join(name))
    }

    pub fn has_core_tools(&self) -> bool {
        self.emulator_bin().is_some() && self.adb_bin().is_some()
    }

    /// Scan system-images/android-XX/<tag>/<abi> directories.
    pub fn installed_images(&self) -> Vec<InstalledImage> {
        let mut out = Vec::new();
        let base = self.root.join("system-images");
        let Ok(levels) = fs::read_dir(&base) else {
            return out;
        };
        for level in levels.flatten() {
            let level_name = level.file_name().to_string_lossy().to_string();
            let Some(api) = level_name
                .strip_prefix("android-")
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };

            let Ok(tags) = fs::read_dir(level.path()) else {
                continue;
            };
            for tag in tags.flatten() {
                let tag_name = tag.file_name().to_string_lossy().to_string();
                let Ok(abis) = fs::read_dir(tag.path()) else {
                    continue;
                };
                for abi in abis.flatten() {
                    let abi_name = abi.file_name().to_string_lossy().to_string();
                    let dir = abi.path();
                    // a real image always ships system.img or a kernel
                    let looks_valid = dir.join("system.img").exists()
                        || dir.join("kernel-ranchu").exists()
                        || dir.join("userdata.img").exists();
                    if !looks_valid {
                        continue;
                    }
                    out.push(InstalledImage {
                        package_path: format!("system-images;android-{api};{tag_name};{abi_name}"),
                        api_level: api,
                        tag: tag_name.clone(),
                        abi: abi_name,
                        dir,
                    });
                }
            }
        }
        out.sort_by_key(|i| std::cmp::Reverse(i.api_level));
        out
    }
}

fn existing(p: PathBuf) -> Option<PathBuf> {
    if p.exists() { Some(p) } else { None }
}

/// The SDK folder this app owns, e.g.
/// macOS:   ~/Library/Application Support/avdhub/sdk
/// Windows: %LOCALAPPDATA%\avdhub\sdk
pub fn managed_root() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("avdhub").join("sdk"))
}

fn studio_default_roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(home) = dirs::home_dir() {
        match HostOs::current() {
            HostOs::MacOs => v.push(home.join("Library/Android/sdk")),
            HostOs::Linux => v.push(home.join("Android/Sdk")),
            HostOs::Windows => {}
        }
    }
    if let Some(local) = dirs::data_local_dir() {
        v.push(local.join("Android").join("Sdk"));
    }
    v
}

/// Where AVD definitions live (`~/.android/avd` unless overridden).
pub fn avd_home() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("ANDROID_AVD_HOME") {
        return Some(PathBuf::from(explicit));
    }
    if let Ok(sdk_home) = std::env::var("ANDROID_SDK_HOME") {
        return Some(PathBuf::from(sdk_home).join(".android").join("avd"));
    }
    dirs::home_dir().map(|h| h.join(".android").join("avd"))
}

/// Convert a path to the forward-slash form the emulator's ini files expect.
pub fn ini_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_dir_maps_semicolons_to_folders() {
        let sdk = SdkLayout::new("/tmp/sdk", true);
        assert_eq!(
            sdk.install_dir_for("system-images;android-35;google_apis;arm64-v8a"),
            PathBuf::from("/tmp/sdk/system-images/android-35/google_apis/arm64-v8a")
        );
        assert_eq!(
            sdk.install_dir_for("platform-tools"),
            PathBuf::from("/tmp/sdk/platform-tools")
        );
    }

    #[test]
    fn finds_installed_images() {
        let tmp = std::env::temp_dir().join("avdhub-test-sdk");
        let img = tmp.join("system-images/android-34/google_apis/arm64-v8a");
        fs::create_dir_all(&img).unwrap();
        fs::write(img.join("system.img"), b"fake").unwrap();

        let sdk = SdkLayout::new(&tmp, true);
        let found = sdk.installed_images();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].api_level, 34);
        assert_eq!(found[0].abi, "arm64-v8a");
        assert_eq!(
            found[0].sysdir_relative(),
            "system-images/android-34/google_apis/arm64-v8a/"
        );

        fs::remove_dir_all(&tmp).ok();
    }
}
