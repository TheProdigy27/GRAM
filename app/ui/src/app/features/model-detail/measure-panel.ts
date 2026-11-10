import { Component, computed, inject, input } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatSnackBar } from '@angular/material/snack-bar';
import { MatTooltipModule } from '@angular/material/tooltip';
import type { ModelDetail } from '../../core/generated/ModelDetail';
import type { VariantDetail } from '../../core/generated/VariantDetail';
import { TauriService, errorMessage } from '../../core/tauri.service';
import { decimal, gb, int, relativeTime, signedPercent, tokS } from '../../shared/format';
import { CtxPipe, TokSPipe } from '../../shared/pipes';
import { CalibrationStore } from '../../state/calibration.store';
import { HardwareStore } from '../../state/hardware.store';

/** Tokens générés par une mesure (`gram_ollama::MEASURE_TOKENS`), pour l'explication. */
const MEASURE_TOKENS = 128;

/**
 * Mesure réelle avec Ollama (phase 7) de la quantification choisie dans la fiche : vitesse
 * et mémoire mesurées face à l'estimation, puis recalibrage de l'estimation de vitesse.
 * Affiché seulement si Ollama est installé ; le runtime choisi pour les commandes n'importe pas.
 */
@Component({
  selector: 'app-measure-panel',
  imports: [
    MatButtonModule,
    MatIconModule,
    MatProgressBarModule,
    MatTooltipModule,
    CtxPipe,
    TokSPipe,
  ],
  templateUrl: './measure-panel.html',
  styleUrl: './measure-panel.scss',
})
export class MeasurePanel {
  readonly detail = input.required<ModelDetail>();
  readonly variant = input.required<VariantDetail>();

  protected readonly store = inject(CalibrationStore);
  private readonly hardware = inject(HardwareStore);
  private readonly tauri = inject(TauriService);
  private readonly snackBar = inject(MatSnackBar);

  protected readonly tokens = MEASURE_TOKENS;
  protected readonly gap = signedPercent;
  protected readonly decimal = decimal;
  protected readonly int = int;

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

  /** Lignes du tableau mesuré / estimé / écart. */
  protected readonly rows = computed(() => {
    const m = this.latest();
    if (!m) return [];
    const size = (bytes: number | null) => (bytes === null ? '—' : gb(bytes));
    return [
      {
        label: 'Vitesse (tok/s)',
        measured: decimal(m.tokS, 1),
        estimated: tokS(m.estimatedTokS),
        gap: signedPercent(m.tokSError),
      },
      {
        label: 'Mémoire',
        measured: size(m.memTotal),
        estimated: gb(m.estimatedMemTotal),
        gap: signedPercent(m.memError),
      },
      {
        label: 'dont VRAM',
        measured: size(m.memVram),
        estimated: gb(m.estimatedMemVram),
        gap: '',
      },
    ];
  });

  protected readonly when = computed(() => {
    const m = this.latest();
    return m ? relativeTime(m.measuredAt) : '';
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
      this.snackBar.open('Mesure terminée.', undefined, { duration: 3000 });
    } catch {
      // Affichée dans le panneau.
    }
  }

  protected async apply(): Promise<void> {
    try {
      const s = await this.store.apply();
      this.snackBar.open(
        `Estimation recalibrée (efficacité ${decimal(s.estimate.efficiency)}) : vitesses recalculées.`,
        undefined,
        { duration: 4000 },
      );
    } catch (e) {
      this.snackBar.open(`Recalibrage impossible : ${errorMessage(e)}`, 'OK');
    }
  }

  protected async clear(): Promise<void> {
    try {
      await this.store.clear();
    } catch (e) {
      this.snackBar.open(`Mesures non effacées : ${errorMessage(e)}`, 'OK');
    }
  }

  /** Après `ollama pull` ou le lancement d'Ollama. */
  protected refresh(): void {
    void this.hardware.refreshRuntimes();
  }

  protected async copy(command: string): Promise<void> {
    try {
      await this.tauri.copyText(command);
      this.snackBar.open('Commande copiée', undefined, { duration: 2000 });
    } catch (e) {
      this.snackBar.open(`Copie impossible : ${errorMessage(e)}`, 'OK');
    }
  }
}
