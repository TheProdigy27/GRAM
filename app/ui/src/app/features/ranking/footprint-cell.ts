import { Component, computed, input } from '@angular/core';
import type { MemoryBudget } from '../../core/generated/MemoryBudget';
import type { VariantEstimate } from '../../core/generated/VariantEstimate';
import { gb } from '../../shared/format';
import { Tooltip } from '../../shared/ui/tooltip/tooltip';
import { rowFootprint } from './footprint';

/**
 * Taille du fichier au-dessus d'une barre de 8 px sur l'échelle de la carte mémoire : zone VRAM
 * teintée, part en VRAM (`--g-gpu`) puis part en RAM (`--g-hybrid`) ; un modèle trop gros
 * déborde en rouge jusqu'au bout de l'échelle.
 */
@Component({
  selector: 'app-footprint-cell',
  imports: [Tooltip],
  template: `
    <span class="cell" [gTooltip]="fp().label">
      <span class="size mono">{{ size() }}</span>
      <span class="bar" aria-hidden="true" [class.bar--unified]="fp().unified">
        @if (fp().vramEnd > 0) {
          <span class="zone" [style.width.%]="fp().vramEnd * 100"></span>
        }
        @if (fp().gpuLimit; as limit) {
          <span class="limit" [style.left.%]="limit * 100"></span>
        }
        @if (fp().gpu; as s) {
          <span
            class="seg seg--gpu"
            [style.left.%]="s.start * 100"
            [style.width.%]="s.width * 100"
          ></span>
        }
        @if (fp().ram; as s) {
          <span
            class="seg seg--ram"
            [style.left.%]="s.start * 100"
            [style.width.%]="s.width * 100"
          ></span>
        }
        @if (fp().over; as s) {
          <span
            class="seg seg--over"
            [style.left.%]="s.start * 100"
            [style.width.%]="s.width * 100"
          ></span>
        }
      </span>
    </span>
    <span class="g-sr-only">, {{ fp().label }}</span>
  `,
  styles: `
    :host {
      display: block;
      min-width: 0;
    }
    .cell {
      display: flex;
      flex-direction: column;
      gap: 5px;
    }
    .size {
      font-size: 12px;
      color: var(--g-fg);
    }
    .bar {
      position: relative;
      height: 8px;
      border-radius: 3px;
      background: var(--g-track);
    }
    .zone {
      position: absolute;
      inset: 0 auto 0 0;
      border-radius: 3px 0 0 3px;
      background: var(--g-vram-zone);
    }
    .bar--unified .zone {
      border-radius: 3px;
    }
    .limit {
      position: absolute;
      top: -2px;
      bottom: -2px;
      width: 1px;
      background: var(--g-vram-fg);
      opacity: 0.6;
    }
    .seg {
      position: absolute;
      top: 0;
      bottom: 0;
      min-width: 2px;
      border-radius: 3px;
    }
    .seg--gpu {
      background: var(--g-gpu);
    }
    .seg--ram {
      background: var(--g-hybrid);
    }
    .seg--over {
      background: repeating-linear-gradient(
        135deg,
        var(--g-error) 0 3px,
        var(--g-error-bg) 3px 5px
      );
    }
  `,
})
export class FootprintCell {
  readonly best = input.required<VariantEstimate>();
  readonly budget = input.required<MemoryBudget>();

  protected readonly size = computed(() => gb(this.best().variant.sizeBytes));
  protected readonly fp = computed(() => rowFootprint(this.budget(), this.best().estimate));
}
