import { Component, computed, inject } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatSnackBar } from '@angular/material/snack-bar';
import { MatTooltipModule } from '@angular/material/tooltip';
import type { VariantDetail } from '../../core/generated/VariantDetail';
import { TauriService, errorMessage } from '../../core/tauri.service';
import { decimal, relativeTime } from '../../shared/format';
import { FIT_LABELS, runtimeLabel, useCaseLabel } from '../../shared/labels';
import { ModeBadge } from '../../shared/mode-badge';
import { CountPipe, CtxPipe, GbPipe, ParamsPipe, TokSPipe } from '../../shared/pipes';
import { scoreTooltip } from '../../shared/score';
import { DetailStore } from '../../state/detail.store';
import { MeasurePanel } from './measure-panel';
import { MemoryBar } from './memory-bar';

/** Vert : tient confortablement ; orange : limite ; rouge : trop juste ou trop gros. */
export function fitClass(v: VariantDetail): 'ok' | 'warn' | 'bad' {
  if (v.estimate.mode.kind === 'tooBig') return 'bad';
  switch (v.estimate.fit) {
    case 'perfect':
    case 'good':
      return 'ok';
    case 'marginal':
      return 'warn';
    case 'tooTight':
      return 'bad';
  }
}

/**
 * Fiche détail (panneau latéral) : barre mémoire, toutes les quantifications, commandes du
 * runtime choisi à copier, mesure réelle avec Ollama, dépôts alternatifs, liens vers Hugging
 * Face.
 */
@Component({
  selector: 'app-model-detail',
  imports: [
    MatButtonModule,
    MatIconModule,
    MatProgressBarModule,
    MatTooltipModule,
    MeasurePanel,
    MemoryBar,
    ModeBadge,
    CountPipe,
    CtxPipe,
    GbPipe,
    ParamsPipe,
    TokSPipe,
  ],
  templateUrl: './model-detail.html',
  styleUrl: './model-detail.scss',
})
export class ModelDetailPanel {
  protected readonly store = inject(DetailStore);
  private readonly tauri = inject(TauriService);
  private readonly snackBar = inject(MatSnackBar);

  protected readonly detail = this.store.detail;
  protected readonly selected = this.store.selected;

  protected readonly fitClass = fitClass;
  protected readonly fitLabels = FIT_LABELS;
  protected readonly scoreTooltip = scoreTooltip;
  protected readonly decimal = decimal;

  protected readonly runtimeName = computed(() => {
    const d = this.detail();
    return d ? runtimeLabel(d.runtime) : '';
  });

  /** Étiquettes : vision, protégé, MoE, cas d'usage reconnus. */
  protected readonly tags = computed(() => {
    const m = this.detail()?.model;
    if (!m) return [];
    const tags: string[] = [];
    if (m.paramsActive !== null && m.paramsActive < m.paramsTotal) tags.push('MoE');
    for (const u of m.useCases) if (u !== 'general') tags.push(useCaseLabel(u));
    if (m.gated) tags.push('Protégé');
    return tags;
  });

  protected readonly released = computed(() => {
    const created = this.detail()?.model.createdAt;
    return created ? relativeTime(created) : '';
  });

  protected hfUrl(repoId: string): string {
    return `https://huggingface.co/${repoId}`;
  }

  protected async copy(command: string): Promise<void> {
    try {
      await this.tauri.copyText(command);
      this.snackBar.open('Commande copiée', undefined, { duration: 2000 });
    } catch (e) {
      this.snackBar.open(`Copie impossible : ${errorMessage(e)}`, 'OK');
    }
  }

  protected async openUrl(url: string): Promise<void> {
    try {
      await this.tauri.openUrl(url);
    } catch (e) {
      this.snackBar.open(`Ouverture impossible : ${errorMessage(e)}`, 'OK');
    }
  }
}
