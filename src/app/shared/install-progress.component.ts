import { DecimalPipe } from '@angular/common';
import { Component, input, output } from '@angular/core';

import type { InstallProgress } from '../core/models';

@Component({
  selector: 'app-install-progress',
  imports: [DecimalPipe],
  template: `
    @if (progress(); as p) {
      <p>{{ p.label }} <span class="muted">({{ p.index }} of {{ p.total }})</span></p>
      <progress [value]="p.percent" max="100"></progress>
      <p class="muted">
        @switch (p.stage) {
          @case ('downloading') { {{ p.percent }}% — {{ p.doneBytes / 1048576 | number: '1.0-0' }} MB }
          @case ('verifying') { Checking the download… }
          @case ('unpacking') { Unpacking… }
          @case ('finished') { Done }
        }
      </p>
    } @else {
      <p class="muted">Starting…</p>
    }
    <div class="actions">
      <button type="button" (click)="cancel.emit()">Cancel</button>
    </div>
  `,
})
export class InstallProgressComponent {
  readonly progress = input<InstallProgress | null>(null);
  readonly cancel = output<void>();
}
