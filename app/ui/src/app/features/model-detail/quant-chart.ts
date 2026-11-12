import { Component, ElementRef, computed, inject, input, output } from '@angular/core';
import type { MemoryBudget } from '../../core/generated/MemoryBudget';
import type { RunMode } from '../../core/generated/RunMode';
import type { VariantDetail } from '../../core/generated/VariantDetail';
import { niceAxis } from '../../shared/chart';
import { ctxK, gb, int, tokS } from '../../shared/format';
import { MODE_LABELS } from '../../shared/labels';
import { Tooltip } from '../../shared/ui/tooltip/tooltip';

type ModeKind = RunMode['kind'];

/** Libellés de la légende, dans l'ordre des modes. */
export const MODE_LEGEND: Record<ModeKind, string> = {
  gpu: 'GPU, tient en VRAM',
  hybrid: 'Hybride, déborde en RAM',
  cpu: 'CPU',
  tooBig: 'Trop gros',
};

const MODE_ORDER: readonly ModeKind[] = ['gpu', 'hybrid', 'cpu', 'tooBig'];

/** Au-delà, seules quelques quantifications ont leur nom sous l'axe. */
const ALL_LABELS_UP_TO = 7;
const AXIS_LABELS = 5;

export interface ChartBar {
  label: string;
  mode: ModeKind;
  /** Hauteur, en % de la zone de tracé. */
  height: number;
  /** « ~95 tok/s », « — ». */
  speed: string;
  /** Au-dessus de la taille maximale demandée. */
  dimmed: boolean;
  best: boolean;
  /** Nom sous l'axe (★ devant la meilleure) ; vide pour la plupart des barres. */
  axisLabel: string;
  /** Alignement des textes centrés sur la barre, près des bords. */
  align: 'start' | 'center' | 'end';
  /** Infobulle et description accessible. */
  tooltip: string;
}

export interface QuantChartModel {
  bars: ChartBar[];
  /** Graduations de l'axe des vitesses, en % de la hauteur. */
  ticks: { pct: number; label: string }[];
  /** Repère « Limite VRAM » entre la dernière quantification GPU et la suivante. */
  limit: { pct: number; label: string; side: 'right' | 'left' } | null;
  /** Modes présents, pour la légende. */
  modes: ModeKind[];
  /** Résumé pour les lecteurs d'écran. */
  summary: string;
}

/**
 * Noms affichés sous l'axe : tous jusqu'à 7 quantifications, sinon 5 répartis régulièrement,
 * dont la meilleure à la place du plus proche.
 */
export function axisIndices(count: number, best: number | null): Set<number> {
  if (count <= ALL_LABELS_UP_TO) return new Set(Array.from({ length: count }, (_, i) => i));
  const picks = Array.from({ length: AXIS_LABELS }, (_, k) =>
    Math.round((k * (count - 1)) / (AXIS_LABELS - 1)),
  );
  if (best !== null && best >= 0 && best < count && !picks.includes(best)) {
    let nearest = 0;
    for (let k = 1; k < picks.length; k++) {
      if (Math.abs(picks[k] - best) < Math.abs(picks[nearest] - best)) nearest = k;
    }
    picks[nearest] = best;
  }
  return new Set(picks);
}

function align(index: number, count: number): ChartBar['align'] {
  if (count <= 2) return 'center';
  const at = (index + 0.5) / count;
  return at < 0.1 ? 'start' : at > 0.9 ? 'end' : 'center';
}

function modeText(mode: RunMode): string {
  return mode.kind === 'hybrid'
    ? `${MODE_LABELS.hybrid} (${mode.gpuLayers}/${mode.layers} couches en VRAM)`
    : mode.kind === 'tooBig'
      ? 'Ne tient pas'
      : MODE_LABELS[mode.kind];
}

/**
 * Vitesse estimée de chaque quantification (de la plus petite à la plus grosse) : barres de
 * hauteur proportionnelle aux tok/s, axe rond, repère au passage de GPU à hybride.
 */
export function quantChart(
  variants: readonly VariantDetail[],
  best: number | null,
  budget: MemoryBudget,
): QuantChartModel {
  const n = variants.length;
  const axis = niceAxis(Math.max(0, ...variants.map((v) => v.estimate.tokS)));
  const labelled = axisIndices(n, best);

  const bars = variants.map((v, i): ChartBar => {
    const e = v.estimate;
    const isBest = i === best;
    const tooltip = [
      `${v.variant.label}${isBest ? ' · ★ meilleure pour cette machine' : ''}`,
      `${gb(v.variant.sizeBytes)} · ${modeText(e.mode)}`,
      `${tokS(e.tokS)} tok/s · contexte max ${ctxK(e.ctxMax)} · score ${Math.round(v.score.total)}`,
      ...(v.withinSize ? [] : ['Au-dessus de la taille maximale demandée']),
    ].join('\n');
    return {
      label: v.variant.label,
      mode: e.mode.kind,
      height: axis.max > 0 ? (Math.max(e.tokS, 0) / axis.max) * 100 : 0,
      speed: tokS(e.tokS),
      dimmed: !v.withinSize,
      best: isBest,
      axisLabel: labelled.has(i) ? `${isBest ? '★ ' : ''}${v.variant.label}` : '',
      align: align(i, n),
      tooltip,
    };
  });

  const cut = variants.findIndex(
    (v, i) =>
      i > 0 && v.estimate.mode.kind !== 'gpu' && variants[i - 1].estimate.mode.kind === 'gpu',
  );
  const limitPct = (cut / n) * 100;
  const limit =
    cut > 0
      ? {
          pct: limitPct,
          label: `${budget.unified ? 'Limite du GPU' : 'Limite VRAM'} · ${gb(budget.vram)}`,
          side: limitPct > 62 ? ('left' as const) : ('right' as const),
        }
      : null;

  const present = new Set(variants.map((v) => v.estimate.mode.kind));
  const range = n > 0 ? `de ${variants[0].variant.label} à ${variants[n - 1].variant.label}` : '';
  const overflow =
    cut > 0
      ? ` Au-delà de ${variants[cut - 1].variant.label}, le modèle déborde de la VRAM : ` +
        `${tokS(variants[cut - 1].estimate.tokS)} puis ${tokS(variants[cut].estimate.tokS)} tok/s.`
      : '';

  return {
    bars,
    ticks: axis.ticks.map((t) => ({ pct: (t / axis.max) * 100, label: int(t) })),
    limit,
    modes: MODE_ORDER.filter((m) => present.has(m)),
    summary: `Vitesse estimée en tokens par seconde pour ${n} quantifications, ${range}.${overflow}`,
  };
}

/**
 * Graphique « Vitesse estimée selon la quantification » : une barre par quantification (couleur
 * du mode), la choisie cerclée avec son nom et sa vitesse au-dessus, repère de la limite VRAM.
 * Les barres forment une liste à choix unique : clic, ou ← → Début Fin au clavier, choisit la
 * quantification ; l'infobulle de chaque barre donne taille, mode, tok/s, contexte et score.
 */
@Component({
  selector: 'app-quant-chart',
  imports: [Tooltip],
  template: `
    @let m = model();
    <ul class="legend" aria-label="Légende">
      @for (mode of m.modes; track mode) {
        <li class="legend__item" [attr.data-mode]="mode">
          <span class="legend__swatch" aria-hidden="true"></span>{{ legend[mode] }}
        </li>
      }
    </ul>
    <figure class="chart">
      <div class="y-axis" aria-hidden="true">
        @for (t of m.ticks; track t.pct) {
          <span class="y-axis__tick mono" [style.bottom.%]="t.pct">{{ t.label }}</span>
        }
      </div>
      <div class="plot-area">
        <div class="plot">
          @for (t of m.ticks; track t.pct) {
            <span class="grid-line" aria-hidden="true" [style.bottom.%]="t.pct"></span>
          }
          @if (m.limit; as l) {
            <span
              class="limit"
              data-testid="vram-limit"
              aria-hidden="true"
              [class.limit--left]="l.side === 'left'"
              [style.left.%]="l.pct"
              ><span class="limit__label">{{ l.label }}</span></span
            >
          }
          <div
            class="bars"
            role="listbox"
            aria-label="Quantifications"
            aria-orientation="horizontal"
            [attr.aria-describedby]="summaryId"
            (keydown)="onKeydown($event)"
          >
            @for (b of m.bars; track b.label; let i = $index) {
              <div
                class="col"
                role="option"
                data-testid="quant-bar"
                gTooltipPosition="above"
                [class.col--selected]="i === selectedIndex()"
                [class.col--dimmed]="b.dimmed"
                [attr.data-mode]="b.mode"
                [attr.data-label]="b.label"
                [attr.aria-selected]="i === selectedIndex()"
                [attr.aria-label]="b.label"
                [tabIndex]="i === focusIndex() ? 0 : -1"
                [gTooltip]="b.tooltip"
                (click)="choose(i)"
              >
                @if (i === selectedIndex()) {
                  <span
                    class="col__tag"
                    aria-hidden="true"
                    [attr.data-align]="b.align"
                    [style.bottom]="'calc(' + b.height + '% + 8px)'"
                    ><span class="mono">{{ b.label }}</span
                    ><span class="col__speed"> {{ b.speed }} tok/s</span></span
                  >
                }
                <span class="bar" [style.height.%]="b.height"></span>
              </div>
            }
          </div>
        </div>
        <div class="x-axis" aria-hidden="true">
          @for (b of m.bars; track b.label) {
            <span class="x-axis__slot"
              ><span class="x-axis__label mono" [attr.data-align]="b.align">{{
                b.axisLabel
              }}</span></span
            >
          }
        </div>
        <div class="direction" aria-hidden="true">
          <span>← plus rapide, moins précis</span><span>plus fidèle, plus lent →</span>
        </div>
      </div>
      <p class="g-sr-only" [id]="summaryId">{{ m.summary }}</p>
    </figure>
  `,
  styleUrl: './quant-chart.scss',
})
export class QuantChart {
  readonly variants = input.required<readonly VariantDetail[]>();
  readonly best = input<number | null>(null);
  readonly budget = input.required<MemoryBudget>();
  readonly selected = input<string | null>(null);
  readonly selectedChange = output<string>();

  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
  protected readonly summaryId = `quant-chart-${nextId++}`;
  protected readonly legend = MODE_LEGEND;

  protected readonly model = computed(() =>
    quantChart(this.variants(), this.best(), this.budget()),
  );

  protected readonly selectedIndex = computed(() =>
    this.variants().findIndex((v) => v.variant.label === this.selected()),
  );

  /** Barre dans l'ordre de tabulation : la choisie, sinon la première. */
  protected readonly focusIndex = computed(() => Math.max(this.selectedIndex(), 0));

  protected choose(index: number): void {
    const v = this.variants()[index];
    if (v) this.selectedChange.emit(v.variant.label);
  }

  /** ← → Début Fin : la sélection suit le focus (liste à choix unique). */
  protected onKeydown(event: KeyboardEvent): void {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const last = this.variants().length - 1;
    const current = this.focusIndex();
    let next: number;
    switch (event.key) {
      case 'ArrowRight':
      case 'ArrowDown':
        next = Math.min(current + 1, last);
        break;
      case 'ArrowLeft':
      case 'ArrowUp':
        next = Math.max(current - 1, 0);
        break;
      case 'Home':
        next = 0;
        break;
      case 'End':
        next = last;
        break;
      default:
        return;
    }
    event.preventDefault();
    if (next < 0) return;
    this.choose(next);
    this.host.querySelectorAll<HTMLElement>('[role="option"]')[next]?.focus();
  }
}

let nextId = 0;
