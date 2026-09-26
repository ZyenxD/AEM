use serde::{Deserialize, Serialize};

use crate::error::{EngineError, Result};
use crate::host::{Abi, HostInfo};

pub const BASE_URL: &str = "https://dl.google.com/android/repository/";

pub const MANIFEST_URLS: &[&str] = &[
    "https://dl.google.com/android/repository/repository2-3.xml",
    "https://dl.google.com/android/repository/sys-img/android/sys-img2-3.xml",
    "https://dl.google.com/android/repository/sys-img/google_apis/sys-img2-3.xml",
    "https://dl.google.com/android/repository/sys-img/google_apis_playstore/sys-img2-3.xml",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Archive {
    pub url: String,
    pub size_bytes: u64,
    pub sha1: String,
    pub host_os: Option<String>,
    pub host_arch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemotePackage {
    pub path: String,
    pub display_name: String,
    pub revision: (u32, u32, u32),
    pub api_level: Option<u32>,
    pub abi: Option<String>,
    pub tag_id: Option<String>,
    pub tag_display: Option<String>,
    pub license_id: Option<String>,
    pub archives: Vec<Archive>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    pub packages: Vec<RemotePackage>,
    pub licenses: Vec<License>,
}

impl RemotePackage {
    /// The archive that runs on this machine, if any.
    pub fn archive_for(&self, host: &HostInfo) -> Option<&Archive> {
        self.archives.iter().find(|a| {
            a.host_os
                .as_deref()
                .is_none_or(|o| o == host.os.manifest_name())
                && a.host_arch
                    .as_deref()
                    .is_none_or(|x| x == host.arch.manifest_name())
        })
    }

    pub fn is_system_image(&self) -> bool {
        self.path.starts_with("system-images;")
    }

    /// Human label such as "Android 14 · Google Play".
    pub fn friendly_name(&self) -> String {
        match (self.api_level, self.tag_display.as_deref()) {
            (Some(api), Some(tag)) => format!("{} · {}", android_version_name(api), tag),
            (Some(api), None) => android_version_name(api).to_string(),
            _ => self.display_name.clone(),
        }
    }
}

impl Catalog {
    pub fn license_for<'a>(&'a self, pkg: &RemotePackage) -> Option<&'a License> {
        let id = pkg.license_id.as_deref()?;
        self.licenses.iter().find(|l| l.id == id)
    }

    /// System images this machine can actually run, newest API first.
    pub fn system_images(&self, host: &HostInfo) -> Vec<&RemotePackage> {
        let want: &str = host.preferred_abi.manifest_name();
        let mut out: Vec<&RemotePackage> = self
            .packages
            .iter()
            .filter(|p| {
                p.is_system_image()
                    && p.abi.as_deref() == Some(want)
                    && p.archive_for(host).is_some()
            })
            .collect();
        out.sort_by(|a, b| {
            b.api_level
                .cmp(&a.api_level)
                .then(b.revision.cmp(&a.revision))
        });
        out
    }

    /// Highest-revision build of a tool such as "emulator" or "platform-tools".
    pub fn latest_tool(&self, name: &str, host: &HostInfo) -> Option<&RemotePackage> {
        self.packages
            .iter()
            .filter(|p| {
                (p.path == name || p.path.starts_with(&format!("{name};")))
                    && p.archive_for(host).is_some()
            })
            .max_by_key(|p| p.revision)
    }

    pub fn find(&self, path: &str) -> Option<&RemotePackage> {
        self.packages.iter().find(|p| p.path == path)
    }
}

/// Download every manifest and merge them into one catalog.
pub fn fetch_catalog() -> Result<Catalog> {
    let mut catalog = Catalog::default();
    for url in MANIFEST_URLS {
        let xml = http_get_text(url)?;
        let part = parse_manifest(&xml)?;
        catalog.packages.extend(part.packages);
        for license in part.licenses {
            if !catalog.licenses.iter().any(|l| l.id == license.id) {
                catalog.licenses.push(license);
            }
        }
    }
    Ok(catalog)
}

fn http_get_text(url: &str) -> Result<String> {
    ureq::get(url)
        .call()
        .map_err(|e| EngineError::Network {
            url: url.to_string(),
            message: e.to_string(),
        })?
        .into_string()
        .map_err(|e| EngineError::Network {
            url: url.to_string(),
            message: e.to_string(),
        })
}

/// Parse one manifest document. Pure function: no network, easy to unit test.
pub fn parse_manifest(xml: &str) -> Result<Catalog> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| EngineError::Manifest(e.to_string()))?;

    let mut licenses = Vec::new();
    for node in doc.descendants().filter(|n| n.has_tag_name("license")) {
        if let (Some(id), Some(text)) = (node.attribute("id"), node.text()) {
            licenses.push(License {
                id: id.to_string(),
                text: text.trim().to_string(),
            });
        }
    }

    let mut packages = Vec::new();
    for node in doc
        .descendants()
        .filter(|n| n.has_tag_name("remotePackage"))
    {
        let path = node.attribute("path").unwrap_or_default().to_string();
        if path.is_empty() {
            continue;
        }

        let display_name = first_text(node, "display-name").unwrap_or_else(|| path.clone());
        let api_level = first_text(node, "api-level").and_then(|s| s.parse().ok());
        let abi = first_text(node, "abi");

        let tag = node.descendants().find(|n| n.has_tag_name("tag"));
        let tag_id = tag.and_then(|t| first_text(t, "id"));
        let tag_display = tag.and_then(|t| first_text(t, "display"));

        let license_id = node
            .descendants()
            .find(|n| n.has_tag_name("uses-license"))
            .and_then(|n| n.attribute("ref"))
            .map(|s| s.to_string());

        let revision = node
            .children()
            .find(|n| n.has_tag_name("revision"))
            .map(|r| {
                (
                    first_text(r, "major")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                    first_text(r, "minor")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                    first_text(r, "micro")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                )
            })
            .unwrap_or((0, 0, 0));

        let mut archives = Vec::new();
        for a in node.descendants().filter(|n| n.has_tag_name("archive")) {
            let raw_url = first_text(a, "url").unwrap_or_default();
            if raw_url.is_empty() {
                continue;
            }
            archives.push(Archive {
                url: if raw_url.starts_with("http") {
                    raw_url
                } else {
                    format!("{BASE_URL}{raw_url}")
                },
                size_bytes: first_text(a, "size")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
                sha1: first_text(a, "checksum").unwrap_or_default(),
                host_os: first_text(a, "host-os"),
                host_arch: first_text(a, "host-arch"),
            });
        }

        packages.push(RemotePackage {
            path,
            display_name,
            revision,
            api_level,
            abi,
            tag_id,
            tag_display,
            license_id,
            archives,
        });
    }

    Ok(Catalog { packages, licenses })
}

fn first_text(node: roxmltree::Node, name: &str) -> Option<String> {
    node.descendants()
        .find(|n| n.has_tag_name(name))
        .and_then(|n| n.text())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Android version marketing name for an API level (used in the UI).
pub fn android_version_name(api: u32) -> &'static str {
    match api {
        36 => "Android 16",
        35 => "Android 15",
        34 => "Android 14",
        33 => "Android 13",
        31 | 32 => "Android 12",
        30 => "Android 11",
        29 => "Android 10",
        28 => "Android 9",
        26 | 27 => "Android 8",
        _ => "Android",
    }
}

/// Pretty ABI label for the UI.
pub fn abi_label(abi: Abi) -> &'static str {
    match abi {
        Abi::Arm64V8a => "Apple Silicon / ARM",
        Abi::X86_64 => "Intel / AMD",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{HostArch, HostOs};

    const SAMPLE: &str = include_str!("../tests/fixtures/sample_manifest.xml");

    fn mac_arm() -> HostInfo {
        HostInfo {
            os: HostOs::MacOs,
            arch: HostArch::Aarch64,
            preferred_abi: Abi::Arm64V8a,
            total_ram_bytes: 0,
        }
    }

    #[test]
    fn parses_packages_and_licenses() {
        let cat = parse_manifest(SAMPLE).expect("should parse");
        assert!(!cat.packages.is_empty());
        assert!(cat.licenses.iter().any(|l| l.id == "android-sdk-license"));

        let img = cat
            .find("system-images;android-35;google_apis;arm64-v8a")
            .expect("image present");
        assert_eq!(img.api_level, Some(35));
        assert_eq!(img.abi.as_deref(), Some("arm64-v8a"));
        assert_eq!(img.license_id.as_deref(), Some("android-sdk-license"));
        assert_eq!(img.archives.len(), 1);
        assert!(img.archives[0].url.starts_with("https://"));
    }

    #[test]
    fn picks_archive_for_host() {
        let cat = parse_manifest(SAMPLE).unwrap();
        let host = mac_arm();

        let emu = cat
            .latest_tool("emulator", &host)
            .expect("emulator present");
        assert!(
            emu.archive_for(&host)
                .unwrap()
                .url
                .contains("darwin_aarch64")
        );

        let images = cat.system_images(&host);
        assert!(images.iter().all(|p| p.abi.as_deref() == Some("arm64-v8a")));
        assert_eq!(images.first().unwrap().api_level, Some(35));
    }
}
