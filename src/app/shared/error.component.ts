import { Component, input } from '@angular/core';

import type { FriendlyError } from '../core/errors';

@Component({
  selector: 'app-error',
  template: `
    @if (error(); as e) {
      <div class="error-box">
        <strong>{{ e.message }}</strong>
        @if (e.suggestion) { <p>{{ e.suggestion }}</p> }
        <details>
          <summary>Technical details</summary>
          <pre>{{ e.technical }}</pre>
        </details>
      </div>
    }
  `,
})
export class ErrorComponent {
  readonly error = input<FriendlyError | null>(null);
}
