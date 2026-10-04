use std::sync::Mutex;

use engine::downloader::CancelFlag;
use engine::host::HostInfo;
use engine::repository::Catalog;
use engine::sdk_layout::SdkLayout;

pub struct AppState {
    pub host: HostInfo,
    pub sdk: Mutex<SdkLayout>,
    pub catalog: Mutex<Option<Catalog>>,
    /// Shared with the running install; `cancel_install` sets it.
    pub cancel: CancelFlag,
}

impl AppState {
    pub fn new() -> Result<AppState, String> {
        let host = engine::host::detect();

        // Reuse an existing SDK if we find one, otherwise create ours.
        let sdk = SdkLayout::discover()
            .into_iter()
            .next()
            .map(Ok)
            .unwrap_or_else(SdkLayout::create_managed)
            .map_err(|e| e.to_string())?;

        log::info!(
            "host: {:?} {:?}, abi {:?}, sdk root {} (managed by us: {}), emulator {:?}",
            host.os,
            host.arch,
            host.preferred_abi,
            sdk.root.display(),
            sdk.managed_by_us,
            sdk.emulator_bin(),
        );

        // Pay the adb server's startup cost now, off the startup path.
        let warm_sdk = sdk.clone();
        std::thread::spawn(move || engine::process::warm_up(&warm_sdk));

        Ok(AppState {
            host,
            sdk: Mutex::new(sdk),
            catalog: Mutex::new(None),
            cancel: CancelFlag::new(),
        })
    }

    /// Snapshot of the SDK, so callers never hold the lock while doing slow work.
    pub fn sdk(&self) -> Result<SdkLayout, String> {
        self.sdk
            .lock()
            .map(|g| g.clone())
            .map_err(|_| "internal lock error".to_string())
    }

    /// Snapshot of the cached catalog; errors if it has not been fetched yet.
    pub fn cached_catalog(&self) -> Result<Catalog, String> {
        let guard = self
            .catalog
            .lock()
            .map_err(|_| "internal lock error".to_string())?;
        guard
            .clone()
            .ok_or_else(|| "the package list has not been loaded".to_string())
    }
}
