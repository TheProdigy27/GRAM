import { Component, booleanAttribute, input, output } from '@angular/core';
import { Tag } from '../../shared/ui/tag/tag';

/** Repère d'un réglage : modifié (différent de la valeur par défaut) ou recalibré par mesure. */
export type RowMarker = 'modified' | 'calibrated' | null;

let nextId = 0;

/**
 * Ligne d'un réglage : titre et aide à gauche, contrôle projeté à droite, puis « défaut X »
 * toujours visible, et le repère « modifié » (ou « recalibrée ») avec « rétablir ».
 * `labelFor` relie le titre au champ ; sinon le titre porte l'identifiant `titleId`, à donner
 * en `aria-labelledby` au contrôle. `stacked` : contrôle sous le titre, sur toute la largeur.
 */
@Component({
  selector: 'app-setting-row',
  imports: [Tag],
  template: `
    <div class="text">
      @if (labelFor()) {
        <label class="title" [id]="titleId" [for]="labelFor()">{{ heading() }}</label>
      } @else {
        <span class="title" [id]="titleId">{{ heading() }}</span>
      }
      @if (help()) {
        <p class="help">{{ help() }}</p>
      }
    </div>
    <div class="control">
      <ng-content />
      @if (defaultText() || marker()) {
        <div class="meta" data-testid="setting-meta">
          @switch (marker()) {
            @case ('modified') {
              <g-tag tone="warn" size="sm" data-testid="modified">modifié</g-tag>
            }
            @case ('calibrated') {
              <g-tag tone="ok" size="sm" data-testid="calibrated">recalibrée</g-tag>
            }
          }
          @if (defaultText()) {
            <span class="default">{{ defaultText() }}</span>
          }
          @if (marker()) {
            <button
              type="button"
              class="reset"
              data-testid="row-reset"
              [attr.aria-label]="'Rétablir : ' + heading()"
              (click)="reset.emit()"
            >
              rétablir
            </button>
          }
        </div>
      }
    </div>
  `,
  host: {
    class: 'setting-row',
    '[class.setting-row--stacked]': 'stacked()',
    '[attr.data-row]': 'row()',
  },
  styles: `
    :host {
      display: grid;
      grid-template-columns: minmax(0, 220px) minmax(0, 1fr);
      gap: 8px 24px;
      align-items: start;
      padding: 16px 0;
      border-top: 1px solid var(--g-line-2);
    }
    :host(.setting-row--stacked) {
      grid-template-columns: minmax(0, 1fr);
    }
    .title {
      display: block;
      font-size: 13.5px;
      font-weight: 600;
      color: var(--g-fg);
    }
    .help {
      margin: 3px 0 0;
      font-size: 12px;
      line-height: 1.45;
      color: var(--g-muted);
    }
    .control {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: 8px 12px;
      min-width: 0;
    }
    .meta {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      font-size: 12px;
      color: var(--g-muted);
      white-space: nowrap;
    }
    .reset {
      min-height: 24px;
      padding: 0 2px;
      border: 0;
      background: none;
      color: var(--g-accent-ink);
      font: inherit;
      font-weight: 500;
      cursor: pointer;
    }
    .reset:hover {
      text-decoration: underline;
    }
    @media (max-width: 760px) {
      :host {
        grid-template-columns: minmax(0, 1fr);
      }
    }
  `,
})
export class SettingRow {
  readonly heading = input.required<string>();
  readonly help = input('');
  /** Identifiant du champ que le titre étiquette. */
  readonly labelFor = input<string | null>(null);
  /** « défaut 10 % ». */
  readonly defaultText = input('');
  readonly marker = input<RowMarker>(null);
  readonly stacked = input(false, { transform: booleanAttribute });
  /** Identifiant du réglage (`data-row`), pour les tests et la recherche. */
  readonly row = input<string | null>(null);
  readonly reset = output<void>();

  readonly titleId = `setting-${nextId++}-title`;
}
