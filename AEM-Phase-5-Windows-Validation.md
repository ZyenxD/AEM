# AEM — Phase 5 Windows Validation

*How to test the Phase 5 changes on the Windows test PC. Branch `windows-support`, version `0.2.0`.*

The Mac has already covered everything that can run there (see the last section). This document covers what can **only** be checked on Windows: the adb PATH fix, the WHPX/BIOS check, cancel during unpack, the Stop label, local log times, temp cleanup, diagnostics and the Windows updater artifacts.

Fill in each **Result** line as you go. When something fails, attach the evidence the test asks for: the diagnostics block, a screenshot, and the last lines of `avdhub.log`.

---

## 0. Before you start

### 0.1 The machine

| Requirement | Check |
| --- | --- |
| Windows 10 22H2 (19045), x86-64 — the PC used for the v1/v2 reports | `winver` |
| **`C:\adb\adb.exe` still on the PATH, ahead of everything else.** Do not remove it: it is the reproduction case for test 2 | `where.exe adb` → `C:\adb\adb.exe` is listed **first** |
| Rust stable, MSVC toolchain | `rustc -vV` → `host: x86_64-pc-windows-msvc` |
| Visual Studio Build Tools with "Desktop development with C++" | the build succeeds |
| **Node ≥ 20.19 or ≥ 22.12.** Angular 20 refuses older versions (20.9 fails) | `node -v` |
| WebView2 runtime (preinstalled on Windows 10/11) | — |
| About 10 GB of free disk on the SDK drive, for test 3 | — |

### 0.2 Useful paths

| What | Where |
| --- | --- |
| AEM's SDK | `%LOCALAPPDATA%\avdhub\sdk` (the diagnostics `sdk:` line shows which SDK is in use) |
| AEM's adb | `%LOCALAPPDATA%\avdhub\sdk\platform-tools\adb.exe` |
| Temp downloads | `%LOCALAPPDATA%\avdhub\sdk\.avdhub-tmp` |
| App log | `%LOCALAPPDATA%\com.neycasilla.aem\logs\avdhub.log` |
| Emulator output per device | `%TEMP%\avdhub-emulator-<device>.log` |

### 0.3 Get the code

```powershell
git fetch
git checkout windows-support
git pull
npm ci
```

---

## 1. Automated gate (must pass before anything manual)

Run these from the repo root in PowerShell:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run check:version
npm run build
```

| Expected | Result |
| --- | --- |
| `fmt` prints nothing and exits 0 | |
| `clippy` finishes with no warnings. On Windows it also compiles the PowerShell firmware query (`#[cfg(windows)]`), which the Mac cannot build | |
| `cargo test`: **engine 30 passed**, `aem_lib` 2 passed, 0 failed | |
| `Version 0.2.0 is consistent.` | |
| Angular build ends with `Application bundle generation complete` | |

> If `cargo test` fails only in `accel::tests` on a checkout made with `core.autocrlf=true`, check that `.gitattributes` is present. It keeps `engine/tests/fixtures/*.txt` as LF. The tests should pass with CRLF too; a failure here is worth reporting.

---

## 2. adb PATH fix (Steps A1, A2) — the High finding

**What changed:** emulators that AEM starts get AEM's `platform-tools` folder placed first on their PATH, so the emulator's internal `adb` calls use AEM's adb (client 41), not `C:\adb` (client 31).

### 2.1 Confirm the reproduction case is still in place

```powershell
where.exe adb
& C:\adb\adb.exe version
```

**Expected:** `C:\adb\adb.exe` is listed first and reports an old version (protocol/client 31). If it is missing, put it back before going on, or the test proves nothing.

Result: ___

### 2.2 Device reaches green

1. Run the app: `npm run tauri dev` (or install the build from test 9).
2. Start a device that already exists and has its image installed. Note the time.
3. Watch the dot and the status label.

**Expected:** the status goes **Starting…** (amber), then **Running** (green) in about **3 minutes** (the v2 report measured 207 s without the stale adb), with no flicker back to amber or grey. Before the fix it never turned green within 400 s.

Time to green: ___ s  Result: ___

### 2.3 Only AEM's adb is talking to the emulator

While the device is running:

```powershell
Get-CimInstance Win32_Process -Filter "Name='adb.exe'" |
  Select-Object ProcessId, ExecutablePath, CommandLine | Format-List
```

**Expected:** every `adb.exe` comes from `...\avdhub\sdk\platform-tools\`. **None** comes from `C:\adb\adb.exe`.

Result: ___

Optional, for extra confidence: in Process Explorer, open the `emulator.exe` (or `qemu-system-x86_64.exe`) process → Properties → Environment and confirm `Path` starts with `...\avdhub\sdk\platform-tools`.

### 2.4 Diagnostics reports the foreign adb (A2 + F1)

On the Devices screen, click **Copy diagnostics** and paste into Notepad.

**Expected:** a line `WARNING other adb first on PATH: C:\adb\adb.exe`. That is correct: the app's own PATH still has `C:\adb` first, and only the emulators it launches are fixed.

Then temporarily remove `C:\adb` from the user PATH, restart the app, and copy diagnostics again. **Expected:** `adb on PATH: ours (or none)`. Put `C:\adb` back afterwards.

Result (with): ___  Result (without): ___

### 2.5 Known limit, nothing to fix

An emulator started outside AEM (Android Studio, or `emulator -avd X` in a shell) still inherits the shell's PATH and may still flicker. That is expected and is documented in the README.

---

## 3. Cancel during unpacking (Steps A3, A4) — the other High finding

You need a package that is **not installed yet**, so there is something large to unpack. A Google APIs x86_64 system image (~1.5 GB) is a good choice.

1. In the app: **New device**, then pick an Android version that is not installed, accept the licence and start the install.
2. Wait until the text reads **Unpacking…**.
3. Click **Cancel** and start a stopwatch.

| Expected | Result |
| --- | --- |
| The button immediately changes to **Stopping…** and is greyed out (it cannot be clicked twice) | |
| The flow returns to the form within **a few seconds**. Before the fix it ran ~104 s more and reported success | |
| The image's folder (e.g. `%LOCALAPPDATA%\avdhub\sdk\system-images\android-XX\google_apis\x86_64`) does **not** exist | |
| `%LOCALAPPDATA%\avdhub\sdk\.avdhub-tmp` is empty | |
| The log has `install of system-images;... cancelled` | |
| The Android version does **not** show as installed when you reopen New device | |

Checking commands:

```powershell
Get-ChildItem "$env:LOCALAPPDATA\avdhub\sdk\.avdhub-tmp" -Force
Test-Path "$env:LOCALAPPDATA\avdhub\sdk\system-images\android-XX\google_apis\x86_64"   # expect False
```

**Regression checks:** cancel again, this time during **downloading**. It should still stop within a second and leave no partial `.zip`. Then run the install to completion and confirm the image installs normally.

Cancel-during-download: ___  Full install still works: ___

---

## 4. Acceleration checks (Steps A5, A6)

### 4.1 The firmware query itself

```powershell
(Get-CimInstance Win32_Processor | Select-Object -First 1).VirtualizationFirmwareEnabled
```

**Expected:** `True` on the test PC. If it prints nothing, the CPU driver does not report it. AEM then treats it as "no opinion" and behaves exactly as before, which is acceptable. Note which one you got.

Output: ___

### 4.2 WHPX off → WHPX steps (not BIOS steps)

1. Turn **Windows Hypervisor Platform** off (`optionalfeatures.exe`, untick it) and restart.
2. Confirm the capture still matches: `& "$env:LOCALAPPDATA\avdhub\sdk\emulator\emulator.exe" -accel-check; $LASTEXITCODE` → the AEHD message and exit code **6**.
3. In AEM, start any device.

**Expected:** the acceleration panel says *"Windows needs one feature switched on…"* and lists the **Windows Hypervisor Platform** steps, including the `DISM ... /FeatureName:HypervisorPlatform` command. It must **not** show the Task Manager / BIOS steps, because firmware is on. It must not mention AEHD.

Result: ___

### 4.3 WHPX on → Ready

Turn the feature back on, restart, and start a device.

**Expected:** no panel. The device starts. `-accel-check` prints `WHPX(10.0.19045) is installed and usable.` and exit code 0.

Result: ___

### 4.4 Optional: VT-x off in the BIOS

Only if a machine is available where you can switch virtualisation off in the firmware.

**Expected:** Task Manager → Performance → CPU shows *Virtualization: Disabled*, the 4.1 query prints `False`, and AEM's panel says *"Your computer's virtualisation feature is switched off."* with the **Check Task Manager** and **BIOS/UEFI** steps. Whatever the emulator prints, these are the steps shown.

**Please save the emulator's `-accel-check` output from this machine.** It would replace the last invented fixture (`WINDOWS_NO_VTX` in `engine/src/accel.rs`).

Result: ___

---

## 5. "Stopping…" label (Step A7)

With a device running (green), click **Stop**.

**Expected:** the label reads **Stopping…** for about **23–26 s**, then **Stopped** (grey). It never shows *Running* or *Starting…* in between, and there is no error.

Duration: ___ s  Result: ___

---

## 6. Log uses local time (Step A7)

1. Note the wall clock (taskbar).
2. Start or stop anything in the app.
3. Read the last lines:

```powershell
Get-Content "$env:LOCALAPPDATA\com.neycasilla.aem\logs\avdhub.log" -Tail 5
```

**Expected:** new lines show **local** time, matching the taskbar clock to the second. Lines written by the old build may still be in UTC; only new lines count.

Result: ___

---

## 7. Orphaned temp files are cleared at startup (Step F2)

1. Start an install of a large package that is not installed yet.
2. While it is **downloading**, kill the app hard: Task Manager → `aem` → **End task**. Closing the window normally is not the same test.
3. Confirm a partial `.zip` was left behind:
   ```powershell
   Get-ChildItem "$env:LOCALAPPDATA\avdhub\sdk\.avdhub-tmp"
   ```
4. Start the app again.

**Expected:** `.avdhub-tmp` is gone, and the log has `cleared leftover temp files at ...\.avdhub-tmp`. A new install afterwards works.

Result: ___

> Windows-specific risk: if an antivirus scanner or a zombie process holds the file open, removal can fail. The log then says `could not clear ...`. Report that if you see it; the app should still start normally.

---

## 8. Copy diagnostics (Step F1)

Click **Copy diagnostics** on the Devices screen and paste the result.

| Line | Expected | Result |
| --- | --- | --- |
| `AEM 0.2.0` | the version | |
| `host:` | `Windows X86_64 (X86_64)` or similar | |
| `ram:` | roughly the PC's RAM in MB | |
| `sdk:` | `...\avdhub\sdk (ours: true)` | |
| `emulator:` / `adb:` | `Some("...emulator.exe")` / `Some("...adb.exe")` | |
| adb on PATH | the WARNING line while `C:\adb` is first (see 2.4) | |
| `images:` | number of installed images | |
| `free disk:` | `Some(<MB>)` | |
| `accel:` | `Ready — WHPX(10.0.19045) is installed and usable.` with WHPX on | |
| `--- last 40 log lines ---` | the tail of `avdhub.log` | |

Also check:

- The button changes to **Copied** for about 2 s.
- **No console window flashes** while it runs. It starts adb, the emulator's accel check and (only if acceleration fails) PowerShell, all of which must stay hidden.
- It takes a few seconds at most.
- The note next to the button says it copies folder paths.

Result: ___

---

## 9. Updater artifacts and startup (Steps C2–C4)

### 9.1 Build without a key fails (expected)

```powershell
npm run tauri build
```

**Expected:** the bundling **fails** near the end with an error about a missing private key (`TAURI_SIGNING_PRIVATE_KEY`). That is correct: `createUpdaterArtifacts` is on, and the real key does not exist yet (Step C1 has not been done).

Result: ___

### 9.2 Build with a throwaway key produces signatures

This tests the pipeline only. Do **not** use this key for a release.

```powershell
npx tauri signer generate --ci -p '""' -w "$env:TEMP\aem-throwaway.key" -f
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$env:TEMP\aem-throwaway.key" -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
$pub = (Get-Content "$env:TEMP\aem-throwaway.key.pub" -Raw).Trim()
npx tauri build --config "{`"plugins`":{`"updater`":{`"pubkey`":`"$pub`"}}}"
Get-ChildItem target\release\bundle -Recurse -Include *.msi,*.exe,*.sig
```

**Expected:** under `target\release\bundle\` you find

- `msi\aem_0.2.0_x64_en-US.msi` **and** `msi\aem_0.2.0_x64_en-US.msi.sig`
- `nsis\aem_0.2.0_x64-setup.exe` **and** `nsis\aem_0.2.0_x64-setup.exe.sig`

(Tauri v2 signs the installers directly. The `.msi.zip` named in the tutorial is the old v1 format and is not expected.)

Afterwards: `Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY, Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

Result: ___

### 9.3 The installed app starts cleanly

Install the `.msi` from 9.2 and launch AEM from the Start menu.

**Expected:** the app opens normally with **no update banner**. The log has `update endpoint did not respond with a successful status code`. That is correct until a release with `latest.json` is published, and it must not produce any dialog.

Result: ___

### 9.4 The installing flag

In `npm run tauri dev`, right-click the window → **Inspect** → Console. Start an install, then run:

```js
await window.__TAURI_INTERNALS__.invoke('is_installing')
```

**Expected:** `true` while downloading or unpacking. `false` after it finishes, **and** `false` after a cancel (test 3) or a failed install (e.g. pull the network cable mid-download).

Result (during / after finish / after cancel / after failure): ___ / ___ / ___ / ___

---

## 10. After the real key and two releases (Steps C1, F4)

These need the real signing key and published releases, so run them last.

1. Do Step C1 on the Mac: generate the key, back it up, add the GitHub secrets and paste the **public** key into `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`).
2. Tag and publish `v0.2.0`. On the Windows PC, **download the `.msi` from the GitHub release page in a browser**, not a local build, so it has Mark-of-the-Web. Install it.

| Expected | Result |
| --- | --- |
| SmartScreen shows "Windows protected your PC" (unsigned — expected until Part D). More info → Run anyway works | |
| The release has `latest.json` attached, listing a `windows-x86_64` entry | |
| AEM 0.2.0 runs the full walkthrough (setup → create device → start → stop) | |

3. Publish a throwaway `v0.2.1` (bump all three versions; `npm run check:version` must pass). Restart the installed 0.2.0.

| Expected | Result |
| --- | --- |
| Banner: **AEM 0.2.1 is available.** with **Update and restart** / **Later** | |
| **Later** hides it; it comes back on the next launch | |
| With an install running, no banner appears at startup. Clicking **Update and restart** during an install shows *"Finish or cancel the current download first."* | |
| **Update and restart** shows the small passive installer progress window with no clicks needed, and the app relaunches | |
| Diagnostics now reads `AEM 0.2.1`. Devices and SDK are untouched | |

---

## 11. Only if Part D (Windows code signing) was done

Download the `.msi` from the release page in a browser and install it.

**Expected:** right-click → Properties → **Digital Signatures** tab shows your name. SmartScreen no longer says "unknown publisher". It may still warn until reputation builds; that is expected.

Result: ___

---

## Already validated on the Mac (2026-11-23)

| Check | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean (now includes the Tauri app) |
| `cargo test --workspace` | engine **30** passed, `aem_lib` 2 passed |
| Windows-only code (`firmware_virtualization_enabled`, `hide_console`) | type-checks and passes clippy for `x86_64-pc-windows-msvc`, built on its own. The full engine cannot cross-compile from the Mac because `ring` needs the MSVC C toolchain |
| `foreign_adb_on_path` | stale adb first on PATH → reported. Ours first → `None`. No adb on PATH → `None` |
| `npm run check:version` | passes at 0.2.0. Fails on one mismatched manifest and on a tag that disagrees |
| `npm run build` | succeeds |
| Release build signed with a throwaway key | `aem.app.tar.gz` + `aem.app.tar.gz.sig` produced |
| Built app startup | temp cleanup removed a real leftover partial zip, log timestamps match local time, updater check fails quietly (no `latest.json` yet) |

## Reporting back

For each failure, send:

1. The test number and what happened instead of the expected result
2. The **Copy diagnostics** block
3. `Get-Content "$env:LOCALAPPDATA\com.neycasilla.aem\logs\avdhub.log" -Tail 60`
4. For device-status problems: `%TEMP%\avdhub-emulator-<device>.log` and the output of the 2.3 command
