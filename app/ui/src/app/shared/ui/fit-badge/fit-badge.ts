import { Component, computed, input } from '@angular/core';
import type { FitLevel } from '../../../core/generated/FitLevel';
import { FIT_LABELS } from '../../labels';
import { Icon } from '../icon/icon';
import type { IconName } from '../icon/icons';

const ICON_BY_FIT: Record<FitLevel, IconName> = {
  perfect: 'check',
  good: 'circle',
  marginal: 'warning',
  tooTight: 'minus-circle',
};

/** Adéquation mémoire : icône et libellé (parfait ✓, bon ●, limite ⚠, trop juste ⊖). */
@Component({
  selector: 'g-fit-badge',
  imports: [Icon],
  template: `<g-icon [name]="icon()" [size]="13" />{{ label() }}`,
  host: { class: 'g-fit-badge', '[attr.data-fit]': 'fit()' },
  styles: `
    :host {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      height: 24px;
      padding: 0 8px;
      border-radius: var(--g-radius-badge);
      font-size: 12px;
      font-weight: 500;
      line-height: 1;
      white-space: nowrap;
    }
    :host([data-fit='perfect']) {
      background: var(--g-ok-bg);
      color: var(--g-ok);
    }
    :host([data-fit='good']) {
      background: var(--g-accent-weak);
      color: var(--g-vram-fg);
    }
    :host([data-fit='marginal']) {
      background: var(--g-warn-bg);
      color: var(--g-warn);
    }
    :host([data-fit='tooTight']) {
      background: var(--g-error-bg);
      color: var(--g-error);
    }
  `,
})
export class FitBadge {
  readonly fit = input.required<FitLevel>();
  protected readonly icon = computed(() => ICON_BY_FIT[this.fit()]);
  protected readonly label = computed(() => FIT_LABELS[this.fit()]);
}
