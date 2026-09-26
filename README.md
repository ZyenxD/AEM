# avdhub

Create and run Android emulators without installing Android Studio.

- Downloads the Android emulator and system images directly from Google
- Creates and manages virtual devices
- No Java, no Android Studio, no command line
- macOS and Windows

## Status

In development. Phase 2 (engine) complete: catalog, downloads, device management and
launching all work from the command line. GUI in progress.

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
