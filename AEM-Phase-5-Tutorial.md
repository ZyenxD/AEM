# AEM — Phase 5 Tutorial

*Windows test fixes · release hygiene · auto-updater · code signing · diagnostics · launch*

*(Written against the current codebase: branch `windows-support`, commit `1805698`, read from github.com/ZyenxD/aem. Every file path and symbol below comes from that branch, not from the Phase 3/4 guides.)*

---

## What you'll build

- AEM's own adb wins on Windows, so the device dot actually turns green on machines with an old `adb` on the PATH
- Cancel stops an install during unpacking, not only during download
- The acceleration tests run against the real emulator output you captured, not invented strings
- A machine with virtualisation off in the BIOS gets BIOS steps, not WHPX steps
- The app checks for updates and installs them, signed, without touching an install in progress
- `.msi`, `.exe` and `.dmg` carry a real signature, so Windows and macOS stop warning users off
- A "Copy diagnostics" button that produces the one block of text you need to debug a stranger's machine
- A tagged, published `v0.2.0` and the launch materials to announce it

## Before you start

You should have, from Phase 4:

- The `windows-support` branch building clean on MSVC: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p engine` (26 passing), `npm run build`
- The two real captures from the Windows PC, **currently only on that PC**:
  - `-accel-check` with WHPX off (exit code 6, the AEHD message)
  - `-accel-check` with WHPX on (exit code 0, the WHPX message)
- The Windows test PC still carrying `C:\adb\adb.exe` on its PATH. **Do not remove it** — it is your reproduction case for Part A.

Two things to know before you begin.

**`windows-support` is not merged.** `main` is still at Phase 2 plus CI. Part B merges it; everything before that happens on the branch.

**One step in Part C cannot be undone.** Step C1 generates the updater signing key. Lose it and you can never ship an update to anyone who has already installed AEM — they would have to download a new installer by hand. It is flagged again where it happens.

---

## Part A — Close out the Windows findings

Six fixes, all against findings in the v1 and v2 test reports. Do A1 and A2 first; they are the two the reports rank High.

### Step A1 — Put AEM's adb first on the emulator's PATH

**Root cause:** the emulator does not use the adb you hand it. It shells out by name — `cmd.exe /C "adb -s emulator-5554 shell getprop sys.boot_completed"` — and Windows resolves that from `PATH`. `ANDROID_SDK_ROOT` has no effect on it. On the test PC a 2014 adb (client 31) answered, AEM's adb (client 41) kept killing its server, and the device never reported `booted` in 400 seconds. Without the stale folder it went green in 207.

`engine/src/process.rs` — in `launch()`, the env block currently reads:

```rust
    // The emulator resolves the device's image relative to the SDK root.
    command
        .env("ANDROID_SDK_ROOT", &image_sdk.root)
        .env("ANDROID_HOME", &image_sdk.root)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
```

Add the PATH block immediately **above** it:

```rust
    // The emulator shells out to `adb` by name and resolves it from PATH, not
    // from ANDROID_SDK_ROOT. An old adb there answers instead of ours, and the
    // two servers kill each other until the device never reports booted.
    // Ours goes first so the client the emulator uses matches the one `running()` uses.
    if let Some(adb) = sdk.adb_bin()                       // ← add this block
        && let Some(tools_dir) = adb.parent()
    {
        let mut entries = vec![tools_dir.to_path_buf()];
        if let Some(existing) = std::env::var_os("PATH") {
            entries.extend(std::env::split_paths(&existing));
        }
        if let Ok(joined) = std::env::join_paths(entries) {
            command.env("PATH", joined);
        }
    }

    // The emulator resolves the device's image relative to the SDK root.
    command
        .env("ANDROID_SDK_ROOT", &image_sdk.root)
        // ...existing code...
```

> **Note:** `sdk`, not `image_sdk`. `image_sdk` is whichever SDK holds the device's system image, which may be Android Studio's and may have no `platform-tools` at all. `sdk` is the one `running()` queries, and matching those two clients is the whole point.

### Step A2 — Report a foreign adb so you can see it in the field

**Why:** the PATH fix only covers emulators AEM launched. One started by Android Studio or a shell still gets the stale adb, and you will get "it just sits there" reports with no way to confirm the cause. This function is what Part F's diagnostics will print.

`engine/src/process.rs` — add after `warm_up()`:

```rust
/// An `adb` earlier on the PATH than ours, if there is one.
///
/// The emulator resolves `adb` from PATH, so a stale copy there breaks device
/// status for emulators AEM did not launch. We cannot fix those; we can say so.
pub fn foreign_adb_on_path(sdk: &SdkLayout) -> Option<PathBuf> {
    let ours = sdk.adb_bin();
    let file_name = if cfg!(windows) { "adb.exe" } else { "adb" };

    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(file_name);
        if !candidate.is_file() {
            continue;
        }
        match &ours {
            Some(ours) if same_file(ours, &candidate) => return None, // ours wins
            _ => return Some(candidate),
        }
    }
    None
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
```

`PathBuf` is not imported in this file yet — change the import at the top:

```rust
use std::path::{Path, PathBuf};   // was: use std::path::Path;
```

And a test, in the existing `mod tests` at the bottom of the file:

```rust
    #[test]
    fn no_adb_anywhere_means_no_foreign_adb() {
        // An SDK with no platform-tools and a PATH we do not control:
        // the function must not panic and must not claim ours is foreign.
        let sdk = SdkLayout::new(std::env::temp_dir().join("avdhub-no-such-sdk"), true);
        let _ = foreign_adb_on_path(&sdk); // either None or a real system adb
    }
```

**Test it:** on the Windows PC, **with `C:\adb` still on the PATH**, start a device from the installed app and watch the dot. It should reach green in roughly three minutes instead of flickering forever. Then run `cargo test -p engine` on the Mac — 27 passing.

---

### Step A3 — Make Cancel work while unpacking

**Root cause:** `CancelFlag` is only read inside the download loop in `download_file`. `extract_zip` never sees it, so Cancel pressed at "Unpacking…" does nothing. On the test PC the emulator package ran 104 seconds past the click and then reported success.

`engine/src/downloader.rs` — change the signature and add the check. The function currently starts:

```rust
/// Unpack a zip into `dest`.
pub fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
```

Replace that line and add the check inside the write loop:

```rust
/// Unpack a zip into `dest`.
///
/// Checks `cancel` between entries: a large image takes minutes to unpack, and
/// a Cancel button that does nothing for that long reads as a hang.
pub fn extract_zip(zip_path: &Path, dest: &Path, cancel: &CancelFlag) -> Result<()> {
```

Then, inside **pass 2**, as the first statement of the `for i in 0..archive.len()` loop:

```rust
    // Pass 2: write files out.
    for i in 0..archive.len() {
        if cancel.is_cancelled() {                      // ← add this block
            // A half-written package is worse than none: the folder would look
            // installed to `installed_images()`.
            fs::remove_dir_all(dest).ok();
            return Err(EngineError::Cancelled);
        }

        let mut entry = archive
            .by_index(i)
        // ...existing code...
```

Update the one caller, in `install_package`:

```rust
    extract_zip(&zip_path, &install_dir, cancel)?;      // ← was: extract_zip(&zip_path, &install_dir)?;
```

And the existing extraction tests in the same file, which call it with two arguments:

```rust
        extract_zip(&zip_path, &dest, &CancelFlag::new()).unwrap();   // ← add the flag
```

> **Note:** removing `dest` on cancel is safe because `install_package` already deleted any previous install before extracting. There is nothing of the user's in that folder.

Add a test next to the other cancel tests:

```rust
    #[test]
    fn cancel_during_unpacking_stops_and_leaves_nothing() {
        let tmp = std::env::temp_dir().join("avdhub-zip-cancel");
        fs::remove_dir_all(&tmp).ok();
        fs::create_dir_all(&tmp).unwrap();

        let zip_bytes = build_zip(&[("a/one.txt", b"1"), ("a/two.txt", b"2")]);
        let zip_path = tmp.join("c.zip");
        fs::write(&zip_path, zip_bytes).unwrap();

        let cancel = CancelFlag::new();
        cancel.cancel(); // already cancelled before the first entry

        let dest = tmp.join("out");
        let result = extract_zip(&zip_path, &dest, &cancel);

        assert!(matches!(result, Err(EngineError::Cancelled)));
        assert!(!dest.exists(), "a cancelled unpack must leave no folder");
        fs::remove_dir_all(&tmp).ok();
    }
```

### Step A4 — Keep the Cancel button honest

Even with A3, cancellation during verify is instant but during unpack takes until the current file finishes. That is fine. What is not fine is a button with no feedback.

`src/app/shared/install-progress.component.ts` — replace the actions block:

```html
    <div class="actions">
      <button type="button" (click)="onCancel()" [disabled]="cancelling()">
        {{ cancelling() ? 'Stopping…' : 'Cancel' }}
      </button>
    </div>
```

and the class:

```ts
export class InstallProgressComponent {
  readonly progress = input<InstallProgress | null>(null);
  readonly cancel = output<void>();

  /** Set on click so the button cannot be pressed twice while the stage finishes. */
  readonly cancelling = signal(false);          // ← add this

  onCancel(): void {                            // ← add this
    this.cancelling.set(true);
    this.cancel.emit();
  }
}
```

Add `signal` to the Angular import on line 2.

**Test it:** start a fresh install, press Cancel while the text reads "Unpacking…". The button changes to "Stopping…" and the flow returns to the form within a few seconds. Check `<sdk>\.avdhub-tmp` is empty and the package folder is gone. Then run `cargo test -p engine` — 28 passing.

---

### Step A5 — Test the classifier against the real captures

**Root cause:** `engine/src/accel.rs` lines 241–243 still assert against strings invented before anyone had a Windows machine. The real WHPX-off message names AEHD and exits 6, not "WHPX is not installed" with exit 1. The classifier happens to be right, but only through its fall-through branch, and nothing proves that stays true.

Copy the two captures off the Windows PC into the repo. Exact contents, normalised to LF:

`engine/tests/fixtures/accel_windows_ok.txt`

```
accel:
0
WHPX(10.0.19045) is installed and usable.
accel
```

`engine/tests/fixtures/accel_windows_no_whpx.txt`

```
accel:
6
Android Emulator hypervisor driver is not installed on this machine
accel
```

`engine/src/accel.rs` — replace the three invented constants in `mod tests`:

```rust
    // Real output from emulator 37.4.1.0 (build 16916650) on Windows 10 19045.
    // Captured 2026-11-19 (WHPX off) and 2026-11-21 (WHPX on).
    const WINDOWS_OK: &str = include_str!("../tests/fixtures/accel_windows_ok.txt");
    const WINDOWS_NO_WHPX: &str = include_str!("../tests/fixtures/accel_windows_no_whpx.txt");
    // Still invented: nobody has turned VT-x off in a BIOS yet. See Step A6.
    const WINDOWS_NO_VTX: &str =
        "accel:\n1\nVT-x is not supported by the host CPU or is disabled in the BIOS\naccel\n";
```

Then add a test that pins down what the real message does *not* say, so a future "simplification" of `classify_windows` cannot quietly break it:

```rust
    #[test]
    fn real_whpx_off_message_names_aehd_and_still_gives_whpx_steps() {
        // The emulator blames its own driver, not WHPX. The verdict is correct
        // only because nothing matches the firmware keywords and we fall through.
        assert!(WINDOWS_NO_WHPX.to_lowercase().contains("hypervisor driver"));
        assert!(!WINDOWS_NO_WHPX.to_lowercase().contains("whpx"));

        let report = classify(WINDOWS_NO_WHPX, HostOs::Windows);
        assert_eq!(report.status, AccelStatus::HypervisorDisabled);
        assert!(report.requires_reboot);
        assert!(
            report
                .fix_steps
                .iter()
                .any(|s| s.command.as_deref().is_some_and(|c| c.contains("HypervisorPlatform"))),
            "the user must be pointed at WHPX, never at AEHD (it sunsets 2026-12-31)"
        );
    }
```

> **Note:** the emulator writes CRLF. `str::lines()` handles either, but keep the committed fixtures LF so a future diff is not noise.

### Step A6 — Decide the BIOS case from the CPU, not the message

**Root cause:** `classify_windows` decides "firmware off" by looking for `vt-x`, `svm` or `bios` in the emulator's text. The real WHPX-off message contains none of those — and nobody has yet captured what the emulator prints when VT-x really is off. If it prints the same AEHD sentence, a user with virtualisation disabled in their BIOS is told to enable WHPX, which will not help them. That is wrong advice at the one moment the app most needs to be right.

Rather than wait for a BIOS capture, ask Windows directly.

`engine/src/accel.rs` — add below `extract_message`:

```rust
/// Does the CPU report virtualisation enabled in firmware?
///
/// `None` when we cannot tell (not Windows, or the query failed), which the
/// classifier treats as "no opinion" rather than as a verdict.
#[cfg(windows)]
pub fn firmware_virtualization_enabled() -> Option<bool> {
    let mut command = std::process::Command::new("powershell");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "(Get-CimInstance Win32_Processor | Select-Object -First 1).VirtualizationFirmwareEnabled",
    ]);
    crate::process::hide_console(&mut command);

    let output = command.output().ok()?;
    match String::from_utf8_lossy(&output.stdout).trim().to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[cfg(not(windows))]
pub fn firmware_virtualization_enabled() -> Option<bool> {
    None
}
```

Now let the classifier use it, keeping the pure function pure. This mirrors how `repository.rs` grew `parse_manifest_with_base` beside `parse_manifest`:

```rust
/// Turn the emulator's output into a verdict. Pure function: unit-testable.
pub fn classify(output: &str, os: HostOs) -> AccelReport {
    classify_with_firmware(output, os, None)
}

/// As `classify`, plus what the CPU says about firmware virtualisation.
/// `firmware = Some(false)` outranks the message: the BIOS is the real blocker,
/// whatever the emulator chose to complain about.
pub fn classify_with_firmware(
    output: &str,
    os: HostOs,
    firmware: Option<bool>,
) -> AccelReport {
    let lowered = output.to_lowercase();
    let message = extract_message(output);

    // ...existing accel_ok block, unchanged...

    if accel_ok {
        // ...existing Ready report, unchanged...
    }

    match os {
        HostOs::Windows => {
            if firmware == Some(false) {
                return windows_firmware_report(message);
            }
            classify_windows(&lowered, message)
        }
        // ...existing MacOs and Linux arms, unchanged...
    }
}
```

Pull the firmware branch out of `classify_windows` so both callers share it. In `classify_windows`, replace the body of the `if firmware_off { ... }` block with a call:

```rust
    if firmware_off {
        return windows_firmware_report(message);      // ← was the inline AccelReport
    }
```

and add the extracted function below it, holding the two `FixStep`s that were inline:

```rust
fn windows_firmware_report(message: String) -> AccelReport {
    AccelReport {
        status: AccelStatus::VirtualizationDisabled,
        summary: "Your computer's virtualisation feature is switched off.".to_string(),
        raw_message: message,
        fix_steps: vec![
            // ...the two existing steps, moved here unchanged...
        ],
        requires_reboot: true,
    }
}
```

Finally, call the new path from `check()`:

```rust
    Ok(classify_with_firmware(
        &text,
        HostOs::current(),
        firmware_virtualization_enabled(),
    ))
```

A test:

```rust
    #[test]
    fn firmware_off_outranks_the_emulator_message() {
        // Even the AEHD message means "fix your BIOS" when the CPU says so.
        let report = classify_with_firmware(WINDOWS_NO_WHPX, HostOs::Windows, Some(false));
        assert_eq!(report.status, AccelStatus::VirtualizationDisabled);
        assert!(report.fix_steps[0].detail.contains("Task Manager"));

        // With firmware on, or unknown, the message decides as before.
        assert_eq!(
            classify_with_firmware(WINDOWS_NO_WHPX, HostOs::Windows, Some(true)).status,
            AccelStatus::HypervisorDisabled
        );
        assert_eq!(
            classify_with_firmware(WINDOWS_NO_WHPX, HostOs::Windows, None).status,
            AccelStatus::HypervisorDisabled
        );
    }
```

> **Note:** the PowerShell call costs a few hundred milliseconds, and only runs when acceleration has already failed. The Ready path is untouched.

**Known limits:** `VirtualizationFirmwareEnabled` is not reported by every CPU driver; on some machines it comes back blank, which is why `None` means "no opinion" rather than "enabled". And this only disambiguates the BIOS case — if the emulator one day prints something genuinely new, the fall-through still sends the user to WHPX.

**Test it:** `cargo test -p engine` → 30 passing. On the Windows PC with WHPX off, the panel still shows the WHPX steps. If you can borrow a machine with VT-x off in the BIOS, it should now show the Task Manager and BIOS steps instead.

---

### Step A7 — Show "Stopping…" and fix the log clock

Two small ones from the v2 report. Stop takes 23–26 seconds (the emulator's own graceful shutdown plus a snapshot save), and the UI says nothing, so it reads as a hang. And the installed build logs in UTC, which confuses anyone reading their own log — including you, reading theirs.

`src/app/devices/devices.component.ts` — add a signal and use it:

```ts
  readonly confirmingDelete = signal<string | null>(null);
  /** Devices we have asked to stop. Shutdown takes ~25 s with a snapshot save. */
  readonly stopping = signal<Set<string>>(new Set());    // ← add this
```

In `stop()`:

```ts
  async stop(device: Device): Promise<void> {
    this.busy.set(device.name);
    this.actionError.set(null);
    this.stopping.update((s) => new Set(s).add(device.name));    // ← add this
    try {
      await this.tauri.stopDevice(device.name);
      setTimeout(() => void this.store.refresh(), 1500);
    } catch (e) {
      this.actionError.set(friendly(e));
      this.stopping.update((s) => {                              // ← add this block
        const next = new Set(s);
        next.delete(device.name);
        return next;
      });
    } finally {
      this.busy.set(null);
    }
  }
```

And in `statusLabel`:

```ts
  statusLabel(device: Device): string {
    if (this.stopping().has(device.name) && device.running) return 'Stopping…';  // ← add this
    if (device.booted) return 'Running';
    if (device.running) return 'Starting…';
    return 'Stopped';
  }
```

The entry clears itself: once the poller stops seeing the device, `device.running` is false and the label falls through to "Stopped".

`src-tauri/src/lib.rs` — one line on the log plugin builder:

```rust
                .level(log::LevelFilter::Info)
                .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)  // ← add this
                .build(),
```

**Test it:** stop a running device — the label reads "Stopping…" for about 25 seconds, then "Stopped". Open the log and confirm new lines match your wall clock.

---

## Part B — Release hygiene

### Step B1 — Make CI build the app, not just the engine

**Root cause:** `ci.yml` lints and tests `-p engine` only. That is why `greet` calling a deleted `engine::hello()` reached a Windows machine before anyone noticed — the desktop app was never compiled by CI at all.

`.github/workflows/ci.yml` — in the `engine` job, add the Linux dependency step before Format, and widen the two cargo steps:

```yaml
      - uses: Swatinem/rust-cache@v2

      - name: Tauri system dependencies            # ← add this step
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev \
            libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libssl-dev \
            libayatana-appindicator3-dev librsvg2-dev

      - name: Format
        run: cargo fmt --all -- --check

      - name: Lint
        run: cargo clippy --workspace --all-targets -- -D warnings   # ← was: -p engine

      - name: Test
        run: cargo test --workspace                                  # ← was: -p engine
```

> **Note:** the job is still called `engine`. Rename it to `rust` if you like, but if you have branch protection configured against the old check names, update those too or merges will block on checks that no longer exist.

### Step B2 — Keep the three version numbers in step

**Root cause:** the version lives in `package.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml`, all currently `0.1.0`. The updater you are about to add decides whether to offer an update by comparing the tag's version with the installed one. A tag that disagrees with the config ships an update that nobody is offered, or one that installs and claims the same version.

`scripts/check-version.mjs`

```js
// Fails if package.json, tauri.conf.json and src-tauri/Cargo.toml disagree,
// or (in CI, on a tag) if they disagree with the tag.
import { readFileSync } from 'node:fs';

const pkg = JSON.parse(readFileSync('package.json', 'utf8')).version;
const conf = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8')).version;
const cargo = readFileSync('src-tauri/Cargo.toml', 'utf8')
  .match(/^version\s*=\s*"([^"]+)"/m)?.[1];

const found = { 'package.json': pkg, 'tauri.conf.json': conf, 'src-tauri/Cargo.toml': cargo };
const unique = [...new Set(Object.values(found))];

if (unique.length !== 1) {
  console.error('Version mismatch:', found);
  process.exit(1);
}

const ref = process.env.GITHUB_REF_NAME;
if (ref?.startsWith('v') && ref.slice(1) !== unique[0]) {
  console.error(`Tag ${ref} does not match version ${unique[0]}`);
  process.exit(1);
}

console.log(`Version ${unique[0]} is consistent.`);
```

Add it to `package.json` scripts:

```json
    "tauri": "tauri",
    "check:version": "node scripts/check-version.mjs"
```

And to the `frontend` job in `ci.yml`, plus the release workflow in Step C4:

```yaml
      - run: npm ci

      - run: npm run check:version      # ← add this

      - run: npm run build
```

### Step B3 — Fix the release name

`.github/workflows/release.yml` still says `avdhub`, the old working title. The product is `aem`.

```yaml
          releaseName: 'AEM ${{ github.ref_name }}'   # ← was: 'avdhub ${{ github.ref_name }}'
```

**Test it:** push the branch and confirm CI is green on all three runners with the wider lint, and that the frontend job runs the version check. Then merge:

```bash
git checkout main
git merge windows-support
git push
```

---

## Part C — The auto-updater

Do this **before** announcing. Anyone who installs a build without the updater in it can never be auto-updated; they would have to find and download a new installer by hand. Every install from here on is one you can reach.

### Step C1 — Generate the signing keypair

> **This step is irreversible.** The private key signs every update. If you lose it, users who already installed AEM will never receive another update from you — there is no recovery, no reissue, and no override. Back it up somewhere you will still have in two years: a password manager entry, not just the Mac's home folder.

```bash
npm run tauri signer generate -- -w ~/.tauri/aem.key
```

It prints a public key and writes `~/.tauri/aem.key` (private) and `~/.tauri/aem.key.pub`. Choose a password when prompted; you will need it in CI.

Then, on GitHub → Settings → Secrets and variables → Actions:

- `TAURI_SIGNING_PRIVATE_KEY` — the **contents** of `~/.tauri/aem.key`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — the password you chose

> **Note:** this key is not a code-signing certificate and does not stop SmartScreen or Gatekeeper. It proves an update came from you. The two are independent; Part D and E handle the other one.

### Step C2 — Install the plugin

```bash
npm run tauri add updater
```

That adds `tauri-plugin-updater` to `src-tauri/Cargo.toml`, `@tauri-apps/plugin-updater` to `package.json`, the plugin registration in `lib.rs` and the permission in `capabilities/default.json`. Relaunching after an update also needs the process plugin:

```bash
npm run tauri add process
```

Confirm `src-tauri/src/lib.rs` now registers both, next to the existing ones:

```rust
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())   // ← added by the CLI
        .plugin(tauri_plugin_process::init())                   // ← added by the CLI
```

and that `src-tauri/capabilities/default.json` reads:

```json
  "permissions": [
    "core:default",
    "opener:default",
    "updater:default",
    "process:allow-restart"
  ]
```

### Step C3 — Configure the endpoint and artifacts

`src-tauri/tauri.conf.json` — add `createUpdaterArtifacts` inside the existing `bundle` block, and a new top-level `plugins` block:

```json
  "bundle": {
    "active": true,
    "targets": "all",
    "createUpdaterArtifacts": true,
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  },
  "plugins": {
    "updater": {
      "pubkey": "PASTE THE PUBLIC KEY FROM STEP C1",
      "endpoints": [
        "https://github.com/ZyenxD/aem/releases/latest/download/latest.json"
      ],
      "windows": {
        "installMode": "passive"
      }
    }
  }
```

`"passive"` shows a small progress window and needs no clicks. The default (`"basic"`) runs the full installer UI, which for an update the user already agreed to is a worse experience.

> **Note:** the pubkey is the key's contents, not a file path. It is safe to commit.

### Step C4 — Publish `latest.json` from the release workflow

`.github/workflows/release.yml` — add the version check, the signing secrets and the updater manifest:

```yaml
      - run: npm ci

      - run: npm run check:version                      # ← add this

      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}                    # ← add
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}  # ← add
        with:
          tagName: ${{ github.ref_name }}
          releaseName: 'AEM ${{ github.ref_name }}'
          releaseDraft: true
          includeUpdaterJson: true                      # ← add
          args: ${{ matrix.args }}
```

> **Note:** `releaseDraft: true` and the updater interact. The endpoint is `releases/latest/download/latest.json`, and a draft release is not "latest" — so nothing is offered until you publish the draft. That is the behaviour you want, but it also means forgetting to publish looks exactly like a broken updater.

### Step C5 — Never update over a running install

**Root cause:** the updater closes the app and replaces the binary. If a 2 GB install is in flight, that leaves a partial package and files in `.avdhub-tmp`, and the user's next launch finds an SDK that looks installed but is not.

`src-tauri/src/state.rs` — add a flag to `AppState`:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub host: HostInfo,
    pub sdk: Mutex<SdkLayout>,
    pub catalog: Mutex<Option<Catalog>>,
    /// Shared with the running install; `cancel_install` sets it.
    pub cancel: CancelFlag,
    /// True while an install job is running, so the updater holds off.
    pub installing: Arc<AtomicBool>,        // ← add this
}
```

In `AppState::new()`, in the returned struct:

```rust
        Ok(AppState {
            host,
            sdk: Mutex::new(sdk),
            catalog: Mutex::new(None),
            cancel: CancelFlag::new(),
            installing: Arc::new(AtomicBool::new(false)),   // ← add this
        })
```

And a helper on the impl:

```rust
    pub fn is_installing(&self) -> bool {
        self.installing.load(Ordering::Relaxed)
    }
```

`src-tauri/src/install.rs` — clear the flag however the job ends. At the very end of `run_install`, before the finished event:

```rust
    // Whatever happened, the job is over: let the updater proceed again.
    if let Some(state) = app.try_state::<crate::state::AppState>() {     // ← add this block
        state.installing.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    let _ = app.emit(
        FINISHED_EVENT,
        // ...existing code...
```

and add the import at the top:

```rust
use tauri::{AppHandle, Emitter, Manager};   // ← was: use tauri::{AppHandle, Emitter};
```

`src-tauri/src/commands.rs` — set the flag where `start_install` spawns the job, and add a command the UI can ask:

```rust
    state.installing.store(true, std::sync::atomic::Ordering::Relaxed);   // ← before the spawn
    tauri::async_runtime::spawn_blocking(move || {
        // ...existing run_install call...
    });
```

```rust
/// True while a download or unpack is in progress. The updater asks before prompting.
#[tauri::command]
pub fn is_installing(state: State<'_, AppState>) -> bool {
    state.is_installing()
}
```

Register it in `src-tauri/src/lib.rs` alongside the others:

```rust
            commands::open_windows_features,
            commands::is_installing,            // ← add this
```

> **Note:** set the flag in the command, not inside `run_install`. The command returns immediately; if the flag were set by the spawned job, a check in the first few milliseconds would miss it.

### Step C6 — Check for updates from the app

`src/app/core/tauri.service.ts` — add to the install section:

```ts
  isInstalling(): Promise<boolean> {
    return invoke<boolean>('is_installing');
  }
```

`src/app/core/updates.service.ts`

```ts
import { Injectable, inject, signal } from '@angular/core';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

import { TauriService } from './tauri.service';

@Injectable({ providedIn: 'root' })
export class UpdatesService {
  private readonly tauri = inject(TauriService);

  readonly available = signal<Update | null>(null);
  readonly installing = signal(false);
  readonly error = signal<string | null>(null);

  /** Look for an update. Silent when there is none, or when AEM is busy. */
  async checkQuietly(): Promise<void> {
    try {
      // Never interrupt a download: the updater would replace the app mid-install.
      if (await this.tauri.isInstalling()) return;

      const update = await check();
      if (update) this.available.set(update);
    } catch (e) {
      // A failed check is not worth a dialog; the user did not ask for it.
      console.warn('update check failed', e);
    }
  }

  async installNow(): Promise<void> {
    const update = this.available();
    if (!update) return;

    if (await this.tauri.isInstalling()) {
      this.error.set('Finish or cancel the current download first.');
      return;
    }

    this.installing.set(true);
    try {
      await update.downloadAndInstall();
      await relaunch();
    } catch (e) {
      this.error.set(String(e));
      this.installing.set(false);
    }
  }

  dismiss(): void {
    this.available.set(null);
  }
}
```

`src/app/app.component.ts` — check once at startup, after the existing setup check. Add to the constructor:

```ts
    void this.checkSetup();
    void inject(UpdatesService).checkQuietly();   // ← add this (see note)
```

> **Note:** `inject()` only works in an injection context, so put `private readonly updates = inject(UpdatesService);` with the other fields and call `void this.updates.checkQuietly();` in the constructor. Written inline it will throw at runtime.

A banner, rendered in `app.component.html` above the router outlet:

```html
@if (updates.available(); as update) {
  <div class="update-banner">
    <span>AEM {{ update.version }} is available.</span>
    <button class="primary" (click)="updates.installNow()" [disabled]="updates.installing()">
      {{ updates.installing() ? 'Installing…' : 'Update and restart' }}
    </button>
    <button (click)="updates.dismiss()">Later</button>
  </div>
}
<router-outlet />
```

**Known limits:** the check runs once per app start, not on a timer. There is no release-channel support, no rollback, and the update download itself cannot be cancelled. A user who dismisses the banner sees it again on the next launch, which is the intended nag level.

**Test it:** you cannot fully test an updater against a release that does not exist yet. What you can verify now: `npm run tauri build` produces `.msi.zip` and `.msi.zip.sig` (or `.nsis.zip` + `.sig`) in `target/release/bundle/`, and the app starts with no console errors from the plugin. The real test is in Part F, after two releases exist.

---

## Part D — Windows code signing

### Step D1 — Choose a path

This is the one place in Phase 5 that costs money, and the decision is yours.

| Option | Cost | What you get | What it does not do |
| --- | --- | --- | --- |
| **Ship unsigned** | free | Works. Users click through SmartScreen. | Every download shows "Windows protected your PC" with an unknown publisher |
| **Azure Trusted Signing** (recently renamed Artifact Signing) | ~$10/month | Publicly trusted signature, no hardware token, no annual renewal. Reputation tracked at the account level. | Requires an organization identity validation, which takes days. Microsoft does **not** guarantee instant SmartScreen reputation |
| **OV certificate** | ~$200–400/year | A signature, on a hardware token or in Azure Key Vault | Reputation accrues slowly with download volume; the token is awkward in CI |

**Recommendation:** Trusted Signing, but not before you have downloads. Ship `v0.2.0` unsigned with the README warning, see whether anyone installs it, and sign once the numbers justify a subscription. Switching later is a config change, not a rework.

Be careful with the claim you will see repeated online that Trusted Signing gives instant SmartScreen reputation. Microsoft's own support answers say reputation rests on file prevalence and telemetry, can fluctuate, and that recreating a certificate profile makes SmartScreen reassess your binaries. Treat signing as removing the "unknown publisher" line and getting onto the reputation ladder, not as a guaranteed clean first download.

**If you choose to ship unsigned now, skip to Part F.** Steps D2–D4 are here for when you come back.

### Step D2 — Set up Trusted Signing

In the Azure portal: create a Trusted Signing account, start the **organization identity validation** (this is the slow part — days, sometimes longer), then create a **Public Trust** certificate profile. Note the endpoint region, account name and profile name.

Register an Entra ID app registration with a client secret and give it the **Trusted Signing Certificate Profile Signer** role on the account. You need its client ID, tenant ID and secret.

### Step D3 — Wire the sign command

`src-tauri/tauri.conf.json` — add to the `bundle` block:

```json
  "bundle": {
    "active": true,
    "targets": "all",
    "createUpdaterArtifacts": true,
    "windows": {
      "signCommand": "trusted-signing-cli -e https://wus2.codesigning.azure.net -a YOUR_ACCOUNT -c YOUR_PROFILE -d AEM %1"
    },
```

Tauri replaces `%1` with each artifact to sign.

### Step D4 — Install the CLI and pass the credentials in CI

`.github/workflows/release.yml` — in the Windows matrix entry's steps:

```yaml
      - name: Install signing CLI
        if: runner.os == 'Windows'
        run: cargo install trusted-signing-cli

      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
          AZURE_CLIENT_ID: ${{ secrets.AZURE_CLIENT_ID }}           # ← add
          AZURE_TENANT_ID: ${{ secrets.AZURE_TENANT_ID }}           # ← add
          AZURE_CLIENT_SECRET: ${{ secrets.AZURE_CLIENT_SECRET }}   # ← add
```

**Test it:** this is the one test that cannot be done locally. A locally built `.msi` has no Mark-of-the-Web, so it never triggers SmartScreen at all — the v1 report hit exactly this. Tag a release, **download the `.msi` from the GitHub release page in a browser** on the Windows PC, and run it. Right-click → Properties should show a Digital Signatures tab with your name.

---

## Part E — macOS signing and notarization

### Step E1 — Understand what is required

A Developer ID signature alone is no longer enough. Gatekeeper requires the app to be **notarized** as well: uploaded to Apple, scanned, and the resulting ticket stapled to the bundle. Signing without notarizing gets you the same warning as not signing.

This needs an Apple Developer account at $99/year. Same recommendation as Part D: wait until you have users.

### Step E2 — Collect the credentials

From your Apple Developer account you need a **Developer ID Application** certificate exported as a `.p12`, and an **app-specific password** for your Apple ID (appleid.apple.com → Sign-In and Security → App-Specific Passwords).

Add as GitHub secrets:

- `APPLE_CERTIFICATE` — the `.p12`, base64 encoded (`base64 -i cert.p12 | pbcopy`)
- `APPLE_CERTIFICATE_PASSWORD` — the `.p12` export password
- `APPLE_SIGNING_IDENTITY` — e.g. `Developer ID Application: Your Name (TEAMID)`
- `APPLE_ID` — your Apple ID email
- `APPLE_PASSWORD` — the app-specific password
- `APPLE_TEAM_ID` — from the developer portal

### Step E3 — Pass them to the action

`.github/workflows/release.yml` — same `env:` block, these apply on the macOS runners:

```yaml
          APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
          APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
          APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
          APPLE_ID: ${{ secrets.APPLE_ID }}
          APPLE_PASSWORD: ${{ secrets.APPLE_PASSWORD }}
          APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
```

`tauri-action` signs and notarizes automatically when these are present. Notarization adds several minutes to the build.

**Test it:** download the `.dmg` from the release page (not a local build — same Mark-of-the-Web reasoning as Windows) and open it. No "unidentified developer" dialog. Verify with `spctl -a -vvv -t install /Applications/aem.app`.

---

## Part F — Diagnostics and launch

### Step F1 — One block of text that explains a stranger's machine

**Why:** the adb-on-PATH bug took process monitoring to find. After launch you will get "it just sits there spinning" from machines you cannot touch. This is the single feature that makes those reports answerable.

`src-tauri/src/commands.rs` — add:

```rust
/// Everything needed to understand a user's machine, as pasteable text.
#[tauri::command]
pub async fn collect_diagnostics(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let sdk = state.sdk()?;
    let host = state.host.clone();
    let version = app.package_info().version.to_string();
    let log_dir = app.path().app_log_dir().ok();

    tauri::async_runtime::spawn_blocking(move || {
        let mut out = String::new();
        out.push_str(&format!("AEM {version}\n"));
        out.push_str(&format!("host: {:?} {:?} ({:?})\n", host.os, host.arch, host.preferred_abi));
        out.push_str(&format!("ram: {} MB\n", host.total_ram_bytes / 1_048_576));
        out.push_str(&format!("sdk: {} (ours: {})\n", sdk.root.display(), sdk.managed_by_us));
        out.push_str(&format!("emulator: {:?}\n", sdk.emulator_bin()));
        out.push_str(&format!("adb: {:?}\n", sdk.adb_bin()));

        match engine::process::foreign_adb_on_path(&sdk) {
            Some(other) => out.push_str(&format!("WARNING other adb first on PATH: {}\n", other.display())),
            None => out.push_str("adb on PATH: ours (or none)\n"),
        }

        out.push_str(&format!("images: {}\n", sdk.installed_images().len()));
        out.push_str(&format!(
            "free disk: {:?} MB\n",
            engine::host::free_disk_bytes(&sdk.root).map(|b| b / 1_048_576)
        ));

        match engine::accel::check(&sdk) {
            Ok(report) => out.push_str(&format!(
                "accel: {:?} — {}\n",
                report.status, report.raw_message
            )),
            Err(e) => out.push_str(&format!("accel: check failed: {e}\n")),
        }

        if let Some(dir) = log_dir {
            let path = dir.join("avdhub.log");
            if let Ok(text) = std::fs::read_to_string(&path) {
                let tail: Vec<&str> = text.lines().rev().take(40).collect();
                out.push_str("\n--- last 40 log lines ---\n");
                for line in tail.into_iter().rev() {
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        out
    })
    .await
    .map_err(|e| e.to_string())
}
```

Add the import for `app.path()`:

```rust
use tauri::{Manager, State};    // ← was: use tauri::State;
```

Register it in `lib.rs`:

```rust
            commands::is_installing,
            commands::collect_diagnostics,      // ← add this
```

`src/app/core/tauri.service.ts`:

```ts
  collectDiagnostics(): Promise<string> {
    return invoke<string>('collect_diagnostics');
  }
```

Put the button somewhere permanent — the bottom of the devices screen is fine:

```html
<footer class="diagnostics">
  <button (click)="copyDiagnostics()">{{ copied() ? 'Copied' : 'Copy diagnostics' }}</button>
</footer>
```

```ts
  readonly copied = signal(false);

  async copyDiagnostics(): Promise<void> {
    try {
      await navigator.clipboard.writeText(await this.tauri.collectDiagnostics());
      this.copied.set(true);
      setTimeout(() => this.copied.set(false), 2000);
    } catch (e) {
      this.actionError.set(friendly(e));
    }
  }
```

> **Note:** the block contains file paths including the user's name. That is fine for a bug report they choose to paste, but say so next to the button — "copies your setup details so you can paste them into a bug report" — rather than collecting it silently.

### Step F2 — Clean up orphaned temp files at startup

A killed app, or an update applied at the wrong moment, leaves a partial `.zip` in `.avdhub-tmp`. Nothing ever removes it.

`src-tauri/src/state.rs` — in `AppState::new()`, next to the warm-up thread:

```rust
        // A crash or a forced quit can leave a partial download behind.
        let tmp = sdk.root.join(".avdhub-tmp");
        if tmp.exists() {
            match std::fs::remove_dir_all(&tmp) {
                Ok(()) => log::info!("cleared leftover temp files at {}", tmp.display()),
                Err(e) => log::warn!("could not clear {}: {e}", tmp.display()),
            }
        }
```

> **Note:** safe because this runs before any install can start, and `install_package` recreates the folder.

### Step F3 — The README

`README.md` — update the platform table to say what was actually tested, and add the adb note the v2 report asked for:

```markdown
## Supported platforms

| Platform | Status |
| --- | --- |
| macOS (Apple Silicon) | Tested |
| macOS (Intel) | Builds and cross-compiles in CI; not run on hardware |
| Windows 10/11 (x86-64) | Tested on Windows 10 22H2, with and without WHPX |
| Windows on ARM | Not tested — reports welcome |
| Linux | Builds; not a release target yet |

## Windows: hardware acceleration

Android emulators need hardware virtualisation. On Windows that means the
**Windows Hypervisor Platform** feature, which is off by default. AEM detects
this and walks you through enabling it; you will need to restart once. WHPX
works alongside Hyper-V, WSL2 and Docker Desktop.

## Windows: a note on adb

If you have another copy of `adb` on your PATH — from an old SDK, or a tool that
bundled one — the Android emulator may use it instead of AEM's, and devices can
flicker between states. AEM puts its own `adb` first for emulators it launches.
For emulators started by other tools, remove the stale copy from your PATH.
```

Keep the unsigned-build warning until Parts D and E are actually done; delete it the day they are.

### Step F4 — Ship it

```bash
npm run check:version
git tag v0.2.0
git push --tags
```

The workflow builds both platforms and opens a **draft** release. Before publishing:

1. Download the `.msi` and the `.dmg` from the draft in a browser and install from them.
2. Run the walkthrough once on each installed build.
3. Confirm `latest.json` is attached to the release.
4. Publish.

Then tag a throwaway `v0.2.1` with a trivial change and confirm the update banner appears in your installed `v0.2.0` and that "Update and restart" works. **That is the only real test of Part C**, and it needs two published releases to exist.

**Test it:** install `v0.2.0` from the published release, leave it open, publish `v0.2.1`, restart the app. The banner appears, the update installs, and the app relaunches reporting the new version.

---

## Final file structure

```
aem/
├── .github/workflows/
│   ├── ci.yml                               ← workspace lint + tests, Linux deps, version check
│   └── release.yml                          ← updater JSON, signing secrets, correct release name
├── scripts/
│   └── check-version.mjs                    ← NEW: three manifests and the tag must agree
├── engine/
│   ├── src/
│   │   ├── accel.rs                         ← real fixtures, classify_with_firmware, WMI check
│   │   ├── downloader.rs                    ← extract_zip takes a CancelFlag
│   │   └── process.rs                       ← PATH fix in launch, foreign_adb_on_path
│   └── tests/fixtures/
│       ├── accel_windows_ok.txt             ← NEW: real capture, WHPX on
│       └── accel_windows_no_whpx.txt        ← NEW: real capture, WHPX off
├── src-tauri/
│   ├── capabilities/default.json            ← updater + process permissions
│   ├── src/
│   │   ├── commands.rs                      ← is_installing, collect_diagnostics
│   │   ├── install.rs                       ← clears the installing flag
│   │   ├── lib.rs                           ← updater/process plugins, local log times
│   │   └── state.rs                         ← installing flag, temp cleanup
│   └── tauri.conf.json                      ← updater config, sign command
├── src/app/
│   ├── app.component.html/.ts               ← update banner
│   ├── core/
│   │   ├── tauri.service.ts                 ← isInstalling, collectDiagnostics
│   │   └── updates.service.ts               ← NEW: check, install, relaunch
│   ├── devices/devices.component.html/.ts   ← Stopping… state, diagnostics button
│   └── shared/install-progress.component.ts ← Cancel shows Stopping…
└── README.md                                ← platform table, acceleration and adb notes
```

## What to test

**Part A, on the Windows PC with `C:\adb` still on the PATH**

- [ ] A device launched from AEM reaches green in about three minutes, with no flickering
- [ ] Cancel during "Unpacking…" returns to the form within seconds, leaves no package folder and an empty `.avdhub-tmp`
- [ ] Stop shows "Stopping…" for about 25 seconds, then "Stopped"
- [ ] New log lines match your wall clock

**Part A, on the Mac**

- [ ] `cargo test -p engine` → 30 passing
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean

**Part B**

- [ ] CI green on all three runners with the wider lint
- [ ] `npm run check:version` fails if you change one version and not the others
- [ ] `windows-support` merged to `main`

**Part C**

- [ ] `npm run tauri build` produces `.sig` files next to the installers
- [ ] The app starts with no updater errors in the console
- [ ] With an install running, `is_installing` returns true and the banner does not appear
- [ ] After two published releases: the banner appears, updates and relaunches

**Parts D and E, only if you signed**

- [ ] `.msi` downloaded from the release page shows a Digital Signatures tab
- [ ] `.dmg` downloaded from the release page opens without an unidentified-developer dialog

**Part F**

- [ ] "Copy diagnostics" produces a block naming your SDK, accel status and any foreign adb
- [ ] Killing the app mid-download then restarting it clears `.avdhub-tmp`
- [ ] The README platform table matches what you actually tested

## What's coming next

Phase 5 is the last planned phase. What remains is a backlog rather than a sequence:

- **Snapshots** — save and restore device state
- **Drag-and-drop APK install** — the obvious next feature for a non-developer audience
- **Device skins and frames** — the `sdk` parameter in `avd::create` is already there waiting
- **Linux support** — the engine works; it needs testing and a package format
- **Resumable downloads** — HTTP Range, plus validating a partial file belongs to the same version
- **A CLI crate** — reusing the engine untouched, which is only cheap because it never knew about the UI

Pick from it after launch, when you know which one people ask for.

*End of Phase 5 — AEM*
