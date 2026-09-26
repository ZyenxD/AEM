use serde::{Deserialize, Serialize};

/// A ready-made hardware shape the user picks in the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProfile {
    pub id: String,
    pub label: String,
    pub device_name: String,
    pub manufacturer: String,
    pub width: u32,
    pub height: u32,
    pub density: u32,
    pub ram_mb: u32,
    pub heap_mb: u32,
    pub storage_mb: u32,
}

#[allow(clippy::too_many_arguments)]
fn profile(
    id: &str,
    label: &str,
    device_name: &str,
    width: u32,
    height: u32,
    density: u32,
    ram_mb: u32,
    heap_mb: u32,
    storage_mb: u32,
) -> DeviceProfile {
    DeviceProfile {
        id: id.into(),
        label: label.into(),
        device_name: device_name.into(),
        manufacturer: "Google".into(),
        width,
        height,
        density,
        ram_mb,
        heap_mb,
        storage_mb,
    }
}

/// Six curated presets instead of Android Studio's overwhelming list.
pub fn builtin() -> Vec<DeviceProfile> {
    vec![
        profile(
            "phone",
            "Modern phone",
            "pixel_8",
            1080,
            2400,
            420,
            2048,
            512,
            6144,
        ),
        profile(
            "phone_large",
            "Large phone",
            "pixel_8_pro",
            1344,
            2992,
            480,
            3072,
            512,
            8192,
        ),
        profile(
            "phone_small",
            "Compact phone",
            "pixel_4a",
            1080,
            2340,
            440,
            2048,
            384,
            4096,
        ),
        profile(
            "tablet",
            "Tablet",
            "pixel_tablet",
            1600,
            2560,
            320,
            4096,
            768,
            8192,
        ),
        profile(
            "foldable",
            "Foldable",
            "pixel_fold",
            2208,
            1840,
            420,
            4096,
            768,
            8192,
        ),
        profile(
            "tv",
            "Android TV (1080p)",
            "tv_1080p",
            1920,
            1080,
            320,
            2048,
            512,
            4096,
        ),
    ]
}

pub fn by_id(id: &str) -> Option<DeviceProfile> {
    builtin().into_iter().find(|p| p.id == id)
}

pub fn default_profile() -> DeviceProfile {
    by_id("phone").expect("phone profile always exists")
}
