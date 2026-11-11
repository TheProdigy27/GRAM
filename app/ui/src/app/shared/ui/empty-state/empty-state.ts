import { Component, input } from '@angular/core';
import { Icon } from '../icon/icon';
import type { IconName } from '../icon/icons';

/**
 * État vide : icône, titre, explication et actions projetées
 * (« Aucun modèle ne correspond à … » + « Effacer la recherche »).
 */
@Component({
  selector: 'g-empty-state',
  imports: [Icon],
  template: `
    <g-icon class="g-empty-state__icon" [name]="icon()" [size]="24" />
    <strong class="g-empty-state__heading">{{ heading() }}</strong>
    @if (description()) {
      <span class="g-empty-state__description">{{ description() }}</span>
    }
    <div class="g-empty-state__actions"><ng-content /></div>
  `,
  host: { class: 'g-empty-state' },
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 8px;
      padding: 24px 16px;
      border: 1px dashed var(--g-input);
      border-radius: var(--g-radius-card);
      text-align: center;
    }
    .g-empty-state__icon {
      color: var(--g-muted);
    }
    .g-empty-state__heading {
      font-size: 13.5px;
      font-weight: 600;
      color: var(--g-fg);
    }
    .g-empty-state__description {
      font-size: 11.5px;
      color: var(--g-muted);
    }
    .g-empty-state__actions {
      display: flex;
      flex-wrap: wrap;
      justify-content: center;
      gap: 8px;
      margin-top: 4px;
    }
    .g-empty-state__actions:empty {
      display: none;
    }
  `,
})
export class EmptyState {
  readonly heading = input.required<string>();
  readonly description = input('');
  readonly icon = input<IconName>('search');
}
