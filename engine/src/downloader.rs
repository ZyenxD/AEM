use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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

/// Shared flag the UI can set to stop an install in progress.
#[derive(Debug, Clone, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    pub fn new() -> CancelFlag {
        CancelFlag(Arc::new(AtomicBool::new(false)))
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
    pub fn reset(&self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

/// Bytes needed to install these packages: the download plus the unpacked copy,
/// with headroom.
pub fn space_needed(packages: &[&RemotePackage], host: &HostInfo) -> u64 {
    let download: u64 = packages
        .iter()
        .filter_map(|p| p.archive_for(host))
        .map(|a| a.size_bytes)
        .sum();
    // Unpacked system images run roughly 1.5x their archive, and we hold both
    // at once. Add 500 MB of headroom.
    download + (download * 3 / 2) + 500 * 1_048_576
}

/// Download, verify and unpack one package into the SDK.
/// Returns `EngineError::Cancelled` (leaving no partial file) if `cancel` is set.
pub fn install_package(
    pkg: &RemotePackage,
    host: &HostInfo,
    sdk: &SdkLayout,
    cancel: &CancelFlag,
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
        cancel,
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
    cancel: &CancelFlag,
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
        if cancel.is_cancelled() {
            // Close the handle first: Windows cannot delete an open file.
            drop(file);
            fs::remove_file(dest).ok();
            return Err(EngineError::Cancelled);
        }
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

    /// Serve `body` once over plain HTTP on a random local port; returns the URL.
    fn serve_once(body: Vec<u8>) -> String {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request = [0u8; 1024];
                let _ = stream.read(&mut request);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        format!("http://127.0.0.1:{port}/file.zip")
    }

    #[test]
    fn download_completes_and_reports_progress() {
        let tmp = std::env::temp_dir().join("avdhub-dl-ok");
        fs::remove_dir_all(&tmp).ok();
        fs::create_dir_all(&tmp).unwrap();
        let dest = tmp.join("file.zip");

        let url = serve_once(vec![7u8; 3 * 1_048_576]);
        let mut last = 0;
        download_file(&url, &dest, 0, &CancelFlag::new(), &mut |p| {
            if let Progress::Downloading { done_bytes, .. } = p {
                last = done_bytes;
            }
        })
        .unwrap();

        assert_eq!(last, 3 * 1_048_576);
        assert_eq!(fs::metadata(&dest).unwrap().len(), 3 * 1_048_576);
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn cancelled_download_leaves_no_partial_file() {
        let tmp = std::env::temp_dir().join("avdhub-dl-cancel");
        fs::remove_dir_all(&tmp).ok();
        fs::create_dir_all(&tmp).unwrap();
        let dest = tmp.join("file.zip");

        let url = serve_once(vec![7u8; 8 * 1_048_576]);
        let cancel = CancelFlag::new();
        let trigger = cancel.clone();
        // Cancel as soon as the first megabyte has arrived.
        let result = download_file(&url, &dest, 0, &cancel, &mut |p| {
            if matches!(p, Progress::Downloading { .. }) {
                trigger.cancel();
            }
        });

        assert!(matches!(result, Err(EngineError::Cancelled)));
        assert!(!dest.exists());
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn cancel_flag_resets() {
        let flag = CancelFlag::new();
        let shared = flag.clone();
        shared.cancel();
        assert!(flag.is_cancelled());
        flag.reset();
        assert!(!shared.is_cancelled());
    }

    #[test]
    fn space_needed_adds_unpacked_copy_and_headroom() {
        use crate::host::{Abi, HostArch, HostOs};
        let host = HostInfo {
            os: HostOs::MacOs,
            arch: HostArch::Aarch64,
            preferred_abi: Abi::Arm64V8a,
            total_ram_bytes: 0,
        };
        let pkg = RemotePackage {
            path: "system-images;android-35;google_apis;arm64-v8a".into(),
            display_name: "img".into(),
            revision: (1, 0, 0),
            api_level: Some(35),
            abi: Some("arm64-v8a".into()),
            tag_id: None,
            tag_display: None,
            license_id: None,
            archives: vec![crate::repository::Archive {
                url: "x".into(),
                size_bytes: 1000 * 1_048_576,
                sha1: String::new(),
                host_os: None,
                host_arch: None,
            }],
        };
        // 1000 MB download + 1500 MB unpacked + 500 MB headroom
        assert_eq!(space_needed(&[&pkg], &host), 3000 * 1_048_576);
        assert_eq!(space_needed(&[], &host), 500 * 1_048_576);
    }
}
