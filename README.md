# avdhub

Create and run Android emulators without installing Android Studio.

- Downloads the Android emulator and system images directly from Google
- Creates and manages virtual devices
- No Java, no Android Studio, no command line
- macOS and Windows

## Status

v0.2.0: the GUI works on macOS and Windows, and updates itself once installed.

## Supported platforms

| Platform | Status |
| --- | --- |
| macOS (Apple Silicon) | Tested |
| macOS (Intel) | Builds and cross-compiles in CI; not run on hardware |
| Windows 10/11 (x86-64) | Tested on Windows 10 22H2, with and without WHPX |
| Windows on ARM | Not tested — reports welcome |
| Linux | Builds; not a release target yet |

### Windows: hardware acceleration

Android emulators need hardware virtualisation. On Windows that means the
**Windows Hypervisor Platform** feature, which is off by default. AEM detects
this and walks you through enabling it; you will need to restart once. WHPX
works alongside Hyper-V, WSL2 and Docker Desktop.

### Windows: a note on adb

If you have another copy of `adb` on your PATH — from an old SDK, or a tool that
bundled one — the Android emulator may use it instead of AEM's, and devices can
flicker between states. AEM puts its own `adb` first for emulators it launches.
For emulators started by other tools, remove the stale copy from your PATH.

### Installing

The builds are not yet code-signed, so:

- **macOS** will say the app is from an unidentified developer. Right-click the
  app and choose Open, then confirm.
- **Windows** SmartScreen will show a warning. Choose More info → Run anyway.

### Logs

The app writes `avdhub.log` to its log folder (on macOS
`~/Library/Logs/com.neycasilla.aem`, on Windows
`%LOCALAPPDATA%\com.neycasilla.aem\logs`). Attach it to bug reports, along with
the text from the **Copy diagnostics** button at the bottom of the Devices screen.

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
