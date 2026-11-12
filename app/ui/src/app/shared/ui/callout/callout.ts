import { Component, computed, input } from '@angular/core';
import { Icon } from '../icon/icon';
import type { IconName } from '../icon/icons';

export type CalloutKind = 'info' | 'warn' | 'error';

const ICON_BY_KIND: Record<CalloutKind, IconName> = {
  info: 'info',
  warn: 'warning',
  error: 'error',
};

/**
 * Encadré d'information, d'attention ou d'erreur, avec une action optionnelle
 * (`[gCalloutAction]`, par exemple « Réessayer »). Pour une erreur qui apparaît en cours
 * d'usage, ajouter `role="alert"` sur l'élément. `icon` remplace l'icône du genre.
 */
@Component({
  selector: 'g-callout',
  imports: [Icon],
  template: `
    <g-icon class="g-callout__icon" [name]="icon()" />
    <div class="g-callout__body">
      @if (heading()) {
        <strong class="g-callout__heading">{{ heading() }}</strong>
      }
      <ng-content />
    </div>
    <ng-content select="[gCalloutAction]" />
  `,
  host: { class: 'g-callout', '[attr.data-kind]': 'kind()' },
  styles: `
    :host {
      display: flex;
      align-items: flex-start;
      gap: 10px;
      padding: 10px 12px;
      border-radius: 10px;
      background: var(--g-accent-weak);
      color: var(--g-vram-fg);
      font-size: 12.5px;
      line-height: 1.45;
    }
    :host([data-kind='warn']) {
      background: var(--g-warn-bg);
      color: var(--g-warn);
    }
    :host([data-kind='error']) {
      background: var(--g-error-bg);
      color: var(--g-error);
    }
    .g-callout__icon {
      margin-top: 1px;
    }
    .g-callout__body {
      flex: 1;
      min-width: 0;
      align-self: center;
    }
    .g-callout__heading {
      display: block;
      font-weight: 600;
    }
  `,
})
export class Callout {
  readonly kind = input<CalloutKind>('info');
  readonly heading = input('');
  readonly iconName = input<IconName | undefined>(undefined, { alias: 'icon' });
  protected readonly icon = computed(() => this.iconName() ?? ICON_BY_KIND[this.kind()]);
}
