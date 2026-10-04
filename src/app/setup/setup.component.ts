import { Component, DestroyRef, OnInit, inject, signal } from '@angular/core';
import { Router } from '@angular/router';

import { friendlyError, type FriendlyError } from '../core/errors';
import type { InstallFinished, InstallProgress, License, SetupStatus } from '../core/models';
import { TauriService } from '../core/tauri.service';
import { InstallProgressComponent } from '../shared/install-progress.component';
import { LicenceStepComponent } from '../shared/licence-step.component';

type Step = 'intro' | 'licence' | 'installing';

@Component({
  selector: 'app-setup',
  imports: [InstallProgressComponent, LicenceStepComponent],
  templateUrl: './setup.component.html',
  styleUrl: './setup.component.css',
})
export class SetupComponent implements OnInit {
  private readonly tauri = inject(TauriService);
  private readonly router = inject(Router);

  readonly status = signal<SetupStatus | null>(null);
  readonly step = signal<Step>('intro');
  readonly licences = signal<License[]>([]);
  readonly progress = signal<InstallProgress | null>(null);
  readonly error = signal<FriendlyError | null>(null);

  private needed: string[] = [];

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
          void this.router.navigate(['/devices']);
        } else {
          this.error.set(friendlyError(r.error ?? 'Setup failed.'));
          this.step.set('intro');
        }
      },
      destroyRef,
    );
  }

  async ngOnInit(): Promise<void> {
    try {
      this.status.set(await this.tauri.getSetupStatus());
    } catch (e) {
      this.error.set(friendlyError(e));
    }
  }

  /** Work out what to download and show the licence before anything is accepted. */
  async getStarted(): Promise<void> {
    this.error.set(null);
    try {
      this.needed = await this.tauri.corePackagesNeeded();
      if (this.needed.length === 0) {
        void this.router.navigate(['/devices']);
        return;
      }
      const licences = await this.tauri.pendingLicenses(this.needed);
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
      this.step.set('intro');
    }
  }

  private async beginInstall(): Promise<void> {
    this.step.set('installing');
    this.progress.set(null);
    await this.tauri.startInstall(this.needed);
  }
}
