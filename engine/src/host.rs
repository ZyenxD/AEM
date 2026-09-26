use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostOs {
    MacOs,
    Windows,
    Linux,
}

impl HostOs {
    pub fn current() -> HostOs {
        match std::env::consts::OS {
            "macos" => HostOs::MacOs,
            "windows" => HostOs::Windows,
            _ => HostOs::Linux,
        }
    }

    /// The spelling Google's manifest uses.
    pub fn manifest_name(self) -> &'static str {
        match self {
            HostOs::MacOs => "macosx",
            HostOs::Windows => "windows",
            HostOs::Linux => "linux",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostArch {
    Aarch64,
    X64,
}

impl HostArch {
    pub fn current() -> HostArch {
        match std::env::consts::ARCH {
            "aarch64" => HostArch::Aarch64,
            _ => HostArch::X64,
        }
    }

    pub fn manifest_name(self) -> &'static str {
        match self {
            HostArch::Aarch64 => "aarch64",
            HostArch::X64 => "x64",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Abi {
    Arm64V8a,
    X86_64,
}

impl Abi {
    pub fn manifest_name(self) -> &'static str {
        match self {
            Abi::Arm64V8a => "arm64-v8a",
            Abi::X86_64 => "x86_64",
        }
    }

    /// Value for config.ini's `hw.cpu.arch`.
    pub fn cpu_arch(self) -> &'static str {
        match self {
            Abi::Arm64V8a => "arm64",
            Abi::X86_64 => "x86_64",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostInfo {
    pub os: HostOs,
    pub arch: HostArch,
    pub preferred_abi: Abi,
    pub total_ram_bytes: u64,
}

pub fn detect() -> HostInfo {
    let os = HostOs::current();
    let arch = HostArch::current();
    let preferred_abi = match arch {
        HostArch::Aarch64 => Abi::Arm64V8a,
        HostArch::X64 => Abi::X86_64,
    };

    let mut sys = sysinfo::System::new();
    sys.refresh_memory();

    HostInfo {
        os,
        arch,
        preferred_abi,
        total_ram_bytes: sys.total_memory(),
    }
}

/// Free bytes on the volume that contains `path`.
pub fn free_disk_bytes(path: &Path) -> Option<u64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| path.starts_with(d.mount_point()))
        // longest matching mount point wins ("/Users" beats "/")
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}
