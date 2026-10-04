import { Component, OnDestroy, OnInit, inject, signal } from '@angular/core';
import { RouterLink } from '@angular/router';

import { DevicesStore } from '../core/devices.store';
import { friendly, type FriendlyError } from '../core/errors';
import type { AccelReport, Device } from '../core/models';
import { TauriService } from '../core/tauri.service';
import { AccelPanelComponent } from '../shared/accel-panel.component';
import { ErrorComponent } from '../shared/error.component';

@Component({
  selector: 'app-devices',
  imports: [RouterLink, AccelPanelComponent, ErrorComponent],
  templateUrl: './devices.component.html',
  styleUrl: './devices.component.css',
})
export class DevicesComponent implements OnInit, OnDestroy {
  private readonly tauri = inject(TauriService);
  readonly store = inject(DevicesStore);

  /** Name of the device being acted on, so only that row shows as busy. */
  readonly busy = signal<string | null>(null);
  readonly actionError = signal<FriendlyError | null>(null);
  readonly confirmingDelete = signal<string | null>(null);
  /** Set when acceleration is missing; the panel replaces a launch into a frozen window. */
  readonly accelReport = signal<AccelReport | null>(null);
  private accelPendingDevice: Device | null = null;

  // Stop polling adb while the window is hidden.
  private readonly onVisibilityChange = (): void => {
    if (document.hidden) {
      this.store.stopPolling();
    } else {
      void this.store.refresh();
      this.store.startPolling();
    }
  };

  ngOnInit(): void {
    void this.store.refresh();
    this.store.startPolling();
    document.addEventListener('visibilitychange', this.onVisibilityChange);
  }

  ngOnDestroy(): void {
    this.store.stopPolling();
    document.removeEventListener('visibilitychange', this.onVisibilityChange);
  }

  async launch(device: Device, skipAccelCheck = false): Promise<void> {
    this.busy.set(device.name);
    this.actionError.set(null);
    try {
      if (!skipAccelCheck) {
        const accel = await this.tauri.checkAcceleration();
        if (accel.status !== 'ready' && accel.status !== 'emulatorMissing') {
          this.accelPendingDevice = device;
          this.accelReport.set(accel);
          return;
        }
      }
      await this.tauri.launchDevice(device.name);
      // The emulator takes a moment to register with adb.
      setTimeout(() => void this.store.refresh(), 2000);
    } catch (e) {
      this.actionError.set(friendly(e));
    } finally {
      this.busy.set(null);
    }
  }

  async stop(device: Device): Promise<void> {
    this.busy.set(device.name);
    this.actionError.set(null);
    try {
      await this.tauri.stopDevice(device.name);
      setTimeout(() => void this.store.refresh(), 1500);
    } catch (e) {
      this.actionError.set(friendly(e));
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
      this.actionError.set(friendly(e));
    }
  }

  async recheckAccel(): Promise<void> {
    try {
      const accel = await this.tauri.checkAcceleration();
      if (accel.status === 'ready' && this.accelPendingDevice) {
        const device = this.accelPendingDevice;
        this.closeAccel();
        await this.launch(device, true);
      } else {
        this.accelReport.set(accel);
      }
    } catch (e) {
      this.actionError.set(friendly(e));
    }
  }

  startAnyway(): void {
    const device = this.accelPendingDevice;
    this.closeAccel();
    if (device) void this.launch(device, true);
  }

  closeAccel(): void {
    this.accelReport.set(null);
    this.accelPendingDevice = null;
  }

  statusLabel(device: Device): string {
    if (device.booted) return 'Running';
    if (device.running) return 'Starting…';
    return 'Stopped';
  }
}
