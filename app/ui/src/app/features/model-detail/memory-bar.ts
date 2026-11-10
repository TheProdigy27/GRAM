import { Component, computed, input } from '@angular/core';
import { MatTooltipModule } from '@angular/material/tooltip';
import type { Estimate } from '../../core/generated/Estimate';
import type { MemoryBudget } from '../../core/generated/MemoryBudget';
import { gb } from '../../shared/format';
import { memoryBar } from '../../shared/memory-bar';

/** Barre mémoire empilée [poids | KV | vision | surcoût] face à la VRAM et à la RAM. */
@Component({
  selector: 'app-memory-bar',
  imports: [MatTooltipModule],
  template: `
    @let m = model();
    <div class="bar" role="img" [class.overflow]="m.overflow" [attr.aria-label]="ariaLabel()">
      @for (s of m.segments; track s.key) {
        <span
          class="segment"
          [class]="'seg-' + s.key"
          [style.width.%]="s.pct"
          [matTooltip]="s.label + ' : ' + gb(s.bytes)"
        ></span>
      }
      @for (mk of m.markers; track mk.label) {
        <span
          class="marker"
          [style.left.%]="mk.pct"
          [matTooltip]="mk.label + ' : ' + gb(mk.bytes)"
        ></span>
      }
    </div>
    <ul class="legend">
      @for (s of m.segments; track s.key) {
        <li>
          <span class="swatch" [class]="'seg-' + s.key"></span>{{ s.label }} {{ gb(s.bytes) }}
        </li>
      }
      <li class="total">Total {{ gb(m.total) }}</li>
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
    const vram = `VRAM ${gb(e.memGpu)} / ${gb(b.vram)}`;
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
    return `Mémoire nécessaire ${gb(m.total)} (${parts}) pour ${gb(m.capacity)} utilisables`;
  });
}
