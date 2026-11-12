import { Component, DestroyRef, computed, inject } from '@angular/core';
import type { Runtime } from '../../core/generated/Runtime';
import type { UseCase } from '../../core/generated/UseCase';
import { ShortcutsService } from '../../core/shortcuts.service';
import { errorMessage } from '../../core/tauri.service';
import { CTX_STEPS, ctxK, decimal } from '../../shared/format';
import { RUNTIMES, USE_CASES } from '../../shared/labels';
import { type RuntimeRow, runtimeRows } from '../../shared/runtimes';
import { FIELD } from '../../shared/ui/field/field';
import { Icon } from '../../shared/ui/icon/icon';
import type { IconName } from '../../shared/ui/icon/icons';
import { Switch } from '../../shared/ui/switch/switch';
import { ToastService } from '../../shared/ui/toast/toast';
import { ToggleGroup, type ToggleOption } from '../../shared/ui/toggle-group/toggle-group';
import { Tooltip } from '../../shared/ui/tooltip/tooltip';
import { DisplayStore } from '../../state/display.store';
import { HardwareStore } from '../../state/hardware.store';
import { RankingStore } from '../../state/ranking.store';
import { SettingsStore } from '../../state/settings.store';

const GIB = 2 ** 30;

const USE_CASE_ICONS: Record<UseCase, IconName> = {
  general: 'grid',
  code: 'code',
  reasoning: 'bulb',
  fastChat: 'bolt',
  vision: 'eye',
};

/**
 * Colonne de gauche, réglable en un clic : usage (touches 1 à 5), contexte en pastilles,
 * runtime avec son état, taille maximale (« Tient en VRAM »), interrupteurs. La recherche par
 * nom est dans l'en-tête.
 */
@Component({
  selector: 'app-filters-panel',
  imports: [FIELD, Icon, Switch, ToggleGroup, Tooltip],
  templateUrl: './filters-panel.html',
  styleUrl: './filters-panel.scss',
})
export class FiltersPanel {
  protected readonly ranking = inject(RankingStore);
  protected readonly display = inject(DisplayStore);
  private readonly settings = inject(SettingsStore);
  private readonly hardware = inject(HardwareStore);
  private readonly toast = inject(ToastService);

  protected readonly useCases = USE_CASES.map((u, i) => ({
    ...u,
    icon: USE_CASE_ICONS[u.value],
    key: String(i + 1),
  }));

  constructor() {
    const off = inject(ShortcutsService).register(
      this.useCases.map((u) => ({
        key: u.key,
        code: `Digit${u.key}`,
        run: () => this.onUseCase(u.value),
      })),
    );
    inject(DestroyRef).onDestroy(off);
  }

  // ── Contexte ──────────────────────────────────────────────────────────────────────────────

  protected readonly ctxOptions: ToggleOption<number>[] = CTX_STEPS.map((c) => ({
    value: c,
    label: ctxK(c),
  }));

  /** Pastille enfoncée : le contexte affiché s'il en est une (un défaut de 12k n'en est pas). */
  protected readonly ctxValue = computed(() => {
    const ctx = this.ranking.effectiveCtx();
    return ctx !== null && (CTX_STEPS as readonly number[]).includes(ctx) ? ctx : null;
  });

  protected readonly ctxLabel = computed(() => {
    const ctx = this.ranking.effectiveCtx();
    return ctx === null ? '…' : ctxK(ctx);
  });

  // ── Runtime ───────────────────────────────────────────────────────────────────────────────

  /** Les trois runtimes ; état vide tant que la détection n'a pas répondu. */
  protected readonly runtimes = computed<RuntimeRow[]>(() => {
    const status = this.hardware.runtimes();
    return status
      ? runtimeRows(status)
      : RUNTIMES.map((r): RuntimeRow => ({ ...r, state: 'absent', stateLabel: '', tooltip: '' }));
  });

  protected readonly runtimesKnown = computed(() => this.hardware.runtimes() !== null);

  /** Aide sous la liste : le runtime choisi n'est pas détecté. */
  protected readonly runtimeHint = computed(() => {
    if (!this.runtimesKnown()) return '';
    const r = this.runtimes().find((x) => x.value === this.ranking.effectiveRuntime());
    return r && r.state === 'absent'
      ? `${r.label} n’est pas détecté : le classement et les commandes restent valables.`
      : '';
  });

  // ── Taille max ────────────────────────────────────────────────────────────────────────────

  /** Taille max telle qu'affichée (« 14,3 ») ; suit la saisie et « Tient en VRAM ». */
  protected readonly maxSizeText = computed(() => {
    const gb = this.ranking.maxSizeGb();
    return gb === null ? '' : decimal(gb, 1);
  });

  /** VRAM utilisable, en Go arrondis au dixième inférieur ; `null` sans VRAM. */
  protected readonly vramGb = computed(() => {
    const vram = this.ranking.response()?.budget.vram ?? 0;
    const gb = Math.floor((vram / GIB) * 10) / 10;
    return gb > 0 ? gb : null;
  });

  protected readonly fitsVram = computed(() => {
    const v = this.vramGb();
    return v !== null && this.ranking.maxSizeGb() === v;
  });

  protected readonly vramTooltip = computed(() => {
    const v = this.vramGb();
    return v === null
      ? ''
      : `Taille max réglée sur la VRAM utilisable (${decimal(v, 1)} Go). Le cache et le contexte peuvent encore déborder en RAM.`;
  });

  // ── Actions ───────────────────────────────────────────────────────────────────────────────

  protected onUseCase(useCase: UseCase): void {
    this.ranking.setUseCase(useCase);
  }

  protected onCtx(ctx: number | null): void {
    if (ctx !== null) this.ranking.setCtx(ctx);
  }

  /** Le runtime choisi est aussi enregistré dans les paramètres (§10.3). */
  protected onRuntime(runtime: Runtime): void {
    this.ranking.setRuntime(runtime);
    if (!this.settings.settings()) return;
    this.settings.update({ runtime }).catch((e: unknown) => {
      this.toast.show(`Runtime non enregistré : ${errorMessage(e)}`, { kind: 'error' });
    });
  }

  protected onMaxSize(value: string): void {
    const gb = value.trim() === '' ? null : Number(value.trim().replace(',', '.'));
    this.ranking.setMaxSizeGb(gb);
  }

  protected toggleFitVram(): void {
    this.ranking.setMaxSizeGb(this.fitsVram() ? null : this.vramGb());
  }

  /** ↑ ↓ (Début, Fin) d'une option à l'autre dans une liste verticale. */
  protected onListKeydown(event: KeyboardEvent): void {
    const keys = ['ArrowDown', 'ArrowUp', 'Home', 'End'];
    if (!keys.includes(event.key) || event.altKey || event.ctrlKey || event.metaKey) return;
    const button = event.target as HTMLElement;
    const options = [...(button.parentElement?.querySelectorAll<HTMLElement>('.opt') ?? [])];
    const i = options.indexOf(button);
    if (i < 0) return;
    event.preventDefault();
    const last = options.length - 1;
    const next =
      event.key === 'Home'
        ? 0
        : event.key === 'End'
          ? last
          : event.key === 'ArrowDown'
            ? Math.min(i + 1, last)
            : Math.max(i - 1, 0);
    options[next].focus();
  }
}
