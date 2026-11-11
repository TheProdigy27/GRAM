import { Component, ElementRef, inject, input, model } from '@angular/core';

export interface TabItem {
  value: string;
  label: string;
  /** Compteur affiché à côté du libellé. */
  count?: number | string;
  disabled?: boolean;
}

let nextId = 0;

/**
 * Onglets : `role="tablist"`, tabindex itinérant, ← → (et Début / Fin) changent d'onglet.
 * Le panneau reste chez l'appelant, relié par `tabId()` / `panelId()` :
 *
 * ```html
 * <g-tabs #tabs label="Sections" [tabs]="sections" [(selected)]="section" />
 * <div role="tabpanel" [id]="tabs.panelId(section())" [attr.aria-labelledby]="tabs.tabId(section())">
 * ```
 */
@Component({
  selector: 'g-tabs',
  template: `
    @for (t of tabs(); track t.value) {
      <button
        type="button"
        role="tab"
        class="g-tab"
        [id]="tabId(t.value)"
        [attr.aria-selected]="t.value === selected()"
        [attr.aria-controls]="panelId(t.value)"
        [attr.data-value]="t.value"
        [tabIndex]="t.value === selected() ? 0 : -1"
        [disabled]="t.disabled"
        (click)="select(t.value)"
        (keydown)="onKeydown($event)"
      >
        {{ t.label }}
        @if (t.count !== undefined && t.count !== null) {
          <span class="g-tab__count">{{ formatCount(t.count) }}</span>
        }
      </button>
    }
  `,
  host: {
    class: 'g-tabs',
    role: 'tablist',
    '[attr.aria-label]': 'label() || null',
  },
  styles: `
    :host {
      display: inline-flex;
      flex-wrap: wrap;
      gap: 4px;
      padding: 3px;
      border-radius: 10px;
      background: var(--g-seg);
    }
    .g-tab {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      height: 34px;
      padding: 0 14px;
      border: 0;
      border-radius: var(--g-radius);
      background: transparent;
      color: var(--g-muted);
      font: inherit;
      font-size: 13.5px;
      font-weight: 600;
      white-space: nowrap;
      cursor: pointer;
    }
    .g-tab:hover:not(:disabled) {
      color: var(--g-fg);
    }
    .g-tab[aria-selected='true'] {
      background: var(--g-surface);
      color: var(--g-fg);
      box-shadow: var(--g-shadow);
    }
    .g-tab:focus-visible {
      outline: 2px solid var(--g-accent);
      outline-offset: 1px;
    }
    .g-tab:disabled {
      color: var(--g-disabled-fg);
      cursor: not-allowed;
    }
    .g-tab__count {
      display: inline-grid;
      place-items: center;
      min-width: 22px;
      height: 20px;
      padding: 0 6px;
      border-radius: 10px;
      background: var(--g-track);
      color: var(--g-muted);
      font-size: 11.5px;
      font-weight: 600;
      font-variant-numeric: tabular-nums;
    }
  `,
})
export class Tabs {
  readonly tabs = input.required<readonly TabItem[]>();
  readonly selected = model<string>('');
  readonly label = input('');
  /** Préfixe des identifiants des onglets et des panneaux. */
  readonly idPrefix = input(`g-tabs-${nextId++}`);

  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;

  tabId(value: string): string {
    return `${this.idPrefix()}-tab-${value}`;
  }

  panelId(value: string): string {
    return `${this.idPrefix()}-panel-${value}`;
  }

  protected select(value: string): void {
    this.selected.set(value);
  }

  protected formatCount(count: number | string): string {
    return typeof count === 'number' ? count.toLocaleString('fr-FR') : count;
  }

  protected onKeydown(event: KeyboardEvent): void {
    const enabled = this.tabs().filter((t) => !t.disabled);
    const index = enabled.findIndex((t) => t.value === this.selected());
    let next: number;
    switch (event.key) {
      case 'ArrowRight':
        next = (index + 1) % enabled.length;
        break;
      case 'ArrowLeft':
        next = (index - 1 + enabled.length) % enabled.length;
        break;
      case 'Home':
        next = 0;
        break;
      case 'End':
        next = enabled.length - 1;
        break;
      default:
        return;
    }
    event.preventDefault();
    const value = enabled[next].value;
    this.select(value);
    this.host.querySelector<HTMLElement>(`[data-value="${CSS.escape(value)}"]`)?.focus();
  }
}
