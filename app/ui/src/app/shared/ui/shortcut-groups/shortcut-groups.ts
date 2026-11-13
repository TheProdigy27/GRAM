import { Component, input } from '@angular/core';

/** Une ligne d'une aide des raccourcis : touches (reliées par `joiner`, « à »…) et effet. */
export interface ShortcutRow {
  keys: readonly string[];
  joiner?: string;
  label: string;
}

export interface ShortcutGroup {
  title: string;
  rows: readonly ShortcutRow[];
}

/**
 * Table des raccourcis d'une application, par groupes (dialogue « Raccourcis clavier » de GRAM
 * et de GRAM Harness).
 */
@Component({
  selector: 'g-shortcut-groups',
  template: `
    @for (g of groups(); track g.title) {
      <section class="group" [attr.aria-labelledby]="'shortcuts-' + $index">
        <h3 class="group__title" [id]="'shortcuts-' + $index">{{ g.title }}</h3>
        <dl class="rows">
          @for (r of g.rows; track r.label) {
            <div class="row" data-testid="shortcut-row">
              <dt class="keys">
                @for (k of r.keys; track $index; let last = $last) {
                  <kbd class="g-kbd">{{ k }}</kbd>
                  @if (r.joiner && !last) {
                    <span class="joiner"> {{ r.joiner }} </span>
                  }
                }
              </dt>
              <dd class="label">{{ r.label }}</dd>
            </div>
          }
        </dl>
      </section>
    }
  `,
  styles: `
    :host {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
      gap: 20px 28px;
    }
    .group__title {
      margin: 0 0 6px;
      font-size: 11px;
      font-weight: 600;
      letter-spacing: 0.06em;
      text-transform: uppercase;
      color: var(--g-muted);
    }
    .rows {
      margin: 0;
    }
    .row {
      display: flex;
      align-items: baseline;
      gap: 12px;
      padding: 6px 0;
      border-bottom: 1px solid var(--g-line-2);
    }
    .keys {
      flex: none;
      display: flex;
      align-items: center;
      gap: 4px;
      width: 96px;
    }
    .joiner {
      font-size: 12px;
      color: var(--g-muted);
    }
    .label {
      margin: 0;
      color: var(--g-fg-2);
    }
  `,
})
export class ShortcutGroups {
  readonly groups = input.required<readonly ShortcutGroup[]>();
}
