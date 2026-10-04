# avdhub

Create and run Android emulators without installing Android Studio.

- Downloads the Android emulator and system images directly from Google
- Creates and manages virtual devices
- No Java, no Android Studio, no command line
- macOS and Windows

## Status

In development. The GUI works on macOS; Windows support is being tested (Phase 4).

## Supported platforms

| Platform | Status |
|---|---|
| macOS (Apple Silicon) | Tested |
| macOS (Intel) | Builds; not yet tested |
| Windows 10/11 (x86-64) | Builds; testing in progress |
| Windows on ARM | Not tested — reports welcome |
| Linux | Builds; not a release target yet |

### Windows: hardware acceleration

Android emulators need hardware virtualisation. On Windows this means the
**Windows Hypervisor Platform** feature, which is off by default. AEM detects
this and walks you through enabling it — you will need to restart once.

WHPX works alongside Hyper-V, WSL2 and Docker Desktop.

### Installing

The builds are not yet code-signed, so:

- **macOS** will say the app is from an unidentified developer. Right-click the
  app and choose Open, then confirm.
- **Windows** SmartScreen will show a warning. Choose More info → Run anyway.

### Logs

The app writes `avdhub.log` to its log folder (on macOS
`~/Library/Logs/com.neycasilla.aem`, on Windows
`%LOCALAPPDATA%\com.neycasilla.aem\logs`). Attach it to bug reports.

## How it works

avdhub talks to the same Google servers and writes the same files as Google's own
`sdkmanager` and `avdmanager` tools, so devices you create here also work in Android
Studio, and licences you accept in either are recognised by both. Google's SDK components
are downloaded from Google's servers after you accept their licence — nothing is
redistributed by this project.

## Building

Requires Rust (rustup) and Node LTS.

    npm install
    npm run tauri dev

### Engine only

The core logic lives in `engine/`, a plain Rust library with no GUI dependencies.

    cd engine
    cargo test
    cargo run --example show_catalog
    cargo run --example show_license
    cargo run --example first_boot   # downloads ~2-3 GB and boots a real emulator

## Licence

Apache-2.0
