use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::error::{EngineError, Result, io};
use crate::host::HostInfo;
use crate::repository::RemotePackage;
use crate::sdk_layout::SdkLayout;

/// Progress updates for the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "camelCase")]
pub enum Progress {
    Started { total_bytes: u64 },
    Downloading { done_bytes: u64, total_bytes: u64 },
    Verifying,
    Unpacking,
    Finished { install_dir: PathBuf },
}

/// Download, verify and unpack one package into the SDK.
pub fn install_package(
    pkg: &RemotePackage,
    host: &HostInfo,
    sdk: &SdkLayout,
    mut on_progress: impl FnMut(Progress),
) -> Result<PathBuf> {
    let archive = pkg.archive_for(host).ok_or_else(|| {
        EngineError::NotFound(format!("{} has no build for this machine", pkg.path))
    })?;

    let tmp_dir = sdk.root.join(".avdhub-tmp");
    fs::create_dir_all(&tmp_dir).map_err(|e| io(&tmp_dir, e))?;

    let file_name = archive
        .url
        .rsplit('/')
        .next()
        .unwrap_or("package.zip")
        .to_string();
    let zip_path = tmp_dir.join(&file_name);

    on_progress(Progress::Started {
        total_bytes: archive.size_bytes,
    });
    download_file(
        &archive.url,
        &zip_path,
        archive.size_bytes,
        &mut on_progress,
    )?;

    on_progress(Progress::Verifying);
    if !archive.sha1.is_empty() {
        let actual = sha1_of_file(&zip_path)?;
        if !actual.eq_ignore_ascii_case(&archive.sha1) {
            fs::remove_file(&zip_path).ok();
            return Err(EngineError::Checksum {
                name: pkg.path.clone(),
                expected: archive.sha1.clone(),
                actual,
            });
        }
    }

    on_progress(Progress::Unpacking);
    let install_dir = sdk.install_dir_for(&pkg.path);
    if install_dir.exists() {
        fs::remove_dir_all(&install_dir).map_err(|e| io(&install_dir, e))?;
    }
    extract_zip(&zip_path, &install_dir)?;
    fs::remove_file(&zip_path).ok();

    on_progress(Progress::Finished {
        install_dir: install_dir.clone(),
    });
    Ok(install_dir)
}

fn download_file(
    url: &str,
    dest: &Path,
    expected_size: u64,
    on_progress: &mut impl FnMut(Progress),
) -> Result<()> {
    let response = ureq::get(url).call().map_err(|e| EngineError::Network {
        url: url.to_string(),
        message: e.to_string(),
    })?;

    let total = response
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(expected_size);

    let mut reader = response.into_reader();
    let mut file = File::create(dest).map_err(|e| io(dest, e))?;

    let mut buffer = vec![0u8; 128 * 1024];
    let mut done: u64 = 0;
    let mut last_reported: u64 = 0;

    loop {
        let read = reader.read(&mut buffer).map_err(|e| EngineError::Network {
            url: url.to_string(),
            message: e.to_string(),
        })?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).map_err(|e| io(dest, e))?;
        done += read as u64;

        // Report at most once per megabyte so the UI is not flooded.
        if done - last_reported >= 1_048_576 {
            last_reported = done;
            on_progress(Progress::Downloading {
                done_bytes: done,
                total_bytes: total,
            });
        }
    }

    on_progress(Progress::Downloading {
        done_bytes: done,
        total_bytes: total,
    });
    Ok(())
}

pub fn sha1_of_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|e| io(path, e))?;
    let mut hasher = Sha1::new();
    let mut buffer = vec![0u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|e| io(path, e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Unpack a zip into `dest`.
pub fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
    let file = File::open(zip_path).map_err(|e| io(zip_path, e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| EngineError::Archive(e.to_string()))?;

    // Pass 1: is there exactly one top-level folder?
    let mut roots: HashSet<String> = HashSet::new();
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| EngineError::Archive(e.to_string()))?;
        if let Some(path) = entry.enclosed_name()
            && let Some(first) = path.components().next()
        {
            roots.insert(first.as_os_str().to_string_lossy().to_string());
        }
    }
    let strip_root = roots.len() == 1;

    // Pass 2: write files out.
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| EngineError::Archive(e.to_string()))?;

        let Some(raw) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue; // skip unsafe paths such as ../../etc
        };
        let relative: PathBuf = if strip_root {
            raw.components().skip(1).collect()
        } else {
            raw
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let out_path = dest.join(&relative);

        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| io(&out_path, e))?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
        }
        let mut out_file = File::create(&out_path).map_err(|e| io(&out_path, e))?;
        std::io::copy(&mut entry, &mut out_file).map_err(|e| io(&out_path, e))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = entry.unix_mode() {
                fs::set_permissions(&out_path, fs::Permissions::from_mode(mode))
                    .map_err(|e| io(&out_path, e))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use zip::write::FileOptions;

    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buf);
            let options = FileOptions::default().unix_permissions(0o755);
            for (name, data) in entries {
                writer.start_file(*name, options).unwrap();
                writer.write_all(data).unwrap();
            }
            writer.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn strips_single_common_root() {
        let tmp = std::env::temp_dir().join("avdhub-zip-strip");
        fs::remove_dir_all(&tmp).ok();
        fs::create_dir_all(&tmp).unwrap();

        let zip_bytes = build_zip(&[
            ("arm64-v8a/system.img", b"img"),
            ("arm64-v8a/data/hi.txt", b"hi"),
        ]);
        let zip_path = tmp.join("a.zip");
        fs::write(&zip_path, zip_bytes).unwrap();

        let dest = tmp.join("out");
        extract_zip(&zip_path, &dest).unwrap();

        assert!(dest.join("system.img").exists());
        assert!(dest.join("data/hi.txt").exists());
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn keeps_multiple_roots_and_restores_exec_bit() {
        let tmp = std::env::temp_dir().join("avdhub-zip-multi");
        fs::remove_dir_all(&tmp).ok();
        fs::create_dir_all(&tmp).unwrap();

        let zip_bytes = build_zip(&[("adb", b"binary"), ("NOTICE.txt", b"legal")]);
        let zip_path = tmp.join("b.zip");
        fs::write(&zip_path, zip_bytes).unwrap();

        let dest = tmp.join("out");
        extract_zip(&zip_path, &dest).unwrap();
        assert!(dest.join("adb").exists());
        assert!(dest.join("NOTICE.txt").exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dest.join("adb")).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "executable bit must survive unzip");
        }
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn sha1_matches_known_value() {
        let tmp = std::env::temp_dir().join("avdhub-sha1.txt");
        fs::write(&tmp, b"abc").unwrap();
        assert_eq!(
            sha1_of_file(&tmp).unwrap(),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        fs::remove_file(&tmp).ok();
    }
}
