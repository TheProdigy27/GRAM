import { Component, DestroyRef, computed, inject, signal, viewChild } from '@angular/core';
import type { RankItem } from '../../core/generated/RankItem';
import { ShortcutsService } from '../../core/shortcuts.service';
import { ctxK, decimal, int } from '../../shared/format';
import { runtimeLabel, useCaseLabel } from '../../shared/labels';
import { Button } from '../../shared/ui/button/button';
import { Callout } from '../../shared/ui/callout/callout';
import { EmptyState } from '../../shared/ui/empty-state/empty-state';
import { Icon } from '../../shared/ui/icon/icon';
import type { IconName } from '../../shared/ui/icon/icons';
import { MENU } from '../../shared/ui/menu/menu';
import { Progress } from '../../shared/ui/progress/progress';
import { Skeleton } from '../../shared/ui/skeleton/skeleton';
import { type TabItem, Tabs } from '../../shared/ui/tabs/tabs';
import { Tooltip } from '../../shared/ui/tooltip/tooltip';
import { DetailStore } from '../../state/detail.store';
import { RankingStore } from '../../state/ranking.store';
import { RankingTable } from './ranking-table';
import { NATURAL_DIR, NO_SORT, SORT_LABELS, type SortKey, type SortState } from './sort';

export type Section = 'items' | 'slow' | 'tooBig';

/** Ce que propose un état vide. */
type EmptyAction = 'clearSearch' | 'showSlow' | 'showTooBig' | 'resetCtx' | 'clearMaxSize';

interface EmptyView {
  testId: string;
  icon: IconName;
  heading: string;
  description: string;
  actions: EmptyAction[];
}

const SECTION_LABELS: Record<Section, { tab: string; list: string; testId: string }> = {
  items: { tab: 'Exécutables', list: 'Modèles exécutables', testId: 'table-items' },
  slow: { tab: 'Lents', list: 'Modèles exécutables mais lents', testId: 'table-slow' },
  tooBig: { tab: 'Trop gros', list: 'Modèles trop gros', testId: 'table-too-big' },
};

/** Entrées du menu « Trier » (l'ordre du classement d'abord). */
const SORT_MENU: readonly (SortKey | '')[] = [
  '',
  'score',
  'tokS',
  'ctxMax',
  'size',
  'model',
  'quant',
  'mode',
];

const plural = (n: number, word: string) => `${int(n)} ${word}${n > 1 ? 's' : ''}`;

/**
 * Zone principale : onglets Exécutables / Lents / Trop gros avec leurs compteurs, résumé des
 * filtres, tri, remarques du runtime, tableau et « Afficher 25 de plus ». ↑ ↓ (hors des champs)
 * passent d'une ligne à l'autre depuis n'importe où, sauf dans la fiche.
 */
@Component({
  selector: 'app-ranking-view',
  imports: [
    Button,
    Callout,
    EmptyState,
    Icon,
    MENU,
    Progress,
    RankingTable,
    Skeleton,
    Tabs,
    Tooltip,
  ],
  templateUrl: './ranking-view.html',
  styleUrl: './ranking-view.scss',
})
export class RankingView {
  protected readonly ranking = inject(RankingStore);
  protected readonly detail = inject(DetailStore);

  protected readonly response = this.ranking.response;
  private readonly table = viewChild(RankingTable);

  /** Onglet choisi ; « Trop gros » n'existe qu'avec l'interrupteur. */
  private readonly chosen = signal<Section>('items');
  protected readonly section = computed<Section>(() =>
    this.chosen() === 'tooBig' && !this.ranking.showTooBig() ? 'items' : this.chosen(),
  );
  protected readonly sort = signal<SortState>(NO_SORT);

  constructor() {
    const off = inject(ShortcutsService).register([
      { key: 'ArrowDown', run: (e) => this.moveFrom(e, 1) },
      { key: 'ArrowUp', run: (e) => this.moveFrom(e, -1) },
    ]);
    inject(DestroyRef).onDestroy(off);
  }

  protected readonly tabs = computed<TabItem[]>(() => {
    const r = this.response();
    if (!r) return [];
    const tabs: TabItem[] = [
      { value: 'items', label: SECTION_LABELS.items.tab, count: r.totalItems },
      { value: 'slow', label: SECTION_LABELS.slow.tab, count: r.totalSlow },
    ];
    if (this.ranking.showTooBig()) {
      tabs.push({ value: 'tooBig', label: SECTION_LABELS.tooBig.tab, count: r.tooBigCount });
    }
    return tabs;
  });

  /** Lignes de l'onglet affiché et leur nombre total (avant la pagination). */
  protected readonly list = computed(() => {
    const r = this.response();
    if (!r) return null;
    const section = this.section();
    const [items, total]: [RankItem[], number] =
      section === 'items'
        ? [r.items, r.totalItems]
        : section === 'slow'
          ? [r.slow, r.totalSlow]
          : [r.tooBig, r.tooBigCount];
    return { section, items, total, ...SECTION_LABELS[section] };
  });

  /** Lignes pas encore chargées, et combien « Afficher N de plus » en ajoute. */
  protected readonly more = computed(() => {
    const l = this.list();
    if (!l) return null;
    const left = l.total - l.items.length;
    return left > 0 ? { next: Math.min(left, this.ranking.pageSize()), left } : null;
  });

  /** « Polyvalent · contexte 8k · Ollama », plus le sens de l'onglet affiché. */
  protected readonly summary = computed(() => {
    const r = this.response();
    if (!r) return '';
    const base = `${useCaseLabel(r.useCase)} · contexte ${ctxK(r.ctx)} · ${runtimeLabel(r.runtime)}`;
    const section = this.section();
    if (section === 'slow') return `${base} · sous ${decimal(r.slowTokS, 1)} tok/s`;
    if (section === 'tooBig') return `${base} · du plus proche de tenir au plus éloigné`;
    return base;
  });

  /** « 7 trop gros · Afficher » quand l'onglet Trop gros est masqué. */
  protected readonly hiddenTooBig = computed(() => {
    const r = this.response();
    return r && !this.ranking.showTooBig() && r.tooBigCount > 0 ? r.tooBigCount : 0;
  });

  protected readonly sortMenu = SORT_MENU.map((key) => ({
    key,
    label: key === '' ? 'Ordre du classement' : capitalize(SORT_LABELS[key]),
  }));

  protected readonly sortLabel = computed(() => {
    const key = this.sort().active;
    return key === '' ? 'classement' : SORT_LABELS[key];
  });

  protected readonly sortIcon = computed<IconName>(() =>
    this.sort().direction === 'asc' ? 'arrow-up' : 'arrow-down',
  );

  // ── Remarques ─────────────────────────────────────────────────────────────────────────────

  /** Remarques masquées (toutes celles affichées à ce moment) : de nouvelles réapparaissent. */
  private readonly hiddenNotes = signal<string | null>(null);
  protected readonly notesOpen = signal(false);

  protected readonly notes = computed(() => {
    const notes = this.response()?.notes ?? [];
    return notes.length > 0 && notes.join('\n') !== this.hiddenNotes() ? notes : [];
  });

  protected hideNotes(): void {
    this.hiddenNotes.set(this.notes().join('\n'));
    this.notesOpen.set(false);
  }

  // ── États vides ───────────────────────────────────────────────────────────────────────────

  protected readonly empty = computed<EmptyView | null>(() => {
    const r = this.response();
    const l = this.list();
    if (!r || !l || l.items.length > 0) return null;
    const search = r.search;
    if (l.section === 'items') {
      if (search !== null) {
        const elsewhere = [
          r.totalSlow > 0 ? plural(r.totalSlow, 'lent') : '',
          r.tooBigCount > 0 ? `${int(r.tooBigCount)} trop gros` : '',
        ].filter(Boolean);
        return elsewhere.length === 0
          ? {
              testId: 'search-empty',
              icon: 'search',
              heading: `Aucun modèle ne correspond à « ${search} ».`,
              description: 'Chercher un autre nom, un dépôt ou un modèle de base.',
              actions: ['clearSearch'],
            }
          : {
              testId: 'search-empty',
              icon: 'search',
              heading: `Aucun modèle exécutable assez rapide ne correspond à « ${search} ».`,
              description: `Trouvés ailleurs : ${elsewhere.join(', ')}.`,
              actions: [
                ...(r.totalSlow > 0 ? (['showSlow'] as const) : []),
                ...(r.tooBigCount > 0 ? (['showTooBig'] as const) : []),
                'clearSearch',
              ],
            };
      }
      return {
        testId: 'filters-empty',
        icon: 'filter',
        heading: 'Aucun modèle ne tient en mémoire assez vite avec ces filtres.',
        description:
          'Essayer un contexte plus court ou une taille max plus grande, ou regarder les modèles lents.',
        actions: [
          ...(r.totalSlow > 0 ? (['showSlow'] as const) : []),
          ...(this.ranking.ctx() !== null ? (['resetCtx'] as const) : []),
          ...(this.ranking.maxSizeGb() !== null ? (['clearMaxSize'] as const) : []),
        ],
      };
    }
    const what = l.section === 'slow' ? 'lent' : 'trop gros';
    if (search !== null) {
      return {
        testId: 'search-empty',
        icon: 'search',
        heading: `Aucun modèle ${what} ne correspond à « ${search} ».`,
        description: '',
        actions: ['clearSearch'],
      };
    }
    return l.section === 'slow'
      ? {
          testId: 'section-empty',
          icon: 'gauge',
          heading: 'Aucun modèle lent avec ces filtres.',
          description: `Tous les modèles exécutables dépassent ${decimal(r.slowTokS, 1)} tok/s estimés.`,
          actions: [],
        }
      : {
          testId: 'section-empty',
          icon: 'check',
          heading: 'Aucun modèle trop gros.',
          description: 'Tout le catalogue filtré tient sur cette machine.',
          actions: [],
        };
  });

  protected readonly actionLabels: Record<EmptyAction, string> = {
    clearSearch: 'Effacer la recherche',
    showSlow: 'Voir les lents',
    showTooBig: 'Voir les trop gros',
    resetCtx: 'Contexte par défaut',
    clearMaxSize: 'Retirer la taille max',
  };

  protected run(action: EmptyAction): void {
    switch (action) {
      case 'clearSearch':
        this.ranking.setSearch('');
        break;
      case 'showSlow':
        this.select('slow');
        break;
      case 'showTooBig':
        this.showTooBig();
        break;
      case 'resetCtx':
        this.ranking.setCtx(null);
        break;
      case 'clearMaxSize':
        this.ranking.setMaxSizeGb(null);
        break;
    }
  }

  // ── Actions ───────────────────────────────────────────────────────────────────────────────

  protected select(section: string): void {
    this.chosen.set(section as Section);
    // La ligne active d'un autre onglet n'a plus de sens.
    this.ranking.setActive(null);
  }

  protected showTooBig(): void {
    this.ranking.setShowTooBig(true);
    this.select('tooBig');
  }

  protected sortBy(key: SortKey | ''): void {
    this.sort.set(key === '' ? NO_SORT : { active: key, direction: NATURAL_DIR[key] });
  }

  protected open(repoId: string): void {
    this.detail.open(repoId);
  }

  /** ↑ ↓ hors du tableau : pas dans la fiche, qui défile. */
  private moveFrom(event: KeyboardEvent, step: 1 | -1): boolean {
    const target = event.target;
    if (target instanceof Element && target.closest('app-model-detail')) return false;
    return this.table()?.move(step) ?? false;
  }
}

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}
