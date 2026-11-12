import { Component, computed, inject, output } from '@angular/core';
import type { Os } from '../../core/generated/Os';
import { TauriService, errorMessage } from '../../core/tauri.service';
import { count, ctxK, paramCount, relativeTime } from '../../shared/format';
import { runtimeLabel, useCaseLabel } from '../../shared/labels';
import { CountPipe, CtxPipe } from '../../shared/pipes';
import { scoreTooltip } from '../../shared/score';
import { Button } from '../../shared/ui/button/button';
import { Callout } from '../../shared/ui/callout/callout';
import { Icon } from '../../shared/ui/icon/icon';
import type { IconName } from '../../shared/ui/icon/icons';
import { Progress } from '../../shared/ui/progress/progress';
import { Skeleton } from '../../shared/ui/skeleton/skeleton';
import { Tag } from '../../shared/ui/tag/tag';
import { ToastService } from '../../shared/ui/toast/toast';
import { ToggleGroup, type ToggleOption } from '../../shared/ui/toggle-group/toggle-group';
import { Tooltip } from '../../shared/ui/tooltip/tooltip';
import { DetailStore } from '../../state/detail.store';
import { DisplayStore, type QuantView } from '../../state/display.store';
import { HardwareStore } from '../../state/hardware.store';
import { RankingStore } from '../../state/ranking.store';
import { CommandBlock } from './command-block';
import { MeasurePanel } from './measure-panel';
import { MemoryBar } from './memory-bar';
import { QuantChart } from './quant-chart';
import { QuantTable } from './quant-table';
import { gpuSuggestion } from './suggestion';

/** Où taper les commandes, selon le système. */
const SHELLS: Record<Os, string> = {
  windows: 'Windows · PowerShell',
  macos: 'macOS · Terminal',
  linux: 'Linux · Terminal',
};

/**
 * Fiche détail, ancrée à droite du classement : en-tête (rang, score, dépôt, Hugging Face),
 * quatre tuiles, vitesse estimée selon la quantification (graphique ou tableau, choix d'une
 * quantification), suggestion d'une quantification qui tient en VRAM, mémoire, commandes du
 * runtime choisi, mesure réelle avec Ollama, autres dépôts GGUF.
 */
@Component({
  selector: 'app-model-detail',
  imports: [
    Button,
    Callout,
    CommandBlock,
    Icon,
    MeasurePanel,
    MemoryBar,
    Progress,
    QuantChart,
    QuantTable,
    Skeleton,
    Tag,
    ToggleGroup,
    Tooltip,
    CountPipe,
    CtxPipe,
  ],
  templateUrl: './model-detail.html',
  styleUrl: './model-detail.scss',
})
export class ModelDetailPanel {
  protected readonly store = inject(DetailStore);
  protected readonly display = inject(DisplayStore);
  private readonly ranking = inject(RankingStore);
  private readonly hardware = inject(HardwareStore);
  private readonly tauri = inject(TauriService);
  private readonly toast = inject(ToastService);

  /** Fermeture demandée (bouton ✕) : la coque ferme la fiche et rend le focus à la ligne. */
  readonly dismiss = output<void>();

  protected readonly detail = this.store.detail;
  protected readonly selected = this.store.selected;

  protected readonly scoreTooltip = scoreTooltip;

  protected readonly viewOptions: ToggleOption<QuantView>[] = [
    { value: 'chart', label: 'Graphique', icon: 'chart' },
    { value: 'table', label: 'Tableau', icon: 'table' },
  ];

  protected readonly title = computed(
    () => this.detail()?.model.displayName ?? this.store.repoId() ?? '',
  );

  /** « n° 2 · score 86 » : rang parmi les exécutables, score de la meilleure quantification. */
  protected readonly standing = computed(() => {
    const d = this.detail();
    if (!d || d.best === null) return null;
    const best = d.variants[d.best];
    if (!best) return null;
    const score = Math.round(best.score.total);
    const items = this.ranking.response()?.items ?? [];
    const item = items.find((i) => i.repoId === d.model.repoId);
    return {
      label: item ? `n° ${item.rank} · score ${score}` : `score ${score}`,
      tooltip: scoreTooltip(best.score),
    };
  });

  protected readonly runtimeName = computed(() => {
    const d = this.detail();
    return d ? runtimeLabel(d.runtime) : '';
  });

  protected readonly shell = computed(() => {
    const os = this.hardware.profile()?.os;
    return os ? SHELLS[os] : '';
  });

  /** Étiquettes : vision, MoE, cas d'usage reconnus, protégé. */
  protected readonly tags = computed(() => {
    const m = this.detail()?.model;
    if (!m) return [];
    const tags: { label: string; icon?: IconName }[] = [];
    if (m.pipeline === 'image-text-to-text') tags.push({ label: 'Vision', icon: 'eye' });
    if (m.paramsActive !== null && m.paramsActive < m.paramsTotal) tags.push({ label: 'MoE' });
    for (const u of m.useCases) {
      if (u !== 'general' && u !== 'vision') tags.push({ label: useCaseLabel(u) });
    }
    if (m.gated) tags.push({ label: 'Protégé', icon: 'lock' });
    return tags;
  });

  protected readonly released = computed(() => {
    const created = this.detail()?.model.createdAt;
    return created ? relativeTime(created) : '';
  });

  /** Paramètres, contexte d'entraînement, téléchargements, architecture. */
  protected readonly tiles = computed(() => {
    const m = this.detail()?.model;
    if (!m) return [];
    const moe = m.paramsActive !== null && m.paramsActive < m.paramsTotal;
    const arch = m.arch;
    const experts =
      arch?.expertCount && arch.expertUsedCount
        ? `${arch.expertUsedCount} experts actifs sur ${arch.expertCount}`
        : '';
    return [
      {
        key: 'params',
        label: 'Paramètres',
        value: paramCount(m.paramsTotal),
        sub: moe ? `${paramCount(m.paramsActive!)} actifs` : '',
      },
      { key: 'ctx', label: 'Contexte d’entraînement', value: ctxK(m.ctxTrain), sub: '' },
      { key: 'downloads', label: 'Téléchargements 30 j', value: count(m.downloads), sub: '' },
      {
        key: 'arch',
        label: 'Architecture',
        value: arch?.architecture ?? 'inconnue',
        sub: experts,
      },
    ];
  });

  protected readonly suggestion = computed(() => {
    const d = this.detail();
    return d ? gpuSuggestion(d.variants, this.store.selectedLabel()) : null;
  });

  protected hfUrl(repoId: string): string {
    return `https://huggingface.co/${repoId}`;
  }

  protected select(label: string): void {
    this.store.select(label);
  }

  protected setView(view: QuantView | null): void {
    if (view) this.display.setQuantView(view);
  }

  protected async openUrl(url: string): Promise<void> {
    try {
      await this.tauri.openUrl(url);
    } catch (e) {
      this.toast.show(`Ouverture impossible : ${errorMessage(e)}`, { kind: 'error' });
    }
  }
}
