import { Component, computed, input } from '@angular/core';

/**
 * Barre de progression (`role="progressbar"`) : déterminée si `value` est un nombre, sinon
 * indéterminée (animation, sans `aria-valuenow`).
 */
@Component({
  selector: 'g-progress',
  template: `<span
    class="g-progress__bar"
    [class.g-progress__bar--indeterminate]="indeterminate()"
    [style.width.%]="indeterminate() ? null : percent()"
  ></span>`,
  host: {
    class: 'g-progress',
    role: 'progressbar',
    'aria-valuemin': '0',
    '[attr.aria-valuemax]': 'max()',
    '[attr.aria-valuenow]': 'indeterminate() ? null : clamped()',
    '[attr.aria-valuetext]': 'valueText() || null',
    '[attr.aria-label]': 'label() || null',
    '[attr.data-size]': 'size()',
  },
  styles: `
    :host {
      position: relative;
      display: block;
      height: 6px;
      border-radius: 3px;
      background: var(--g-track);
      overflow: hidden;
    }
    :host([data-size='sm']) {
      height: 3px;
    }
    .g-progress__bar {
      position: absolute;
      inset: 0 auto 0 0;
      border-radius: inherit;
      background: var(--g-accent);
      transition: width 0.2s;
    }
    .g-progress__bar--indeterminate {
      width: 30%;
      animation: g-progress-slide 1.4s ease-in-out infinite;
    }
    @keyframes g-progress-slide {
      from {
        transform: translateX(-100%);
      }
      to {
        transform: translateX(340%);
      }
    }
    @media (prefers-reduced-motion: reduce) {
      .g-progress__bar {
        transition: none;
      }
      .g-progress__bar--indeterminate {
        width: 100%;
        opacity: 0.5;
        animation: none;
      }
    }
  `,
})
export class Progress {
  /** Avancement entre 0 et `max` ; `null` : indéterminé. */
  readonly value = input<number | null | undefined>(null);
  readonly max = input(100);
  readonly label = input('');
  /** Texte lu à la place du pourcentage (« 96 fichiers sur 150 »). */
  readonly valueText = input('');
  readonly size = input<'md' | 'sm'>('md');

  protected readonly indeterminate = computed(() => {
    const v = this.value();
    return v === null || v === undefined || Number.isNaN(v);
  });
  protected readonly clamped = computed(() => Math.min(this.max(), Math.max(0, this.value() ?? 0)));
  protected readonly percent = computed(() =>
    this.max() > 0 ? (this.clamped() / this.max()) * 100 : 0,
  );
}
