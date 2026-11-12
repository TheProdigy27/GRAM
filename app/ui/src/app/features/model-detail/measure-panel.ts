import { Component, computed, inject, input } from '@angular/core';
import type { ModelDetail } from '../../core/generated/ModelDetail';
import type { VariantDetail } from '../../core/generated/VariantDetail';
import { errorMessage } from '../../core/tauri.service';
import { ctxK, decimal, gb, int, relativeTime } from '../../shared/format';
import { CtxPipe, TokSPipe } from '../../shared/pipes';
import { Button } from '../../shared/ui/button/button';
import { Callout } from '../../shared/ui/callout/callout';
import { Icon } from '../../shared/ui/icon/icon';
import { Progress } from '../../shared/ui/progress/progress';
import { Tag } from '../../shared/ui/tag/tag';
import { ToastService } from '../../shared/ui/toast/toast';
import { Tooltip } from '../../shared/ui/tooltip/tooltip';
import { CalibrationStore } from '../../state/calibration.store';
import { HardwareStore } from '../../state/hardware.store';
import { CommandBlock } from './command-block';
import { memoryDumbbell, speedDumbbell } from './measure-chart';

/** Tokens générés par une mesure (`gram_ollama::MEASURE_TOKENS`), pour l'explication. */
const MEASURE_TOKENS = 128;

/**
 * Mesure réelle avec Ollama de la quantification choisie dans la fiche : vitesse et mémoire
 * mesurées face à l'estimation (graphiques « haltère »), puis recalibrage de l'estimation de
 * vitesse. Affiché seulement si Ollama est installé ; le runtime choisi pour les commandes
 * n'importe pas.
 */
@Component({
  selector: 'app-measure-panel',
  imports: [Button, Callout, CommandBlock, Icon, Progress, Tag, Tooltip, CtxPipe, TokSPipe],
  templateUrl: './measure-panel.html',
  styleUrl: './measure-panel.scss',
})
export class MeasurePanel {
  readonly detail = input.required<ModelDetail>();
  readonly variant = input.required<VariantDetail>();

  protected readonly store = inject(CalibrationStore);
  private readonly hardware = inject(HardwareStore);
  private readonly toast = inject(ToastService);

  protected readonly tokens = MEASURE_TOKENS;

  protected readonly ollama = computed(() => this.hardware.runtimes()?.ollama ?? null);

  private readonly key = computed(() => ({
    repoId: this.detail().model.repoId,
    label: this.variant().variant.label,
  }));

  private same(k: { repoId: string; label: string } | null): boolean {
    const mine = this.key();
    return !!k && k.repoId === mine.repoId && k.label === mine.label;
  }

  protected readonly measuring = computed(() => this.same(this.store.measuring()));
  /** Une mesure (de cette quantification ou d'une autre) est en cours. */
  protected readonly busy = computed(() => this.store.measuring() !== null);
  protected readonly error = computed(() => {
    const e = this.store.error();
    return e && this.same(e) ? e.message : null;
  });

  protected readonly latest = computed(() => {
    const { repoId, label } = this.key();
    return this.store.latest(repoId, label);
  });

  /** Vitesse, puis mémoire si Ollama l'a donnée. */
  protected readonly charts = computed(() => {
    const m = this.latest();
    if (!m) return [];
    const memory = memoryDumbbell(m, this.detail().budget);
    return memory ? [speedDumbbell(m), memory] : [speedDumbbell(m)];
  });

  /** « il y a 2 minutes · contexte 8k · 128 tokens · chargement 2,7 s · prompt 1 476 tok/s ». */
  protected readonly facts = computed(() => {
    const m = this.latest();
    if (!m) return '';
    const parts = [
      `Mesuré ${relativeTime(m.measuredAt)}`,
      `contexte ${ctxK(m.ctx)}`,
      `${int(m.evalCount)} tokens générés`,
      `chargement ${decimal(m.loadMs / 1000, 1)} s`,
    ];
    if (m.promptTokS !== null) parts.push(`prompt ${int(m.promptTokS)} tok/s`);
    if (m.memVram !== null) {
      parts.push(`dont VRAM ${gb(m.memVram)} (estimé ${gb(m.estimatedMemVram)})`);
    }
    return parts.join(' · ');
  });

  /**
   * Estimation actuelle, quand elle a changé depuis la mesure (paramètres recalibrés) et que
   * le contexte est le même.
   */
  protected readonly currentEstimate = computed(() => {
    const m = this.latest();
    const e = this.variant().estimate;
    if (!m || e.ctxUsed !== m.ctx || m.estimatedTokS <= 0) return null;
    return Math.abs(e.tokS - m.estimatedTokS) / m.estimatedTokS > 0.01 ? e.tokS : null;
  });

  protected readonly calibration = computed(() => {
    const c = this.store.calibration();
    return c && c.measurements.length > 0 ? c : null;
  });

  constructor() {
    void this.store.load();
  }

  protected async measure(): Promise<void> {
    const d = this.detail();
    try {
      await this.store.measure({
        repoId: d.model.repoId,
        label: this.variant().variant.label,
        useCase: d.useCase,
        ctx: d.ctx,
      });
      this.toast.show('Mesure terminée.', { kind: 'success' });
    } catch {
      // Affichée dans le panneau.
    }
  }

  protected async apply(): Promise<void> {
    try {
      const { estimate: e } = await this.store.apply();
      this.toast.show(
        `Estimation recalibrée (efficacité GPU ${decimal(e.gpuEfficiency)}, RAM ${decimal(e.ramEfficiency)}) : vitesses recalculées.`,
        { kind: 'success' },
      );
    } catch (e) {
      this.toast.show(`Recalibrage impossible : ${errorMessage(e)}`, { kind: 'error' });
    }
  }

  protected async clear(): Promise<void> {
    try {
      await this.store.clear();
    } catch (e) {
      this.toast.show(`Mesures non effacées : ${errorMessage(e)}`, { kind: 'error' });
    }
  }

  /** Après `ollama pull` ou le lancement d'Ollama. */
  protected refresh(): void {
    void this.hardware.refreshRuntimes();
  }
}
