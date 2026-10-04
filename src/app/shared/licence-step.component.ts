import { Component, input, output } from '@angular/core';

import type { License } from '../core/models';

/**
 * Shows the full licence text with Accept / Back buttons. Pressing Accept is
 * the only path to `acceptLicense`, which is the rule the engine documents.
 */
@Component({
  selector: 'app-licence-step',
  template: `
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
        <button type="button" (click)="back.emit()">Back</button>
        <button type="button" class="primary" (click)="accept.emit()">Accept and download</button>
      </div>
    </section>
  `,
})
export class LicenceStepComponent {
  readonly licences = input.required<License[]>();
  readonly accept = output<void>();
  readonly back = output<void>();
}
