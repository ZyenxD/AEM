use tauri::{AppHandle, Emitter};

use engine::downloader::Progress;
use engine::host::HostInfo;
use engine::repository::Catalog;
use engine::sdk_layout::SdkLayout;

use crate::dto::{InstallFinishedDto, InstallProgressDto};

pub const PROGRESS_EVENT: &str = "install-progress";
pub const FINISHED_EVENT: &str = "install-finished";

/// Runs on a background thread. Emits progress; never returns anything.
pub fn run_install(
    app: AppHandle,
    sdk: SdkLayout,
    host: HostInfo,
    catalog: Catalog,
    package_paths: Vec<String>,
) {
    let total = package_paths.len();
    let mut failure: Option<String> = None;

    for (position, path) in package_paths.iter().enumerate() {
        let Some(pkg) = catalog.find(path) else {
            failure = Some(format!("unknown package: {path}"));
            break;
        };
        let label = pkg.friendly_name();
        let index = position + 1;

        let app_for_closure = app.clone();
        let path_for_closure = path.clone();
        let label_for_closure = label.clone();

        let result = engine::downloader::install_package(pkg, &host, &sdk, move |progress| {
            let dto = to_dto(
                &path_for_closure,
                &label_for_closure,
                index,
                total,
                progress,
            );
            let _ = app_for_closure.emit(PROGRESS_EVENT, dto);
        });

        if let Err(e) = result {
            failure = Some(e.to_string());
            break;
        }
    }

    let _ = app.emit(
        FINISHED_EVENT,
        InstallFinishedDto {
            ok: failure.is_none(),
            error: failure,
        },
    );
}

fn to_dto(
    package_path: &str,
    label: &str,
    index: usize,
    total: usize,
    progress: Progress,
) -> InstallProgressDto {
    let (stage, done_bytes, total_bytes) = match progress {
        Progress::Started { total_bytes } => ("downloading", 0, total_bytes),
        Progress::Downloading {
            done_bytes,
            total_bytes,
        } => ("downloading", done_bytes, total_bytes),
        Progress::Verifying => ("verifying", 0, 0),
        Progress::Unpacking => ("unpacking", 0, 0),
        Progress::Finished { .. } => ("finished", 0, 0),
    };

    // `checked_div` is None when the size is unknown (0), so nothing divides by zero.
    let percent = match (done_bytes * 100).checked_div(total_bytes) {
        Some(p) => p.min(100) as u8,
        None if stage == "finished" => 100,
        None => 0,
    };

    InstallProgressDto {
        package_path: package_path.to_string(),
        label: label.to_string(),
        index,
        total,
        stage: stage.to_string(),
        done_bytes,
        total_bytes,
        percent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_is_clamped_and_safe_with_unknown_size() {
        let half = to_dto(
            "p",
            "l",
            1,
            1,
            Progress::Downloading {
                done_bytes: 50,
                total_bytes: 100,
            },
        );
        assert_eq!(half.percent, 50);
        assert_eq!(half.stage, "downloading");

        let unknown = to_dto(
            "p",
            "l",
            1,
            1,
            Progress::Downloading {
                done_bytes: 50,
                total_bytes: 0,
            },
        );
        assert_eq!(unknown.percent, 0);

        let done = to_dto(
            "p",
            "l",
            1,
            1,
            Progress::Finished {
                install_dir: "/tmp".into(),
            },
        );
        assert_eq!(done.percent, 100);
    }
}
