import { Component, inject } from '@angular/core';
import { Router, RouterOutlet } from '@angular/router';

import { TauriService } from './core/tauri.service';
import { UpdatesService } from './core/updates.service';

@Component({
  selector: 'app-root',
  imports: [RouterOutlet],
  templateUrl: './app.component.html',
  styleUrl: './app.component.css',
})
export class AppComponent {
  private readonly tauri = inject(TauriService);
  private readonly router = inject(Router);
  readonly updates = inject(UpdatesService);

  constructor() {
    void this.checkSetup();
    void this.updates.checkQuietly();
  }

  /** Send the user to first-run setup only when the core tools are missing. */
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
