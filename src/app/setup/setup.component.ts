import { Component, DestroyRef, OnInit, inject, signal } from '@angular/core';
import { Router } from '@angular/router';

import { friendly, type FriendlyError } from '../core/errors';
import type {
  AccelReport, InstallFinished, InstallProgress, License, SetupStatus,
} from '../core/models';
import { TauriService } from '../core/tauri.service';
import { AccelPanelComponent } from '../shared/accel-panel.component';
import { ErrorComponent } from '../shared/error.component';
import { InstallProgressComponent } from '../shared/install-progress.component';
import { LicenceStepComponent } from '../shared/licence-step.component';

type Step = 'intro' | 'licence' | 'installing' | 'accel';

@Component({
  selector: 'app-setup',
  imports: [AccelPanelComponent, ErrorComponent, InstallProgressComponent, LicenceStepComponent],
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
  readonly notice = signal<string | null>(null);
  readonly accelReport = signal<AccelReport | null>(null);

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
        if (r.cancelled) {
          this.notice.set('Download stopped. You can start again whenever you like.');
          this.step.set('intro');
        } else if (r.ok) {
          void this.afterInstall();
        } else {
          this.error.set(friendly(r.error ?? 'Setup failed.'));
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
      this.error.set(friendly(e));
    }
  }

  /** Work out what to download and show the licence before anything is accepted. */
  async getStarted(): Promise<void> {
    this.error.set(null);
    this.notice.set(null);
    try {
      this.needed = await this.tauri.corePackagesNeeded();
      if (this.needed.length === 0) {
        void this.router.navigate(['/devices']);
        return;
      }
      const space = await this.tauri.checkSpace(this.needed);
      if (!space.enough) {
        this.error.set(
          friendly(
            `You need about ${space.neededMb} MB free, but only ${space.availableMb} MB is available.`,
          ),
        );
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
      this.error.set(friendly(e));
    }
  }

  /** Warn about missing acceleration now, so a restart happens before devices are made. */
  private async afterInstall(): Promise<void> {
    try {
      const accel = await this.tauri.checkAcceleration();
      if (accel.status !== 'ready' && accel.status !== 'emulatorMissing') {
        this.accelReport.set(accel);
        this.step.set('accel');
        return;
      }
    } catch {
      /* the check is advisory; carry on */
    }
    void this.router.navigate(['/devices']);
  }

  async recheckAccel(): Promise<void> {
    try {
      const accel = await this.tauri.checkAcceleration();
      if (accel.status === 'ready') {
        void this.router.navigate(['/devices']);
      } else {
        this.accelReport.set(accel);
      }
    } catch (e) {
      this.error.set(friendly(e));
    }
  }

  continueToDevices(): void {
    void this.router.navigate(['/devices']);
  }

  async cancelInstall(): Promise<void> {
    try {
      await this.tauri.cancelInstall();
    } catch (e) {
      this.error.set(friendly(e));
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
      this.error.set(friendly(e));
      this.step.set('intro');
    }
  }

  private async beginInstall(): Promise<void> {
    this.step.set('installing');
    this.progress.set(null);
    await this.tauri.startInstall(this.needed);
  }
}
