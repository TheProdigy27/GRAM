import { Component, computed, inject } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatCheckboxModule } from '@angular/material/checkbox';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatSelectModule } from '@angular/material/select';
import { MatSliderModule } from '@angular/material/slider';
import { MatSnackBar } from '@angular/material/snack-bar';
import type { Runtime } from '../../core/generated/Runtime';
import type { UseCase } from '../../core/generated/UseCase';
import { errorMessage } from '../../core/tauri.service';
import { CTX_STEPS, ctxK, ctxToStep, stepToCtx } from '../../shared/format';
import { RUNTIMES, USE_CASES, runtimeLabel, useCaseLabel } from '../../shared/labels';
import { HardwareStore } from '../../state/hardware.store';
import { RankingStore } from '../../state/ranking.store';
import { SettingsStore } from '../../state/settings.store';

/** Panneau de gauche : cas d'usage, contexte, runtime, taille maximale, modèles trop gros. */
@Component({
  selector: 'app-filters-panel',
  imports: [
    MatButtonModule,
    MatCheckboxModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatSelectModule,
    MatSliderModule,
  ],
  templateUrl: './filters-panel.html',
  styleUrl: './filters-panel.scss',
})
export class FiltersPanel {
  protected readonly ranking = inject(RankingStore);
  private readonly settings = inject(SettingsStore);
  private readonly hardware = inject(HardwareStore);
  private readonly snackBar = inject(MatSnackBar);

  protected readonly useCases = USE_CASES;
  protected readonly maxStep = CTX_STEPS.length - 1;
  protected readonly stepLabel = (step: number) => ctxK(stepToCtx(step));

  protected readonly ctxStep = computed(() => {
    const ctx = this.ranking.effectiveCtx();
    return ctx === null ? ctxToStep(8192) : ctxToStep(ctx);
  });

  protected readonly ctxLabel = computed(() => {
    const ctx = this.ranking.effectiveCtx();
    if (ctx === null) return '…';
    const label = ctxK(ctx);
    return this.ranking.ctx() === null
      ? `${label} (défaut ${useCaseLabel(this.ranking.useCase())})`
      : label;
  });

  /** Runtimes, avec leur état d'installation quand il est connu. */
  protected readonly runtimes = computed(() => {
    const status = this.hardware.runtimes();
    return RUNTIMES.map((r) => ({
      ...r,
      hint: status ? (status[r.value].installed ? 'installé' : 'non installé') : '',
    }));
  });

  protected readonly runtimeName = computed(() => runtimeLabel(this.ranking.effectiveRuntime()));

  protected readonly runtimeHint = computed(
    () => this.runtimes().find((r) => r.value === this.ranking.effectiveRuntime())?.hint ?? '',
  );

  protected onUseCase(useCase: UseCase): void {
    this.ranking.setUseCase(useCase);
  }

  protected onCtxStep(step: number): void {
    this.ranking.setCtx(stepToCtx(step));
  }

  /** Le runtime choisi est aussi enregistré dans les paramètres (§10.3). */
  protected onRuntime(runtime: Runtime): void {
    this.ranking.setRuntime(runtime);
    if (!this.settings.settings()) return;
    this.settings.update({ runtime }).catch((e: unknown) => {
      this.snackBar.open(`Runtime non enregistré : ${errorMessage(e)}`, 'OK');
    });
  }

  protected onMaxSize(value: string): void {
    const gb = value.trim() === '' ? null : Number(value.replace(',', '.'));
    this.ranking.setMaxSizeGb(gb);
  }
}
