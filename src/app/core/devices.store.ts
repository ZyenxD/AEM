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
