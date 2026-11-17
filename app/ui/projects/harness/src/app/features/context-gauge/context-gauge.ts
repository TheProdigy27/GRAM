import { Component, computed, inject } from '@angular/core';
import { int } from '@gram/format';
import { Tooltip } from '@gram/ui/tooltip/tooltip';
import { ChatStore } from '../../state/chat.store';

/**
 * Jauge de l'en-tête : contexte occupé à la fin de la dernière réponse (« 3,2k / 16k »),
 * orange à partir de 80 % (un encadré au-dessus de la zone de saisie dit quoi faire).
 */
@Component({
  selector: 'app-context-gauge',
  imports: [Tooltip],
  template: `
    @if (store.context(); as c) {
      <div
        class="gauge"
        role="meter"
        tabindex="0"
        aria-label="Contexte utilisé"
        aria-valuemin="0"
        data-testid="context-gauge"
        [attr.data-level]="c.level"
        [attr.aria-valuemax]="c.max"
        [attr.aria-valuenow]="c.max ? c.used : null"
        [attr.aria-valuetext]="tip()"
        [gTooltip]="tip()"
      >
        @if (c.max) {
          <span class="track" aria-hidden="true">
            <span class="fill" [style.width.%]="percent()"></span>
          </span>
        }
        <span class="value" data-testid="context-value">{{ c.label }}</span>
      </div>
    }
  `,
  styles: `
    .gauge {
      display: flex;
      align-items: center;
      gap: 8px;
      min-height: 32px;
      padding: 0 8px;
      border-radius: var(--g-radius);
      font-size: 12px;
      font-variant-numeric: tabular-nums;
      color: var(--g-muted);
      white-space: nowrap;
    }
    .gauge:focus-visible {
      outline: 2px solid var(--g-accent);
      outline-offset: -2px;
    }
    .track {
      position: relative;
      width: 48px;
      height: 4px;
      border-radius: 2px;
      background: var(--g-track);
      overflow: hidden;
    }
    .fill {
      position: absolute;
      inset: 0 auto 0 0;
      background: var(--g-accent);
    }
    .gauge[data-level='warn'] .value,
    .gauge[data-level='full'] .value {
      color: var(--g-warn);
      font-weight: 600;
    }
    .gauge[data-level='warn'] .fill,
    .gauge[data-level='full'] .fill {
      background: var(--g-hybrid);
    }
  `,
})
export class ContextGauge {
  protected readonly store = inject(ChatStore);

  protected readonly percent = computed(() => {
    const c = this.store.context();
    return c?.max ? Math.min(100, (c.used / c.max) * 100) : 0;
  });

  /** « Contexte occupé à la fin de la dernière réponse : 3 254 tokens sur 16 384 ». */
  protected readonly tip = computed(() => {
    const c = this.store.context();
    if (!c) return '';
    const of = c.max ? ` sur ${int(c.max)}` : ' (contexte du modèle inconnu)';
    return `Contexte occupé à la fin de la dernière réponse : ${int(c.used)} tokens${of}`;
  });
}
