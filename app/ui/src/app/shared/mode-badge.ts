import { Component, computed, input } from '@angular/core';
import { MatTooltipModule } from '@angular/material/tooltip';
import type { RunMode } from '../core/generated/RunMode';
import { MODE_LABELS } from './labels';

/** Badge coloré du mode d'exécution : GPU vert, Hybride orange, CPU gris, Trop gros rouge. */
@Component({
  selector: 'app-mode-badge',
  imports: [MatTooltipModule],
  template: `<span
    class="mode-badge"
    [class]="'mode-' + mode().kind"
    [matTooltip]="hint()"
    [matTooltipDisabled]="!hint()"
    >{{ label() }}</span
  >`,
})
export class ModeBadge {
  readonly mode = input.required<RunMode>();
  /** Infobulle (par défaut : répartition des couches en mode hybride). */
  readonly tooltip = input<string>('');

  protected readonly label = computed(() => MODE_LABELS[this.mode().kind]);

  protected readonly hint = computed(() => {
    const m = this.mode();
    if (this.tooltip()) return this.tooltip();
    return m.kind === 'hybrid' ? `${m.gpuLayers} couches sur ${m.layers} en VRAM` : '';
  });
}
