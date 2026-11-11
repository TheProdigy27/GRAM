import { Component, input } from '@angular/core';
import { Icon } from '../icon/icon';
import type { IconName } from '../icon/icons';

export type TagTone = 'neutral' | 'accent' | 'ok' | 'warn' | 'error';

/**
 * Étiquette : Vision (icône `eye`), Installé (`ok`), Protégé (`lock`), ≈ approximatif,
 * Simulation (`warn`), ★ meilleure quantification (`accent`, icône `star`)…
 */
@Component({
  selector: 'g-tag',
  imports: [Icon],
  template: `@if (icon(); as name) {
      <g-icon [name]="name" [size]="size() === 'sm' ? 12 : 13" />
    }
    <ng-content />`,
  host: { class: 'g-tag', '[attr.data-tone]': 'tone()', '[attr.data-size]': 'size()' },
  styles: `
    :host {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      height: 24px;
      padding: 0 8px;
      border-radius: var(--g-radius-badge);
      background: var(--g-track);
      color: var(--g-fg-2);
      font-size: 12px;
      font-weight: 500;
      line-height: 1;
      white-space: nowrap;
    }
    :host([data-size='sm']) {
      height: 20px;
      padding: 0 6px;
      font-size: 11px;
    }
    :host([data-tone='accent']) {
      background: var(--g-accent-weak);
      color: var(--g-vram-fg);
    }
    :host([data-tone='ok']) {
      background: var(--g-ok-bg);
      color: var(--g-ok);
    }
    :host([data-tone='warn']) {
      background: var(--g-warn-bg);
      color: var(--g-warn);
    }
    :host([data-tone='error']) {
      background: var(--g-error-bg);
      color: var(--g-error);
    }
  `,
})
export class Tag {
  readonly tone = input<TagTone>('neutral');
  readonly icon = input<IconName>();
  readonly size = input<'md' | 'sm'>('md');
}
