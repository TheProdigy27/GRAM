import { Component, booleanAttribute, computed, input } from '@angular/core';
import type { Score } from '../../core/generated/Score';
import { decimal } from '../../shared/format';
import { scoreAxes, scoreFactors, scoreFormula, scoreTooltip } from '../../shared/score';
import { Tooltip } from '../../shared/ui/tooltip/tooltip';

/** Hauteur maximale d'une mini-barre, en pixels. */
const SPARK_HEIGHT = 18;

/**
 * Score d'une ligne : quatre mini-barres verticales (qualité, vitesse, marge, contexte) puis le
 * total (`compact` : le total seul). L'infobulle détaille les axes en barres, leurs poids, la
 * popularité et les facteurs.
 */
@Component({
  selector: 'app-score-cell',
  imports: [Tooltip],
  template: `
    <span class="score" gTooltipPosition="left" [gTooltip]="text()" [gTooltipTemplate]="detail">
      @if (!compact()) {
        <span class="spark" aria-hidden="true">
          @for (a of axes(); track a.key) {
            <span class="spark__bar" [style.height.px]="sparkHeight(a.value)"></span>
          }
        </span>
      }
      <span class="score__total mono" data-testid="score">{{ total() }}</span>
    </span>

    <ng-template #detail>
      <div class="detail">
        <p class="detail__head">
          Score <strong class="mono">{{ total() }}</strong>
          <span class="detail__muted"> sur 100</span>
        </p>
        <div class="axes">
          @for (a of axes(); track a.key) {
            <span class="axis__label">{{ a.label }}</span>
            <span class="axis__track">
              <span class="axis__fill" [attr.data-axis]="a.key" [style.width.%]="a.value"></span>
            </span>
            <span class="axis__value mono">{{ round(a.value) }}</span>
            <span class="axis__weight mono">× {{ weight(a.weight) }}</span>
          }
        </div>
        <p class="detail__line">{{ formula() }}</p>
        @for (f of factors(); track f) {
          <p class="detail__line detail__line--factor">{{ f }}</p>
        }
      </div>
    </ng-template>
  `,
  styles: `
    :host {
      display: flex;
      justify-content: flex-end;
    }
    .score {
      display: inline-flex;
      align-items: center;
      gap: 10px;
      cursor: default;
    }
    .spark {
      display: inline-flex;
      align-items: flex-end;
      gap: 2px;
      height: 18px;
    }
    .spark__bar {
      width: 4px;
      border-radius: 1px;
      background: var(--g-spark);
    }
    .score__total {
      min-width: 24px;
      font-size: 15px;
      font-weight: 600;
      text-align: right;
      color: var(--g-fg);
    }

    .detail {
      display: flex;
      flex-direction: column;
      gap: 8px;
      min-width: 260px;
      font-size: 12px;
    }
    .detail p {
      margin: 0;
    }
    .detail__head strong {
      font-size: 15px;
    }
    .detail__muted,
    .detail__line {
      color: var(--g-muted);
    }
    .detail__line--factor {
      color: var(--g-warn);
    }
    .axes {
      display: grid;
      grid-template-columns: auto minmax(80px, 1fr) auto auto;
      align-items: center;
      gap: 6px 10px;
    }
    .axis__label {
      color: var(--g-fg-2);
      white-space: nowrap;
    }
    .axis__track {
      position: relative;
      height: 6px;
      border-radius: 3px;
      background: var(--g-track);
      overflow: hidden;
    }
    .axis__fill {
      position: absolute;
      inset: 0 auto 0 0;
      border-radius: 3px;
    }
    .axis__fill[data-axis='quality'] {
      background: var(--g-axis-quality);
    }
    .axis__fill[data-axis='speed'] {
      background: var(--g-axis-speed);
    }
    .axis__fill[data-axis='fit'] {
      background: var(--g-axis-fit);
    }
    .axis__fill[data-axis='context'] {
      background: var(--g-axis-context);
    }
    .axis__value {
      min-width: 24px;
      text-align: right;
      font-weight: 600;
    }
    .axis__weight {
      color: var(--g-muted);
    }
  `,
})
export class ScoreCell {
  readonly score = input.required<Score>();
  /** Le total seul, sans les mini-barres (classement resserré à côté de la fiche). */
  readonly compact = input(false, { transform: booleanAttribute });

  protected readonly total = computed(() => Math.round(this.score().total));
  protected readonly axes = computed(() => scoreAxes(this.score()));
  protected readonly text = computed(() => scoreTooltip(this.score()));
  protected readonly formula = computed(() => scoreFormula(this.score()));
  protected readonly factors = computed(() => scoreFactors(this.score()));

  protected sparkHeight(value: number): number {
    return Math.max(2, Math.round((Math.min(Math.max(value, 0), 100) / 100) * SPARK_HEIGHT));
  }

  protected round(value: number): number {
    return Math.round(value);
  }

  protected weight(w: number): string {
    return decimal(w);
  }
}
