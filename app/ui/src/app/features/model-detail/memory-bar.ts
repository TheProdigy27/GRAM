import { Component, computed, input } from '@angular/core';
import type { Estimate } from '../../core/generated/Estimate';
import type { MemoryBudget } from '../../core/generated/MemoryBudget';
import { gb } from '../../shared/format';
import { memoryBar } from '../../shared/memory-bar';
import { Tooltip } from '../../shared/ui/tooltip/tooltip';

/**
 * Barre mémoire empilée [poids | KV | vision | surcoût] sur une échelle qui contient la VRAM :
 * repère à la VRAM utilisable, part en RAM hachurée au-delà, dépassement d'un modèle trop gros ;
 * légende chiffrée, total et placement.
 */
@Component({
  selector: 'app-memory-bar',
  imports: [Tooltip],
  template: `
    @let m = model();
    <div class="bar" role="img" [class.bar--over]="m.overflow" [attr.aria-label]="ariaLabel()">
      <div class="bar__fill">
        @for (s of m.segments; track s.key) {
          <span
            class="segment"
            [attr.data-key]="s.key"
            [style.width.%]="s.pct"
            [gTooltip]="s.label + ' : ' + gb(s.bytes)"
          ></span>
        }
      </div>
      @if (m.ram; as r) {
        <span
          class="hatch hatch--ram"
          data-testid="memory-ram"
          [style.left.%]="r.start"
          [style.width.%]="r.width"
          [gTooltip]="'En RAM : ' + gb(m.ramBytes)"
        ></span>
      }
      @if (m.over; as o) {
        <span
          class="hatch hatch--over"
          [style.left.%]="o.start"
          [style.width.%]="o.width"
          [gTooltip]="'Au-delà de la mémoire utilisable : ' + gb(m.total - m.capacity)"
        ></span>
      }
      @for (mk of m.markers; track mk.label) {
        <span class="marker" [style.left.%]="mk.pct"></span>
      }
    </div>
    <div class="marks" aria-hidden="true" [class.marks--two]="m.markers.length > 1">
      @for (mk of m.markers; track mk.label; let last = $last) {
        <span
          class="mark"
          [class.mark--end]="mk.pct > 60"
          [class.mark--low]="!last && m.markers.length > 1"
          [style.left.%]="mk.pct"
          >{{ mk.label }} <strong class="mono">{{ gb(mk.bytes) }}</strong></span
        >
      }
    </div>
    <ul class="legend">
      @for (s of m.segments; track s.key) {
        <li>
          <span class="swatch" [attr.data-key]="s.key" aria-hidden="true"></span>{{ s.label }}
          <strong class="mono">{{ gb(s.bytes) }}</strong>
        </li>
      }
      @if (m.ram) {
        <li>
          <span class="swatch swatch--ram" aria-hidden="true"></span>En RAM
          <strong class="mono">{{ gb(m.ramBytes) }}</strong>
        </li>
      }
      <li class="legend__total">
        Total <strong class="mono" data-testid="memory-total">{{ gb(m.total) }}</strong>
      </li>
    </ul>
    <p class="placement" data-testid="placement">{{ placement() }}</p>
  `,
  styleUrl: './memory-bar.scss',
})
export class MemoryBar {
  readonly estimate = input.required<Estimate>();
  readonly budget = input.required<MemoryBudget>();

  protected readonly gb = gb;
  protected readonly model = computed(() => memoryBar(this.estimate(), this.budget()));

  /** Où va la mémoire : VRAM et RAM occupées face à ce qui est utilisable. */
  protected readonly placement = computed(() => {
    const e = this.estimate();
    const b = this.budget();
    const vram = `${b.unified ? 'GPU' : 'VRAM'} ${gb(e.memGpu)} / ${gb(b.vram)}`;
    const ram = `RAM ${gb(e.memCpu)} / ${gb(b.ram)}`;
    switch (e.mode.kind) {
      case 'gpu':
        return vram;
      case 'hybrid':
        return `${vram} · ${ram}`;
      case 'cpu':
        return ram;
      case 'tooBig':
        return `Besoin ${gb(this.model().total)} pour ${gb(this.model().capacity)} utilisables`;
    }
  });

  protected readonly ariaLabel = computed(() => {
    const m = this.model();
    const parts = m.segments.map((s) => `${s.label} ${gb(s.bytes)}`).join(', ');
    const ram = m.ram ? ` Dont ${gb(m.ramBytes)} en RAM.` : '';
    return `Mémoire nécessaire ${gb(m.total)} (${parts}) pour ${gb(m.capacity)} utilisables.${ram}`;
  });
}
