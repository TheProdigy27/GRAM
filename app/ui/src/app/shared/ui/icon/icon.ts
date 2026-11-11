import { Component, computed, input, numberAttribute } from '@angular/core';
import { ICONS, type IconName } from './icons';

/**
 * Icône SVG à trait (24 × 24, trait 1,8, bouts ronds), en `currentColor`.
 * Décorative par défaut (`aria-hidden`) ; avec `label`, elle devient une image nommée.
 */
@Component({
  selector: 'g-icon',
  template: `<svg
    viewBox="0 0 24 24"
    focusable="false"
    aria-hidden="true"
    [attr.width]="size()"
    [attr.height]="size()"
    [attr.stroke-width]="strokeWidth()"
  >
    @for (d of paths(); track $index) {
      <svg:path [attr.d]="d" />
    }
  </svg>`,
  host: {
    class: 'g-icon',
    '[attr.data-icon]': 'name()',
    '[attr.role]': 'label() ? "img" : null',
    '[attr.aria-label]': 'label() || null',
    '[attr.aria-hidden]': 'label() ? null : "true"',
  },
  styles: `
    :host {
      display: inline-flex;
      flex: none;
      line-height: 0;
    }
    svg {
      fill: none;
      stroke: currentColor;
      stroke-linecap: round;
      stroke-linejoin: round;
    }
  `,
})
export class Icon {
  readonly name = input.required<IconName>();
  /** Côté en pixels. */
  readonly size = input(16, { transform: numberAttribute });
  readonly strokeWidth = input(1.8, { transform: numberAttribute });
  /** Nom accessible : à donner seulement si l'icône porte seule une information. */
  readonly label = input<string>('');

  protected readonly paths = computed(() => ICONS[this.name()]);
}
