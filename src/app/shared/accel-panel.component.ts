import { Component, inject, input, output, signal } from '@angular/core';

import type { AccelReport } from '../core/models';
import { TauriService } from '../core/tauri.service';

/** Explains why emulators would be slow and how to fix it. Never blocks: "Start anyway" exists. */
@Component({
  selector: 'app-accel-panel',
  template: `
    <section class="accel-panel">
      <h2>{{ report().summary }}</h2>

      <ol class="steps">
        @for (step of report().fixSteps; track step.title) {
          <li>
            <strong>{{ step.title }}</strong>
            @if (step.requiresAdmin) { <span class="badge">needs administrator</span> }
            <p>{{ step.detail }}</p>
            @if (step.command; as command) {
              <div class="command">
                <code>{{ command }}</code>
                <button type="button" (click)="copy(command)">{{ copied() === command ? 'Copied' : 'Copy' }}</button>
              </div>
            }
          </li>
        }
      </ol>

      @if (report().requiresReboot) {
        <p class="note">Your computer needs to restart before this takes effect.</p>
      }

      <div class="actions">
        @if (report().status === 'hypervisorDisabled') {
          <button type="button" (click)="openFeatures()">Open Windows features</button>
        }
        <button type="button" class="primary" (click)="recheck.emit()">Check again</button>
        @if (allowStartAnyway()) {
          <button type="button" (click)="startAnyway.emit()">Start anyway</button>
        } @else {
          <button type="button" (click)="dismiss.emit()">Dismiss</button>
        }
      </div>

      @if (report().rawMessage) {
        <details>
          <summary>Technical details</summary>
          <pre>{{ report().rawMessage }}</pre>
        </details>
      }
    </section>
  `,
})
export class AccelPanelComponent {
  private readonly tauri = inject(TauriService);

  readonly report = input.required<AccelReport>();
  readonly allowStartAnyway = input(true);
  readonly recheck = output<void>();
  readonly startAnyway = output<void>();
  readonly dismiss = output<void>();

  readonly copied = signal<string | null>(null);

  async copy(text: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
      this.copied.set(text);
    } catch {
      /* clipboard blocked: the command is still visible to select by hand */
    }
  }

  openFeatures(): void {
    void this.tauri.openWindowsFeatures();
  }
}
