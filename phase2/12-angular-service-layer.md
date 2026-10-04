# 12 — The Angular Service Layer

**Goal:** one typed, injectable seam between Angular and Tauri, so components never touch
`invoke` and never think about threads.

**Prerequisite:** documents 10–11. You already know Angular, so this document focuses on
the Tauri-specific parts and on one trap that will waste an evening if you meet it blind.

---

## 12.1 The rule

> **`src/app/core/tauri.service.ts` is the only file in the Angular app allowed to import
> from `@tauri-apps/api`.**

Everything else injects the service. This mirrors the Rust-side rule ("the engine never
imports Tauri") and buys the same things: components become testable with a mocked service,
and the day you add a web demo build or a second frontend, one file changes.

Install the API package if the template did not:

```bash
npm install @tauri-apps/api
```

---

## 12.2 The models

Create `src/app/core/models.ts`. These mirror the DTOs from document 10 exactly — camelCase
because of `#[serde(rename_all = "camelCase")]`, and `| null` because Rust's `Option<T>`
serialises to `null`.

```ts
export interface Device {
  name: string;
  displayName: string;
  apiLevel: number | null;
  androidName: string | null;
  abi: string | null;
  tag: string | null;
  ramMb: number | null;
  width: number | null;
  height: number | null;
  playStore: boolean;
  running: boolean;
  booted: boolean;
  serial: string | null;
}

export interface SystemImage {
  packagePath: string;
  apiLevel: number | null;
  androidName: string;
  tagId: string | null;
  tagLabel: string | null;
  sizeMb: number;
  installed: boolean;
  recommended: boolean;
}

export interface DeviceProfile {
  id: string;
  label: string;
  width: number;
  height: number;
  ramMb: number;
}

export interface License {
  id: string;
  text: string;
}

export interface SetupStatus {
  sdkRoot: string;
  managedByUs: boolean;
  hasEmulator: boolean;
  hasAdb: boolean;
  installedImageCount: number;
  abiLabel: string;
  osLabel: string;
  freeDiskGb: number | null;
  ready: boolean;
}

export type InstallStage = 'downloading' | 'verifying' | 'unpacking' | 'finished';

export interface InstallProgress {
  packagePath: string;
  label: string;
  index: number;
  total: number;
  stage: InstallStage;
  doneBytes: number;
  totalBytes: number;
  percent: number;
}

export interface InstallFinished {
  ok: boolean;
  error: string | null;
}
```

These are hand-written and therefore can drift from the Rust. Two mitigations, in order of
effort: keep the two files open side by side when you change a DTO, or adopt
`tauri-specta` later to generate this file from the Rust types. Hand-written is fine at
this size; generate when it stops being fine.

`InstallStage` as a union type is why the Rust side sends a plain string — TypeScript
narrows it in a `switch` with full exhaustiveness checking.

---

## 12.3 The trap: events arrive outside Angular's zone

This is the one that will confuse you.

Tauri delivers events by evaluating JavaScript in the webview from the Rust side. Those
callbacks originate outside anything Angular patched, so **Angular does not know a change
happened**. Your progress bar sits at 0%, the data is demonstrably correct in the console,
and nothing renders until you click somewhere unrelated — which triggers change detection
and suddenly shows 100%.

`invoke()` is *not* affected: it returns a normal Promise, which zone.js patches, so
awaiting a command updates the UI as expected. It is only `listen()` callbacks.

Two fixes, depending on your setup:

**With zone.js (the default):** wrap the callback body in `NgZone.run()`.

```ts
this.zone.run(() => this.progress.set(event.payload));
```

**Zoneless (`provideZonelessChangeDetection()`, Angular 20's direction):** signals notify
the scheduler themselves, so writing to a signal is enough and `NgZone` is unnecessary.

The service below uses `NgZone.run()`, which is harmless in a zoneless app and essential in
a zoned one. When you are certain you are zoneless, you can strip it.

---

## 12.4 The service

Create `src/app/core/tauri.service.ts`:

```ts
import { Injectable, NgZone, inject } from '@angular/core';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type {
  Device, DeviceProfile, InstallFinished, InstallProgress,
  License, SetupStatus, SystemImage,
} from './models';

@Injectable({ providedIn: 'root' })
export class TauriService {
  private readonly zone = inject(NgZone);

  // ---- setup ----

  getSetupStatus(): Promise<SetupStatus> {
    return invoke<SetupStatus>('get_setup_status');
  }

  // ---- catalog ----

  listImages(): Promise<SystemImage[]> {
    return invoke<SystemImage[]>('list_images');
  }

  refreshCatalog(): Promise<SystemImage[]> {
    return invoke<SystemImage[]>('refresh_catalog');
  }

  listProfiles(): Promise<DeviceProfile[]> {
    return invoke<DeviceProfile[]>('list_profiles');
  }

  // ---- devices ----

  listDevices(): Promise<Device[]> {
    return invoke<Device[]>('list_devices');
  }

  createDevice(displayName: string, profileId: string, packagePath: string): Promise<Device> {
    return invoke<Device>('create_device', { displayName, profileId, packagePath });
  }

  updateDevice(
    name: string,
    changes: { ramMb?: number; storageMb?: number; displayName?: string },
  ): Promise<Device> {
    return invoke<Device>('update_device', { name, ...changes });
  }

  deleteDevice(name: string): Promise<void> {
    return invoke<void>('delete_device', { name });
  }

  launchDevice(name: string, coldBoot = false, wipeData = false): Promise<number> {
    return invoke<number>('launch_device', { name, coldBoot, wipeData });
  }

  stopDevice(name: string): Promise<void> {
    return invoke<void>('stop_device', { name });
  }

  // ---- licences ----

  pendingLicenses(packagePaths: string[]): Promise<License[]> {
    return invoke<License[]>('pending_licenses', { packagePaths });
  }

  acceptLicense(id: string): Promise<void> {
    return invoke<void>('accept_license', { id });
  }

  // ---- installing ----

  packagesNeededFor(packagePath: string): Promise<string[]> {
    return invoke<string[]>('packages_needed_for', { packagePath });
  }

  startInstall(packagePaths: string[]): Promise<void> {
    return invoke<void>('start_install', { packagePaths });
  }

  onInstallProgress(handler: (p: InstallProgress) => void): Promise<UnlistenFn> {
    return listen<InstallProgress>('install-progress', (event) =>
      this.zone.run(() => handler(event.payload)),
    );
  }

  onInstallFinished(handler: (r: InstallFinished) => void): Promise<UnlistenFn> {
    return listen<InstallFinished>('install-finished', (event) =>
      this.zone.run(() => handler(event.payload)),
    );
  }
}
```

Things to notice:

**Argument names are camelCase and must match the Rust parameters after conversion.**
`{ displayName, profileId, packagePath }` maps to `display_name`, `profile_id`,
`package_path`. Get one wrong and you get a runtime error naming the missing argument —
annoying, but at least explicit.

**`invoke` rejects on `Err`.** A Rust command returning `Result<T, String>` produces a
resolved promise on `Ok` and a rejected one carrying the string on `Err`. So
`try/catch` around a service call gives you the engine's message directly. That is why the
commands bothered turning `EngineError` into readable sentences.

**`listen` returns a promise of an unlisten function.** Not calling it leaks a subscription
every time the component is recreated, and you end up with five handlers updating the same
progress bar. §12.6 handles that properly.

---

## 12.5 A small store for shared state

Devices are shown on the list screen and refreshed after actions on other screens. Rather
than each component fetching independently, put the shared slice in a signal store.

Create `src/app/core/devices.store.ts`:

```ts
import { Injectable, computed, inject, signal } from '@angular/core';

import type { Device } from './models';
import { TauriService } from './tauri.service';

@Injectable({ providedIn: 'root' })
export class DevicesStore {
  private readonly tauri = inject(TauriService);

  readonly devices = signal<Device[]>([]);
  readonly loading = signal(false);
  readonly error = signal<string | null>(null);

  readonly isEmpty = computed(() => !this.loading() && this.devices().length === 0);
  readonly runningCount = computed(() => this.devices().filter((d) => d.running).length);

  private pollHandle: ReturnType<typeof setInterval> | null = null;

  async refresh(): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    try {
      this.devices.set(await this.tauri.listDevices());
    } catch (e) {
      this.error.set(String(e));
    } finally {
      this.loading.set(false);
    }
  }

  /** Quietly re-read status; used by the poller so the UI does not flicker. */
  private async refreshQuiet(): Promise<void> {
    try {
      this.devices.set(await this.tauri.listDevices());
    } catch {
      /* transient adb hiccup: keep the previous list */
    }
  }

  startPolling(intervalMs = 4000): void {
    if (this.pollHandle !== null) return;
    this.pollHandle = setInterval(() => void this.refreshQuiet(), intervalMs);
  }

  stopPolling(): void {
    if (this.pollHandle !== null) {
      clearInterval(this.pollHandle);
      this.pollHandle = null;
    }
  }
}
```

**Why polling.** Nothing tells us when an emulator finishes booting — the emulator is a
separate process, and `sys.boot_completed` has to be asked for (document 08). Four seconds
is a reasonable compromise: responsive enough that the status dot feels live, infrequent
enough that you are not running `adb` constantly.

**`refreshQuiet` swallows errors deliberately.** A poll that fails because adb was
momentarily busy should not paint an error banner over a working app. The explicit
`refresh()` — triggered by the user — does surface errors.

**The `if (this.pollHandle !== null) return;` guard** stops a second timer being created
when the component re-initialises. Without it you get compounding poll rates, which is a
genuinely confusing bug to diagnose.

---

## 12.6 Cleaning up listeners

Event subscriptions must be torn down with the component. The modern way:

```ts
import { Component, DestroyRef, inject, signal } from '@angular/core';
import type { UnlistenFn } from '@tauri-apps/api/event';

import { TauriService } from '../core/tauri.service';
import type { InstallProgress } from '../core/models';

@Component({ /* ... */ })
export class SomeComponent {
  private readonly tauri = inject(TauriService);
  private readonly destroyRef = inject(DestroyRef);

  readonly progress = signal<InstallProgress | null>(null);

  constructor() {
    let unlisten: UnlistenFn | null = null;

    void this.tauri.onInstallProgress((p) => this.progress.set(p))
      .then((fn) => { unlisten = fn; });

    this.destroyRef.onDestroy(() => unlisten?.());
  }
}
```

The awkward shape — assigning into a variable from a promise, then calling it later —
exists because `listen()` is async but `onDestroy` is not. The `unlisten?.()` handles the
case where the component is destroyed before the listener finished registering.

If this appears in more than one component, wrap it:

```ts
// in TauriService
async subscribe<T>(
  subscribeFn: (handler: (v: T) => void) => Promise<UnlistenFn>,
  handler: (v: T) => void,
  destroyRef: DestroyRef,
): Promise<void> {
  const unlisten = await subscribeFn(handler);
  destroyRef.onDestroy(() => unlisten());
}
```

---

## 12.7 Prove the bridge works

Before building screens, verify the plumbing end to end. Replace `app.component.ts` with a
throwaway:

```ts
import { Component, inject, signal } from '@angular/core';
import { JsonPipe } from '@angular/common';

import { TauriService } from './core/tauri.service';
import type { SetupStatus, Device } from './core/models';

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [JsonPipe],
  template: `
    <h1>avdhub smoke test</h1>
    <button (click)="loadStatus()">Load setup status</button>
    <button (click)="loadDevices()">Load devices</button>
    <pre>{{ status() | json }}</pre>
    <pre>{{ devices() | json }}</pre>
    <p style="color: crimson">{{ error() }}</p>
  `,
})
export class AppComponent {
  private readonly tauri = inject(TauriService);

  readonly status = signal<SetupStatus | null>(null);
  readonly devices = signal<Device[]>([]);
  readonly error = signal<string | null>(null);

  async loadStatus() {
    try { this.status.set(await this.tauri.getSetupStatus()); }
    catch (e) { this.error.set(String(e)); }
  }

  async loadDevices() {
    try { this.devices.set(await this.tauri.listDevices()); }
    catch (e) { this.error.set(String(e)); }
  }
}
```

```bash
npm run tauri dev
```

Click both buttons. You should see the JSON for your real SDK and — because you ran
`first_boot` in Phase 2 — the "AvdHub Test Device" you created from the terminal, now
listed by your own app.

**That moment is the actual milestone of Phase 3.** Everything after it is interface work.

If a call fails with "command not found", the command is missing from `generate_handler!`
(document 10). If it fails with an argument name, compare the `invoke` object keys with the
Rust parameter names.

---

## 12.8 Checkpoint

- [ ] `npm run tauri dev` opens without console errors.
- [ ] "Load setup status" prints your real SDK path and `ready: true`.
- [ ] "Load devices" lists the device `first_boot` created.
- [ ] Only `tauri.service.ts` imports from `@tauri-apps/api`.

```bash
git add -A && git commit -m "Add Angular service layer and device store"
```

---

## 12.9 Understanding check

1. Why do `listen` callbacks need `NgZone.run()` when `invoke` promises do not?
2. What happens if a component subscribes to `install-progress` and never unlistens?
3. Why does `refreshQuiet` swallow errors while `refresh` records them?
4. Why is polling necessary for device status rather than an event from Rust?
5. What breaks if a Rust DTO field is renamed but `models.ts` is not updated — and when
   would you find out?
