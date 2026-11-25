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

    this.error.set(null);
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
    this.error.set(null);
  }
}
