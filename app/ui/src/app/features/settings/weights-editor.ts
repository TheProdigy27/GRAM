import { Component, computed, input, output } from '@angular/core';
import type { ScoreWeights } from '../../core/generated/ScoreWeights';
import { decimal, parseNumber, sameNumber } from '../../shared/format';
import { SCORE_AXES } from '../../shared/score';
import { FIELD } from '../../shared/ui/field/field';
import { Tag } from '../../shared/ui/tag/tag';
import { WEIGHT_LABELS, sameValue, weightSegments, weightsLabel } from './settings-model';

/** Fin de la course des curseurs (les poids peuvent dépasser 1 dans le champ). */
const SLIDER_MAX = 1;

/** Position sur un curseur de 16 px de pouce : le centre du pouce suit la valeur. */
function thumbPosition(value: number): string {
  const f = Math.min(1, Math.max(0, value / SLIDER_MAX));
  return `calc(8px + (100% - 16px) * ${f})`;
}

export interface WeightChange {
  key: keyof ScoreWeights;
  value: number;
}

/**
 * Poids des quatre axes du score : barre de répartition en % (couleurs des axes, étiquettes dans
 * les segments quand elles tiennent), puis un curseur par axe avec le repère de la valeur par
 * défaut et un champ numérique, « défaut X » et « modifié · rétablir ».
 */
@Component({
  selector: 'app-weights-editor',
  imports: [FIELD, Tag],
  template: `
    <div class="bar" role="img" data-testid="weights-bar" [attr.aria-label]="label()">
      @for (s of segments(); track s.key) {
        @if (s.width > 0) {
          <span class="seg" [attr.data-axis]="s.key" [style.flex-grow]="s.width">{{ s.text }}</span>
        }
      }
    </div>
    @for (w of lines(); track w.key) {
      <div class="line" data-testid="weight" [attr.data-weight]="w.key">
        <span class="name"><span class="swatch" [attr.data-axis]="w.key"></span>{{ w.label }}</span>
        <span class="slider">
          <input
            type="range"
            min="0"
            [max]="max"
            step="0.01"
            data-testid="weight-range"
            [value]="w.value"
            [style.--fill]="w.position"
            [attr.aria-label]="'Poids ' + w.label"
            [attr.aria-valuetext]="w.text"
            (input)="change(w.key, $any($event.target).value)"
          />
          @if (w.defaultPosition) {
            <span class="tick" aria-hidden="true" [style.left]="w.defaultPosition"></span>
          }
        </span>
        <g-field class="value">
          <input
            gInput
            inputmode="decimal"
            autocomplete="off"
            data-testid="weight-value"
            [attr.aria-label]="'Poids ' + w.label + ', valeur'"
            [value]="w.text"
            [sameValue]="sameNumber"
            (input)="change(w.key, $any($event.target).value)"
          />
        </g-field>
        <span class="meta">
          @if (w.modified) {
            <g-tag tone="warn" size="sm">modifié</g-tag>
          }
          @if (w.defaultText) {
            <span>{{ w.defaultText }}</span>
          }
          @if (w.modified && w.default !== null) {
            <button
              type="button"
              class="reset"
              data-testid="weight-reset"
              [attr.aria-label]="'Rétablir le poids ' + w.label"
              (click)="weightChange.emit({ key: w.key, value: w.default })"
            >
              rétablir
            </button>
          }
        </span>
      </div>
    }
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      gap: 12px;
      max-width: 640px;
    }
    .bar {
      display: flex;
      gap: 2px;
      height: 26px;
      border-radius: 6px;
      overflow: hidden;
      font-size: 11.5px;
      font-weight: 600;
    }
    .seg {
      display: flex;
      align-items: center;
      flex-basis: 0;
      min-width: 0;
      padding-left: 8px;
      white-space: nowrap;
      overflow: hidden;
    }
    [data-axis='quality'] {
      background: var(--g-axis-quality);
      color: var(--g-axis-quality-fg);
    }
    [data-axis='speed'] {
      background: var(--g-axis-speed);
      color: var(--g-axis-speed-fg);
    }
    [data-axis='fit'] {
      background: var(--g-axis-fit);
      color: var(--g-axis-fit-fg);
    }
    [data-axis='context'] {
      background: var(--g-axis-context);
      color: var(--g-axis-context-fg);
    }
    .line {
      display: grid;
      grid-template-columns: 96px minmax(120px, 1fr) 72px 196px;
      align-items: center;
      gap: 12px;
    }
    .name {
      display: flex;
      align-items: center;
      gap: 8px;
      font-weight: 500;
    }
    .swatch {
      flex: none;
      width: 10px;
      height: 10px;
      border-radius: 2px;
    }
    .slider {
      position: relative;
      display: block;
    }
    input[type='range'] {
      -webkit-appearance: none;
      appearance: none;
      display: block;
      width: 100%;
      height: 24px;
      margin: 0;
      background: transparent;
      cursor: pointer;
    }
    input[type='range']::-webkit-slider-runnable-track {
      height: 4px;
      border-radius: 2px;
      background: linear-gradient(
        to right,
        var(--g-accent) 0 var(--fill),
        var(--g-input) var(--fill) 100%
      );
    }
    input[type='range']::-webkit-slider-thumb {
      -webkit-appearance: none;
      width: 16px;
      height: 16px;
      margin-top: -6px;
      border-radius: 50%;
      border: 2px solid var(--g-surface);
      background: var(--g-accent);
      box-shadow: 0 0 0 1px var(--g-control);
    }
    input[type='range']:focus-visible {
      outline: none;
    }
    input[type='range']:focus-visible::-webkit-slider-thumb {
      outline: 2px solid var(--g-accent);
      outline-offset: 2px;
    }
    .tick {
      position: absolute;
      top: 4px;
      width: 2px;
      height: 16px;
      margin-left: -1px;
      border-radius: 1px;
      background: var(--g-control);
      pointer-events: none;
    }
    .value .g-input {
      font-family: var(--g-font-mono);
      font-size: 12.5px;
      text-align: right;
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
  `,
})
export class WeightsEditor {
  readonly weights = input.required<ScoreWeights>();
  readonly defaults = input<ScoreWeights | null>(null);
  readonly weightChange = output<WeightChange>();

  protected readonly max = SLIDER_MAX;
  protected readonly sameNumber = sameNumber;
  protected readonly segments = computed(() => weightSegments(this.weights()));
  protected readonly label = computed(() => weightsLabel(this.segments()));

  protected readonly lines = computed(() => {
    const w = this.weights();
    const d = this.defaults();
    return SCORE_AXES.map(({ key }) => {
      const def = d ? d[key] : null;
      return {
        key,
        label: WEIGHT_LABELS[key],
        value: w[key],
        text: decimal(w[key]),
        position: thumbPosition(w[key]),
        default: def,
        defaultPosition: def === null ? null : thumbPosition(def),
        defaultText: def === null ? '' : `défaut ${decimal(def)}`,
        modified: def !== null && !sameValue(w[key], def),
      };
    });
  });

  /** Valeur du curseur ou du champ ; un nombre négatif ou illisible est ignoré. */
  protected change(key: keyof ScoreWeights, raw: string): void {
    const x = parseNumber(raw);
    if (x !== null && x >= 0) this.weightChange.emit({ key, value: x });
  }
}
