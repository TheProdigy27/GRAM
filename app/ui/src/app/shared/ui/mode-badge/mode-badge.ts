import { Component, computed, input } from '@angular/core';
import type { RunMode } from '../../../core/generated/RunMode';
import { MODE_LABELS } from '../../labels';
import { Icon } from '../icon/icon';
import { Tooltip } from '../tooltip/tooltip';

/**
 * Badge du mode d'exécution : repère coloré (GPU, hybride, CPU) et libellé, couches en VRAM en
 * hybride (« Hybride 62/65 ») ; « Trop gros » en rouge avec une croix.
 */
@Component({
  selector: 'app-mode-badge, g-mode-badge',
  imports: [Icon, Tooltip],
  template: `<span class="mode-badge" [class]="'mode-' + mode().kind" [gTooltip]="hint()">
    @if (mode().kind === 'tooBig') {
      <g-icon name="close" [size]="13" />
    } @else {
      <span class="mode-badge__mark" aria-hidden="true"></span>
    }
    {{ label() }}
    @if (layers(); as l) {
      <span class="mode-badge__layers">{{ l }}</span>
    }
  </span>`,
  styles: `
    .mode-badge {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      height: 24px;
      padding: 0 8px;
      border-radius: var(--g-radius-badge);
      background: var(--g-track);
      color: var(--g-fg-2);
      font-size: 12px;
      font-weight: 500;
      line-height: 1;
      white-space: nowrap;
    }
    .mode-badge__mark {
      width: 8px;
      height: 8px;
      border-radius: 2px;
      background: currentColor;
    }
    .mode-gpu .mode-badge__mark {
      background: var(--g-gpu);
    }
    .mode-hybrid .mode-badge__mark {
      background: var(--g-hybrid);
    }
    .mode-cpu .mode-badge__mark {
      background: var(--g-cpu);
    }
    .mode-tooBig {
      background: var(--g-error-bg);
      color: var(--g-error);
    }
    .mode-badge__layers {
      font-family: var(--g-font-mono);
      font-size: 11px;
      font-variant-numeric: tabular-nums;
      color: var(--g-muted);
    }
  `,
})
export class ModeBadge {
  readonly mode = input.required<RunMode>();
  /** Infobulle (par défaut : répartition des couches en mode hybride). */
  readonly tooltip = input<string>('');

  protected readonly label = computed(() => MODE_LABELS[this.mode().kind]);

  protected readonly layers = computed(() => {
    const m = this.mode();
    return m.kind === 'hybrid' ? `${m.gpuLayers}/${m.layers}` : '';
  });

  protected readonly hint = computed(() => {
    const m = this.mode();
    if (this.tooltip()) return this.tooltip();
    return m.kind === 'hybrid' ? `${m.gpuLayers} couches sur ${m.layers} en VRAM` : '';
  });
}
