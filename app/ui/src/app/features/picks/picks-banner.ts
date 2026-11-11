import { Component, computed, inject } from '@angular/core';
import type { MemoryBudget } from '../../core/generated/MemoryBudget';
import type { PickKind } from '../../core/generated/PickKind';
import type { RankPick } from '../../core/generated/RankPick';
import type { RankResponse } from '../../core/generated/RankResponse';
import type { RunMode } from '../../core/generated/RunMode';
import { TauriService, errorMessage } from '../../core/tauri.service';
import { ctxK, gb, tokS } from '../../shared/format';
import { PICK_LABELS, runtimeLabel, speedWord, useCaseLabel } from '../../shared/labels';
import { Button } from '../../shared/ui/button/button';
import { Icon } from '../../shared/ui/icon/icon';
import { ToastService } from '../../shared/ui/toast/toast';
import { Tooltip } from '../../shared/ui/tooltip/tooltip';
import { DetailStore } from '../../state/detail.store';
import { DisplayStore } from '../../state/display.store';
import { RankingStore } from '../../state/ranking.store';

/** Une mesure courte d'une carte : « **~33 tok/s** très fluide ». */
interface Metric {
  value: string;
  text: string;
}

interface PickCard {
  kind: PickKind;
  label: string;
  repoId: string;
  name: string;
  quant: string;
  size: string;
  /** Score du meilleur choix, sinon rang au classement. */
  score: number | null;
  rank: number;
  metrics: Metric[];
  reason: string;
  /** Commande de lancement à copier (la première du runtime), sinon page à ouvrir. */
  command: string | null;
  openUrl: string | null;
  runtime: string;
}

const TITLES = ['Conseil', 'Deux conseils', 'Trois conseils'];

/**
 * Bandeau « Trois conseils » en tête du classement : meilleur choix, le plus rapide, le plus long
 * contexte (calculés par le backend sur les exécutables, sans la recherche). Chaque carte copie
 * la commande de lancement du runtime choisi ou ouvre la fiche sur la quantification conseillée.
 * Repliable sur son titre (préférence gardée).
 */
@Component({
  selector: 'app-picks-banner',
  imports: [Button, Icon, Tooltip],
  templateUrl: './picks-banner.html',
  styleUrl: './picks-banner.scss',
})
export class PicksBanner {
  private readonly ranking = inject(RankingStore);
  private readonly detail = inject(DetailStore);
  private readonly tauri = inject(TauriService);
  private readonly toast = inject(ToastService);
  protected readonly display = inject(DisplayStore);

  protected readonly cards = computed<PickCard[]>(() => {
    const r = this.ranking.response();
    return r ? r.picks.map((p) => pickCard(p, r)) : [];
  });

  protected readonly title = computed(() => TITLES[this.cards().length - 1] ?? TITLES[2]);

  /** « Polyvalent · contexte 8k · Ollama ». */
  protected readonly summary = computed(() => {
    const r = this.ranking.response();
    if (!r) return '';
    return `${useCaseLabel(r.useCase)} · contexte ${ctxK(r.ctx)} · ${runtimeLabel(r.runtime)}`;
  });

  protected toggle(): void {
    this.display.setPicksCollapsed(!this.display.picksCollapsed());
  }

  protected openDetail(card: PickCard): void {
    this.detail.open(card.repoId, card.quant);
  }

  protected async copy(command: string): Promise<void> {
    try {
      await this.tauri.copyText(command);
      this.toast.show('Commande copiée', { kind: 'success' });
    } catch (e) {
      this.toast.show(`Copie impossible : ${errorMessage(e)}`, { kind: 'error' });
    }
  }

  protected async openUrl(url: string): Promise<void> {
    try {
      await this.tauri.openUrl(url);
    } catch (e) {
      this.toast.show(`Ouverture impossible : ${errorMessage(e)}`, { kind: 'error' });
    }
  }
}

function pickCard(pick: RankPick, r: RankResponse): PickCard {
  const { item, kind, launch } = pick;
  const { variant, estimate } = item.best;
  const speed = {
    value: `${tokS(estimate.tokS)} tok/s`,
    text: speedWord(estimate.tokS, r.slowTokS),
  };
  const context = { value: ctxK(estimate.ctxMax), text: 'de contexte max' };
  const where = placement(estimate.mode, r.budget);
  return {
    kind,
    label: PICK_LABELS[kind],
    repoId: item.repoId,
    name: item.displayName,
    quant: variant.label,
    size: gb(variant.sizeBytes),
    score: kind === 'best' ? Math.round(item.score.total) : null,
    rank: item.rank,
    metrics: kind === 'longestContext' ? [context, speed, where] : [speed, where, context],
    reason: pick.reason,
    command: launch.commands[0]?.command ?? null,
    openUrl: launch.openUrl,
    runtime: runtimeLabel(launch.runtime),
  };
}

/** « **tout** en VRAM », « **62/65** couches en VRAM » (« sur le GPU » en mémoire unifiée). */
function placement(mode: RunMode, budget: MemoryBudget): Metric {
  const gpu = budget.unified ? 'sur le GPU' : 'en VRAM';
  switch (mode.kind) {
    case 'gpu':
      return { value: 'tout', text: gpu };
    case 'hybrid':
      return { value: `${mode.gpuLayers}/${mode.layers}`, text: `couches ${gpu}` };
    default:
      return { value: 'tout', text: 'sur le CPU' };
  }
}
