import { Component, input, numberAttribute } from '@angular/core';

/**
 * Indicateur d'activité en CSS, purement visuel : l'état « en cours » se dit ailleurs
 * (`aria-busy` du bouton, texte, `g-progress`).
 */
@Component({
  selector: 'g-spinner',
  template: '',
  host: {
    class: 'g-spinner',
    'aria-hidden': 'true',
    '[style.width.px]': 'size()',
    '[style.height.px]': 'size()',
  },
  styles: `
    :host {
      display: inline-block;
      flex: none;
      border-radius: 50%;
      border: 2px solid currentColor;
      border-right-color: transparent;
      opacity: 0.85;
      animation: g-spin 0.8s linear infinite;
    }
    @keyframes g-spin {
      to {
        transform: rotate(360deg);
      }
    }
    @media (prefers-reduced-motion: reduce) {
      :host {
        animation-duration: 2.4s;
      }
    }
  `,
})
export class Spinner {
  readonly size = input(16, { transform: numberAttribute });
}
