# 13 — The Screens

**Goal:** three screens that make the engine usable by someone who has never heard of an
AVD.

**Prerequisite:** document 12 — the smoke test must show your real devices.

---

## 13.1 What we are building

```
┌─ First run (only when setup is incomplete) ─┐
│  detect → install emulator + adb → done     │
└─────────────────────────────────────────────┘
            ↓
┌─ Devices ──────────────────────────────────┐
│  ● My Pixel 8   Android 15 · Google Play   │
│    2 GB · 1080×2400            [▶] [⋯]     │
│                                             │
│  ○ Tablet test  Android 14                  │
│    4 GB · 1600×2560            [▶] [⋯]     │
│                          [+ New device]     │
└─────────────────────────────────────────────┘
            ↓
┌─ Create device ────────────────────────────┐
│  Name  [My Pixel 8          ]              │
│  Type  (•) Phone  ( ) Tablet  ( ) TV       │
│  Android  [Android 15 · Google Play  ▾]    │
│           1.6 GB download                   │
│                     [Cancel] [Create]       │
└─────────────────────────────────────────────┘
```

Three product rules that should guide every decision:

1. **Never show a word the user did not choose to learn.** "AVD", "ABI", "API level",
   "system image", "sdkmanager" — all absent from the interface. "Device", "Android 15",
   "Google Play" are fine.
2. **Never offer something that cannot work.** The image list is already filtered to this
   machine's ABI (document 04), so an Apple Silicon user simply never sees an x86 option.
3. **Downloads are explained before they start.** "1.6 GB" appears before the button, not
   after the click.

---

## 13.2 Routing

`src/app/app.routes.ts`:

```ts
import type { Routes } from '@angular/router';

export const routes: Routes = [
  { path: '', pathMatch: 'full', redirectTo: 'devices' },
  {
    path: 'devices',
    loadComponent: () => import('./devices/devices.component').then((m) => m.DevicesComponent),
  },
  {
    path: 'devices/new',
    loadComponent: () => import('./create-device/create-device.component').then((m) => m.CreateDeviceComponent),
  },
  {
    path: 'setup',
    loadComponent: () => import('./setup/setup.component').then((m) => m.SetupComponent),
  },
];
```

`app.component.ts` becomes a shell:

```ts
import { Component, inject } from '@angular/core';
import { Router, RouterOutlet } from '@angular/router';

import { TauriService } from './core/tauri.service';

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [RouterOutlet],
  template: `<router-outlet />`,
})
export class AppComponent {
  private readonly tauri = inject(TauriService);
  private readonly router = inject(Router);

  constructor() {
    void this.checkSetup();
  }

  private async checkSetup(): Promise<void> {
    try {
      const status = await this.tauri.getSetupStatus();
      if (!status.hasEmulator || !status.hasAdb) {
        void this.router.navigate(['/setup']);
      }
    } catch {
      void this.router.navigate(['/setup']);
    }
  }
}
```

The setup check runs once at startup and redirects only when the core tools are missing.
Note it does **not** require an installed image — a user with the tools but no images
belongs on the devices screen, where the empty state invites them to create one.

---

## 13.3 The devices screen

`src/app/devices/devices.component.ts`:

```ts
import { Component, OnDestroy, OnInit, inject, signal } from '@angular/core';
import { RouterLink } from '@angular/router';

import { DevicesStore } from '../core/devices.store';
import { TauriService } from '../core/tauri.service';
import type { Device } from '../core/models';

@Component({
  selector: 'app-devices',
  standalone: true,
  imports: [RouterLink],
  templateUrl: './devices.component.html',
  styleUrl: './devices.component.css',
})
export class DevicesComponent implements OnInit, OnDestroy {
  private readonly tauri = inject(TauriService);
  readonly store = inject(DevicesStore);

  readonly busy = signal<string | null>(null);
  readonly actionError = signal<string | null>(null);
  readonly confirmingDelete = signal<string | null>(null);

  ngOnInit(): void {
    void this.store.refresh();
    this.store.startPolling();
  }

  ngOnDestroy(): void {
    this.store.stopPolling();
  }

  async launch(device: Device): Promise<void> {
    this.busy.set(device.name);
    this.actionError.set(null);
    try {
      await this.tauri.launchDevice(device.name);
      // The emulator takes a moment to register with adb.
      setTimeout(() => void this.store.refresh(), 2000);
    } catch (e) {
      this.actionError.set(String(e));
    } finally {
      this.busy.set(null);
    }
  }

  async stop(device: Device): Promise<void> {
    this.busy.set(device.name);
    try {
      await this.tauri.stopDevice(device.name);
      setTimeout(() => void this.store.refresh(), 1500);
    } catch (e) {
      this.actionError.set(String(e));
    } finally {
      this.busy.set(null);
    }
  }

  async confirmDelete(device: Device): Promise<void> {
    try {
      await this.tauri.deleteDevice(device.name);
      this.confirmingDelete.set(null);
      await this.store.refresh();
    } catch (e) {
      this.actionError.set(String(e));
    }
  }

  statusLabel(device: Device): string {
    if (device.booted) return 'Running';
    if (device.running) return 'Starting…';
    return 'Stopped';
  }
}
```

**`busy` holds a device name, not a boolean.** That way only the row being acted on shows a
spinner, and the rest of the list stays usable.

**The `setTimeout` after launching** is a pragmatic acknowledgement that the emulator needs
a second or two before adb sees it. Without it, the immediate refresh shows "Stopped" and
the user thinks the click failed. The 4-second poller would fix it eventually; this makes
it feel instant.

**Three status states, not two.** "Starting…" (process visible to adb, `booted` false) is
the difference between an app that feels responsive and one that looks stuck for the ninety
seconds a cold boot takes.

`devices.component.html`:

```html
<header class="bar">
  <h1>Devices</h1>
  <a routerLink="/devices/new" class="primary">New device</a>
</header>

@if (store.loading() && store.devices().length === 0) {
  <p class="muted">Loading…</p>
}

@if (store.error(); as err) {
  <p class="error">{{ err }}</p>
}

@if (actionError(); as err) {
  <p class="error">{{ err }}</p>
}

@if (store.isEmpty()) {
  <section class="empty">
    <h2>No devices yet</h2>
    <p>Create one to run Android on your computer.</p>
    <a routerLink="/devices/new" class="primary">Create your first device</a>
  </section>
}

<ul class="devices">
  @for (device of store.devices(); track device.name) {
    <li class="device" [class.running]="device.running">
      <span class="dot" [class.on]="device.booted" [class.pending]="device.running && !device.booted"></span>

      <div class="info">
        <strong>{{ device.displayName }}</strong>
        <span class="meta">
          {{ device.androidName ?? 'Android' }}
          @if (device.tag) { · {{ device.tag }} }
          @if (device.ramMb) { · {{ device.ramMb }} MB }
          @if (device.width && device.height) { · {{ device.width }}×{{ device.height }} }
        </span>
        <span class="status">{{ statusLabel(device) }}</span>
      </div>

      <div class="actions">
        @if (device.running) {
          <button (click)="stop(device)" [disabled]="busy() === device.name">Stop</button>
        } @else {
          <button class="primary" (click)="launch(device)" [disabled]="busy() === device.name">Start</button>
        }
        <button class="danger" (click)="confirmingDelete.set(device.name)">Delete</button>
      </div>
    </li>

    @if (confirmingDelete() === device.name) {
      <li class="confirm">
        Delete “{{ device.displayName }}” and everything on it?
        <button class="danger" (click)="confirmDelete(device)">Delete</button>
        <button (click)="confirmingDelete.set(null)">Cancel</button>
      </li>
    }
  }
</ul>
```

`@if` / `@for` is Angular's built-in control flow. `track device.name` is required and
should be a stable identity — the sanitised name is exactly that.

The delete confirmation is inline rather than a modal: fewer moving parts, and the user can
see which device they are about to destroy.

---

## 13.4 The create-device screen

This screen carries the most product weight: image selection, licence acceptance, download
progress, and creation, in one flow.

```ts
import { Component, OnInit, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import type { UnlistenFn } from '@tauri-apps/api/event';

import { DevicesStore } from '../core/devices.store';
import { TauriService } from '../core/tauri.service';
import type { DeviceProfile, InstallProgress, License, SystemImage } from '../core/models';

type Step = 'form' | 'licence' | 'installing' | 'creating';

@Component({
  selector: 'app-create-device',
  standalone: true,
  imports: [FormsModule],
  templateUrl: './create-device.component.html',
  styleUrl: './create-device.component.css',
})
export class CreateDeviceComponent implements OnInit {
  private readonly tauri = inject(TauriService);
  private readonly store = inject(DevicesStore);
  private readonly router = inject(Router);

  readonly step = signal<Step>('form');
  readonly error = signal<string | null>(null);

  readonly profiles = signal<DeviceProfile[]>([]);
  readonly images = signal<SystemImage[]>([]);
  readonly loadingImages = signal(true);

  readonly name = signal('My Android device');
  readonly profileId = signal('phone');
  readonly packagePath = signal<string | null>(null);

  readonly licences = signal<License[]>([]);
  readonly progress = signal<InstallProgress | null>(null);

  private pendingPackages: string[] = [];
  private unlisteners: UnlistenFn[] = [];

  readonly selectedImage = computed(() =>
    this.images().find((i) => i.packagePath === this.packagePath()) ?? null,
  );

  readonly canSubmit = computed(() =>
    this.name().trim().length > 0 && this.packagePath() !== null && this.step() === 'form',
  );

  async ngOnInit(): Promise<void> {
    this.profiles.set(await this.tauri.listProfiles());
    try {
      const images = await this.tauri.listImages();
      this.images.set(images);
      const preferred = images.find((i) => i.installed) ?? images.find((i) => i.recommended) ?? images[0];
      if (preferred) this.packagePath.set(preferred.packagePath);
    } catch (e) {
      this.error.set(String(e));
    } finally {
      this.loadingImages.set(false);
    }

    this.unlisteners.push(
      await this.tauri.onInstallProgress((p) => this.progress.set(p)),
      await this.tauri.onInstallFinished((r) => {
        if (r.ok) {
          void this.finishCreate();
        } else {
          this.error.set(r.error ?? 'The download failed.');
          this.step.set('form');
        }
      }),
    );
  }

  ngOnDestroy(): void {
    this.unlisteners.forEach((fn) => fn());
  }

  async submit(): Promise<void> {
    const image = this.packagePath();
    if (!image) return;
    this.error.set(null);

    try {
      this.pendingPackages = await this.tauri.packagesNeededFor(image);

      if (this.pendingPackages.length === 0) {
        await this.finishCreate();
        return;
      }

      const licences = await this.tauri.pendingLicenses(this.pendingPackages);
      if (licences.length > 0) {
        this.licences.set(licences);
        this.step.set('licence');
        return;
      }

      await this.beginInstall();
    } catch (e) {
      this.error.set(String(e));
    }
  }

  async acceptAll(): Promise<void> {
    try {
      for (const licence of this.licences()) {
        await this.tauri.acceptLicense(licence.id);
      }
      await this.beginInstall();
    } catch (e) {
      this.error.set(String(e));
      this.step.set('form');
    }
  }

  private async beginInstall(): Promise<void> {
    this.step.set('installing');
    this.progress.set(null);
    await this.tauri.startInstall(this.pendingPackages);
  }

  private async finishCreate(): Promise<void> {
    this.step.set('creating');
    try {
      await this.tauri.createDevice(this.name().trim(), this.profileId(), this.packagePath()!);
      await this.store.refresh();
      void this.router.navigate(['/devices']);
    } catch (e) {
      this.error.set(String(e));
      this.step.set('form');
    }
  }

  cancel(): void {
    void this.router.navigate(['/devices']);
  }
}
```

The flow reads as a state machine, which is why `Step` exists as a type: `form → licence →
installing → creating → navigate away`, with any error returning to `form`.

**`finishCreate` is called from the event handler,** not from `beginInstall`. The install
is asynchronous and reports completion by event, so device creation is the continuation of
`install-finished`. This is the part that would be a tangle without the event design from
document 11.

**Defaulting to an already-installed image** (`images.find((i) => i.installed)`) means a
returning user creates a second device with no download at all. Small touch, large
difference.

The template:

```html
<header class="bar">
  <h1>New device</h1>
</header>

@if (error(); as err) { <p class="error">{{ err }}</p> }

@switch (step()) {
  @case ('form') {
    <form (ngSubmit)="submit()">
      <label>
        Name
        <input name="name" [ngModel]="name()" (ngModelChange)="name.set($event)" />
      </label>

      <fieldset>
        <legend>Type</legend>
        @for (profile of profiles(); track profile.id) {
          <label class="radio">
            <input type="radio" name="profile" [value]="profile.id"
                   [ngModel]="profileId()" (ngModelChange)="profileId.set($event)" />
            {{ profile.label }}
            <span class="muted">{{ profile.width }}×{{ profile.height }}</span>
          </label>
        }
      </fieldset>

      <label>
        Android version
        @if (loadingImages()) {
          <span class="muted">Loading available versions…</span>
        } @else {
          <select name="image" [ngModel]="packagePath()" (ngModelChange)="packagePath.set($event)">
            @for (image of images(); track image.packagePath) {
              <option [value]="image.packagePath">
                {{ image.androidName }}@if (image.tagLabel) { · {{ image.tagLabel }} }
                @if (image.installed) { — installed } @else { — {{ image.sizeMb }} MB download }
              </option>
            }
          </select>
        }
      </label>

      @if (selectedImage(); as image) {
        @if (!image.installed) {
          <p class="note">
            {{ image.androidName }} needs a {{ image.sizeMb }} MB download the first time.
          </p>
        }
      }

      <div class="actions">
        <button type="button" (click)="cancel()">Cancel</button>
        <button type="submit" class="primary" [disabled]="!canSubmit()">Create</button>
      </div>
    </form>
  }

  @case ('licence') {
    <section class="licence">
      <h2>Google's terms</h2>
      <p class="muted">Android is made by Google. To download it, you need to accept their licence.</p>
      @for (licence of licences(); track licence.id) {
        <article>
          <h3>{{ licence.id }}</h3>
          <pre class="licence-text">{{ licence.text }}</pre>
        </article>
      }
      <div class="actions">
        <button type="button" (click)="step.set('form')">Back</button>
        <button type="button" class="primary" (click)="acceptAll()">Accept and download</button>
      </div>
    </section>
  }

  @case ('installing') {
    <section class="installing">
      <h2>Downloading</h2>
      @if (progress(); as p) {
        <p>{{ p.label }} <span class="muted">({{ p.index }} of {{ p.total }})</span></p>
        <progress [value]="p.percent" max="100"></progress>
        <p class="muted">
          @switch (p.stage) {
            @case ('downloading') { {{ p.percent }}% — {{ (p.doneBytes / 1048576) | number:'1.0-0' }} MB }
            @case ('verifying') { Checking the download… }
            @case ('unpacking') { Unpacking… }
            @case ('finished') { Done }
          }
        </p>
      } @else {
        <p class="muted">Starting…</p>
      }
    </section>
  }

  @case ('creating') {
    <p class="muted">Creating your device…</p>
  }
}
```

The licence text is rendered in a scrollable `<pre>` — the real agreement, as document 06
required. The Accept button is the only path forward, and it is the only thing that calls
`acceptLicense`.

`| number:'1.0-0'` needs `DecimalPipe`; add `imports: [FormsModule, DecimalPipe]` to the
component.

---

## 13.5 The first-run screen

`src/app/setup/setup.component.ts`:

```ts
import { Component, OnInit, inject, signal } from '@angular/core';
import { Router } from '@angular/router';
import type { UnlistenFn } from '@tauri-apps/api/event';

import { TauriService } from '../core/tauri.service';
import type { InstallProgress, SetupStatus } from '../core/models';

@Component({
  selector: 'app-setup',
  standalone: true,
  templateUrl: './setup.component.html',
  styleUrl: './setup.component.css',
})
export class SetupComponent implements OnInit {
  private readonly tauri = inject(TauriService);
  private readonly router = inject(Router);

  readonly status = signal<SetupStatus | null>(null);
  readonly installing = signal(false);
  readonly progress = signal<InstallProgress | null>(null);
  readonly error = signal<string | null>(null);

  private unlisteners: UnlistenFn[] = [];

  async ngOnInit(): Promise<void> {
    try {
      this.status.set(await this.tauri.getSetupStatus());
    } catch (e) {
      this.error.set(String(e));
    }

    this.unlisteners.push(
      await this.tauri.onInstallProgress((p) => this.progress.set(p)),
      await this.tauri.onInstallFinished(async (r) => {
        this.installing.set(false);
        if (r.ok) {
          void this.router.navigate(['/devices']);
        } else {
          this.error.set(r.error ?? 'Setup failed.');
        }
      }),
    );
  }

  ngOnDestroy(): void {
    this.unlisteners.forEach((fn) => fn());
  }

  async install(): Promise<void> {
    this.error.set(null);
    this.installing.set(true);
    try {
      // An empty package path asks only for the core tools.
      const needed = await this.tauri.packagesNeededFor('');
      const licences = await this.tauri.pendingLicenses(needed);
      for (const licence of licences) {
        await this.tauri.acceptLicense(licence.id);
      }
      await this.tauri.startInstall(needed);
    } catch (e) {
      this.error.set(String(e));
      this.installing.set(false);
    }
  }
}
```

**A problem to fix as you go:** this calls `packagesNeededFor('')` and auto-accepts
licences, which violates the document 06 rule. Do it properly — show the licence text here
too, with its own Accept button, exactly as the create screen does. The version above is
deliberately left as the shortcut so you can see the difference; fixing it is your first
task on this screen.

Also note `packages_needed_for` will push an empty string onto the list when given one.
Guard it in Rust:

```rust
        if !package_path.is_empty() {
            let already = sdk.installed_images().iter().any(|i| i.package_path == package_path);
            if !already {
                needed.push(package_path);
            }
        }
```

The template is simple: explain what will be downloaded, one button, then progress.

```html
<section class="setup">
  <h1>Welcome to avdhub</h1>
  <p>Run Android on your computer — no Android Studio required.</p>

  @if (status(); as s) {
    <dl class="facts">
      <dt>Your computer</dt><dd>{{ s.osLabel }} · {{ s.abiLabel }}</dd>
      <dt>Install location</dt><dd>{{ s.sdkRoot }}</dd>
      @if (s.freeDiskGb !== null) { <dt>Free space</dt><dd>{{ s.freeDiskGb }} GB</dd> }
    </dl>

    @if (!installing()) {
      <p>avdhub needs to download the Android emulator (about 400 MB) before it can create devices.</p>
      <button class="primary" (click)="install()">Get started</button>
    }
  }

  @if (installing()) {
    @if (progress(); as p) {
      <p>{{ p.label }} <span class="muted">({{ p.index }} of {{ p.total }})</span></p>
      <progress [value]="p.percent" max="100"></progress>
    } @else {
      <p class="muted">Starting…</p>
    }
  }

  @if (error(); as err) { <p class="error">{{ err }}</p> }
</section>
```

Showing the install location and free space up front is a small honesty signal: the user
learns where their gigabytes are going before agreeing to spend them.

---

## 13.6 Styling

Keep it minimal. A single `src/styles.css` with a few custom properties will carry the
whole app:

```css
:root {
  --bg: #16181d;
  --panel: #1e2128;
  --text: #e8eaed;
  --muted: #9aa0a6;
  --accent: #4b8bf5;
  --danger: #e5484d;
  --ok: #3fb950;
  --radius: 10px;
  color-scheme: dark;
}

body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font: 15px/1.5 system-ui, -apple-system, "Segoe UI", sans-serif;
}

.dot { width: 9px; height: 9px; border-radius: 50%; background: var(--muted); }
.dot.pending { background: #d29922; }
.dot.on { background: var(--ok); }

button.primary, a.primary {
  background: var(--accent); color: white; border: 0;
  padding: 8px 14px; border-radius: var(--radius); cursor: pointer;
}
button.danger { color: var(--danger); background: transparent; border: 1px solid currentColor; }
.muted { color: var(--muted); }
.error { color: var(--danger); }
.licence-text { max-height: 280px; overflow: auto; white-space: pre-wrap; background: var(--panel); padding: 12px; }
```

`color-scheme: dark` makes native controls (scrollbars, `<select>`, `<progress>`) match.
Resist building a design system now; ship the flow, then make it beautiful.

---

## 13.7 Checkpoint

- [ ] Starting with no devices shows the empty state.
- [ ] Creating a device with an already-installed image works without any download.
- [ ] Creating one with a new image shows the licence, then a moving progress bar.
- [ ] Start turns the dot amber, then green within a couple of minutes.
- [ ] Stop returns it to grey.
- [ ] Delete asks first, then removes the device.
- [ ] The window never freezes during a download.

```bash
git add -A && git commit -m "Add devices, create-device and setup screens"
```

---

## 13.8 Understanding check

1. Why is `finishCreate` triggered by the `install-finished` event rather than called
   directly after `startInstall`?
2. What would the user see if the device list had only "Running" and "Stopped" states?
3. Why does the create screen default to an installed image when one exists?
4. The setup screen auto-accepts licences. What rule does that break, and how do you fix
   it?
5. Why is `track device.name` required in `@for`, and what breaks with an unstable key?
