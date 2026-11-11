import { Component, ElementRef, booleanAttribute, inject, input, model } from '@angular/core';
import { Icon } from '../icon/icon';
import type { IconName } from '../icon/icons';
import { Tooltip } from '../tooltip/tooltip';

export interface ToggleOption<T> {
  value: T;
  label: string;
  icon?: IconName;
  /** Infobulle de l'option. */
  tooltip?: string;
  disabled?: boolean;
}

/**
 * Choix unique parmi quelques options, en boutons à bascule (`aria-pressed`) :
 * `segmented` (contrôle segmenté) ou `chips` (pastilles, par exemple le contexte 2k … 128k).
 * Chaque bouton reste accessible à la tabulation ; ← → passent aussi d'un bouton à l'autre.
 */
@Component({
  selector: 'g-toggle-group',
  imports: [Icon, Tooltip],
  template: `
    @for (o of options(); track o.value) {
      <button
        type="button"
        class="g-toggle"
        [attr.aria-pressed]="o.value === value()"
        [attr.data-value]="o.value"
        [disabled]="disabled() || o.disabled"
        [gTooltip]="o.tooltip"
        (click)="value.set(o.value)"
        (keydown)="onKeydown($event)"
      >
        @if (o.icon) {
          <g-icon [name]="o.icon" />
        }
        {{ o.label }}
      </button>
    }
  `,
  host: {
    class: 'g-toggle-group',
    role: 'group',
    '[attr.aria-label]': 'label() || null',
    '[attr.data-appearance]': 'appearance()',
    '[class.g-toggle-group--fill]': 'fill()',
    '[class.g-toggle-group--grid]': '!!columns()',
    '[style.grid-template-columns]':
      'columns() ? "repeat(" + columns() + ", minmax(0, 1fr))" : null',
  },
  styles: `
    :host {
      display: inline-flex;
      flex-wrap: wrap;
      gap: 6px;
    }
    :host(.g-toggle-group--fill) {
      display: grid;
      grid-auto-columns: minmax(0, 1fr);
      grid-auto-flow: column;
    }
    :host(.g-toggle-group--grid) {
      display: grid;
    }
    .g-toggle {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 6px;
      height: 32px;
      padding: 0 12px;
      border-radius: var(--g-radius);
      border: 1px solid var(--g-input);
      background: var(--g-surface);
      color: var(--g-fg-2);
      font: inherit;
      font-size: 12.5px;
      font-weight: 500;
      white-space: nowrap;
      cursor: pointer;
    }
    .g-toggle:hover:not(:disabled) {
      background: var(--g-hover);
    }
    .g-toggle[aria-pressed='true'],
    .g-toggle[aria-pressed='true']:hover {
      border-color: var(--g-chip-on);
      background: var(--g-chip-on);
      color: var(--g-chip-on-fg);
    }
    .g-toggle:focus-visible {
      outline: 2px solid var(--g-accent);
      outline-offset: 2px;
    }
    .g-toggle:disabled {
      color: var(--g-disabled-fg);
      cursor: not-allowed;
    }
    :host([data-appearance='segmented']) {
      gap: 2px;
      padding: 3px;
      border-radius: 10px;
      background: var(--g-seg);
    }
    :host([data-appearance='segmented']) .g-toggle {
      border: 0;
      border-radius: 7px;
      background: transparent;
      color: var(--g-muted);
    }
    :host([data-appearance='segmented']) .g-toggle:hover:not(:disabled) {
      color: var(--g-fg);
    }
    :host([data-appearance='segmented']) .g-toggle[aria-pressed='true'] {
      background: var(--g-surface);
      color: var(--g-fg);
      font-weight: 600;
      box-shadow: var(--g-shadow);
    }
  `,
})
export class ToggleGroup<T> {
  readonly options = input.required<readonly ToggleOption<T>[]>();
  readonly value = model<T | null>(null);
  /** Nom du groupe pour les lecteurs d'écran (« Contexte »). */
  readonly label = input('');
  readonly appearance = input<'chips' | 'segmented'>('chips');
  /** Colonnes de largeur égale sur toute la largeur disponible. */
  readonly fill = input(false, { transform: booleanAttribute });
  /** Grille de N colonnes égales, sur plusieurs lignes (les 7 pastilles du contexte : 4). */
  readonly columns = input<number | null>(null);
  readonly disabled = input(false, { transform: booleanAttribute });

  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;

  protected onKeydown(event: KeyboardEvent): void {
    const step = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
    if (!step) return;
    const buttons = [...this.host.querySelectorAll<HTMLButtonElement>('.g-toggle:not(:disabled)')];
    const index = buttons.indexOf(event.target as HTMLButtonElement);
    if (index < 0) return;
    event.preventDefault();
    buttons[(index + step + buttons.length) % buttons.length].focus();
  }
}
