import { Component, DestroyRef, OnInit, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';

import { DevicesStore } from '../core/devices.store';
import { friendlyError, type FriendlyError } from '../core/errors';
import type {
  DeviceProfile, InstallFinished, InstallProgress, License, SystemImage,
} from '../core/models';
import { TauriService } from '../core/tauri.service';
import { InstallProgressComponent } from '../shared/install-progress.component';
import { LicenceStepComponent } from '../shared/licence-step.component';

/** form → licence → installing → creating → back to the list; any error returns to form. */
type Step = 'form' | 'licence' | 'installing' | 'creating';

@Component({
  selector: 'app-create-device',
  imports: [FormsModule, InstallProgressComponent, LicenceStepComponent],
  templateUrl: './create-device.component.html',
  styleUrl: './create-device.component.css',
})
export class CreateDeviceComponent implements OnInit {
  private readonly tauri = inject(TauriService);
  private readonly store = inject(DevicesStore);
  private readonly router = inject(Router);

  readonly step = signal<Step>('form');
  readonly error = signal<FriendlyError | null>(null);

  readonly profiles = signal<DeviceProfile[]>([]);
  readonly images = signal<SystemImage[]>([]);
  readonly loadingImages = signal(true);

  readonly name = signal('My Android device');
  readonly profileId = signal('phone');
  readonly packagePath = signal<string | null>(null);

  readonly licences = signal<License[]>([]);
  readonly progress = signal<InstallProgress | null>(null);

  private pendingPackages: string[] = [];

  readonly selectedImage = computed(
    () => this.images().find((i) => i.packagePath === this.packagePath()) ?? null,
  );

  readonly canSubmit = computed(
    () => this.name().trim().length > 0 && this.packagePath() !== null && this.step() === 'form',
  );

  constructor() {
    const destroyRef = inject(DestroyRef);
    this.tauri.subscribe(
      (h) => this.tauri.onInstallProgress(h),
      (p: InstallProgress) => this.progress.set(p),
      destroyRef,
    );
    this.tauri.subscribe(
      (h) => this.tauri.onInstallFinished(h),
      (r: InstallFinished) => {
        if (this.step() !== 'installing') return;
        if (r.ok) {
          // Creation is the continuation of the install, which reports completion by event.
          void this.finishCreate();
        } else {
          this.error.set(friendlyError(r.error ?? 'The download failed.'));
          this.step.set('form');
        }
      },
      destroyRef,
    );
  }

  async ngOnInit(): Promise<void> {
    try {
      this.profiles.set(await this.tauri.listProfiles());
      const images = await this.tauri.listImages();
      this.images.set(images);
      // Prefer an image already on disk: a second device then needs no download at all.
      const preferred =
        images.find((i) => i.installed) ?? images.find((i) => i.recommended) ?? images[0];
      if (preferred) this.packagePath.set(preferred.packagePath);
    } catch (e) {
      this.error.set(friendlyError(e));
    } finally {
      this.loadingImages.set(false);
    }
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
      this.error.set(friendlyError(e));
    }
  }

  /** Called only from the Accept button, with the licence text on screen. */
  async acceptAll(): Promise<void> {
    try {
      for (const licence of this.licences()) {
        await this.tauri.acceptLicense(licence.id);
      }
      await this.beginInstall();
    } catch (e) {
      this.error.set(friendlyError(e));
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
      this.error.set(friendlyError(e));
      this.step.set('form');
    }
  }

  cancel(): void {
    void this.router.navigate(['/devices']);
  }
}
