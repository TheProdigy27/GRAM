import { Component, input } from '@angular/core';

/** Bloc de chargement (masqué aux lecteurs d'écran : l'état se dit avec `aria-busy`). */
@Component({
  selector: 'g-skeleton',
  template: '',
  host: {
    class: 'g-skeleton',
    'aria-hidden': 'true',
    '[style.width]': 'width()',
    '[style.height]': 'height()',
    '[style.border-radius]': 'radius()',
  },
  styles: `
    :host {
      display: block;
      background: var(--g-track);
      animation: g-skeleton-pulse 1.6s ease-in-out infinite;
    }
    @keyframes g-skeleton-pulse {
      50% {
        opacity: 0.55;
      }
    }
    @media (prefers-reduced-motion: reduce) {
      :host {
        animation: none;
      }
    }
  `,
})
export class Skeleton {
  readonly width = input('100%');
  readonly height = input('10px');
  readonly radius = input('4px');
}
