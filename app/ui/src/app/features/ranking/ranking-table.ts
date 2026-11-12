import {
  Component,
  ElementRef,
  Injector,
  afterNextRender,
  booleanAttribute,
  computed,
  inject,
  input,
  linkedSignal,
  model,
  output,
  signal,
} from '@angular/core';
import type { MemoryBudget } from '../../core/generated/MemoryBudget';
import type { RankItem } from '../../core/generated/RankItem';
import { CtxPipe, GbPipe, ParamsPipe, TokSPipe } from '../../shared/pipes';
import { Icon } from '../../shared/ui/icon/icon';
import type { IconName } from '../../shared/ui/icon/icons';
import { ModeBadge } from '../../shared/ui/mode-badge/mode-badge';
import { Tag } from '../../shared/ui/tag/tag';
import { Tooltip } from '../../shared/ui/tooltip/tooltip';
import {
  type ModelRow,
  type TreeRow,
  flatRows,
  groupFamilies,
  sortFamilies,
  treeRows,
} from './families';
import { scaleLabel } from './footprint';
import { FootprintCell } from './footprint-cell';
import { ScoreCell } from './score-cell';
import { NO_SORT, type SortKey, type SortState, ariaSort, nextSort } from './sort';

/** Colonnes repliées quand la fiche est ouverte. */
const DOCKED_HIDDEN: ReadonlySet<SortKey> = new Set(['quant', 'size', 'ctxMax']);

/** Lignes sautées par Page précédente / Page suivante. */
const PAGE_STEP = 10;

/**
 * Une liste du classement (exécutables, lents ou trop gros) en grille ARIA : en-têtes triables,
 * empreinte sur l'échelle de la carte mémoire, badge de mode, score en mini-barres.
 *
 * Avec `grouped`, les variantes d'un modèle (`RankItem.family`) sont regroupées sous sa ligne de
 * famille (grille arborescente) : repliées, puis 3 variantes et « Voir les N autres ». Pendant
 * une recherche, les familles sont dépliées et montrent toutes les variantes trouvées.
 *
 * Avec `docked` (fiche ouverte à côté), seules restent les colonnes #, Modèle (quant · taille en
 * sous-ligne), Mode, tok/s et Score (le total seul).
 *
 * Clavier : une seule ligne est dans l'ordre de tabulation (la ligne active) ; ↑ ↓, Début, Fin,
 * Page précédente / suivante passent d'une ligne à l'autre, Entrée ouvre la fiche. → déplie une
 * famille puis descend sur sa première variante, ← la replie ou remonte d'une variante à sa
 * famille. La ligne qui prend le focus devient la ligne active (`activeChange`).
 */
@Component({
  selector: 'app-ranking-table',
  imports: [
    FootprintCell,
    Icon,
    ModeBadge,
    ScoreCell,
    Tag,
    Tooltip,
    CtxPipe,
    GbPipe,
    ParamsPipe,
    TokSPipe,
  ],
  templateUrl: './ranking-table.html',
  styleUrl: './ranking-table.scss',
  host: {
    '[attr.data-density]': 'compact() ? "compact" : null',
    '[attr.data-tree]': 'tree() ? "" : null',
    '[attr.data-docked]': 'docked() ? "" : null',
  },
})
export class RankingTable {
  readonly items = input.required<readonly RankItem[]>();
  /** Nom de la liste pour les lecteurs d'écran. */
  readonly label = input.required<string>();
  /** Mémoire utilisable : échelle de l'empreinte. */
  readonly budget = input.required<MemoryBudget>();
  /** Modèle dont la fiche est ouverte (ligne surlignée). */
  readonly selected = input<string | null>(null);
  /** Ligne active (tabindex itinérant) ; sinon la première. */
  readonly active = input<string | null>(null);
  /** Nombre de lignes de la liste complète, au-delà de celles chargées (`aria-rowcount`). */
  readonly total = input<number | null>(null);
  /** Lignes de 46 px au lieu de 56. */
  readonly compact = input(false, { transform: booleanAttribute });
  /** Variantes regroupées sous leur modèle officiel. */
  readonly grouped = input(false, { transform: booleanAttribute });
  /** Fiche ouverte à côté : colonnes secondaires repliées. */
  readonly docked = input(false, { transform: booleanAttribute });
  /** Recherche appliquée : les familles sont dépliées tant qu'elle dure. */
  readonly search = input<string | null>(null);
  readonly sort = model<SortState>(NO_SORT);
  readonly openModel = output<string>();
  readonly activeChange = output<string>();

  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
  private readonly injector = inject(Injector);

  private readonly allColumns: readonly { key: SortKey; label: string; num?: boolean }[] = [
    { key: 'rank', label: '#', num: true },
    { key: 'model', label: 'Modèle' },
    { key: 'quant', label: 'Quant' },
    { key: 'size', label: 'Taille' },
    { key: 'mode', label: 'Mode' },
    { key: 'tokS', label: 'tok/s', num: true },
    { key: 'ctxMax', label: 'Ctx max', num: true },
    { key: 'score', label: 'Score', num: true },
  ];

  /** Familles repliées ou dépliées à la main ; oubliées quand la recherche change. */
  private readonly toggled = linkedSignal<string | null, ReadonlyMap<string, boolean>>({
    source: this.search,
    computation: () => new Map(),
  });
  /** Familles dont toutes les variantes sont montrées (« Voir les N autres »). */
  private readonly showingAll = linkedSignal<string | null, ReadonlySet<string>>({
    source: this.search,
    computation: () => new Set(),
  });

  protected readonly rows = computed<TreeRow[]>(() => {
    const { active, direction } = this.sort();
    if (!this.grouped()) return flatRows(this.items(), active, direction);
    const reveal = this.search() !== null;
    const toggled = this.toggled();
    const all = this.showingAll();
    return treeRows(
      sortFamilies(groupFamilies(this.items()), active, direction),
      (key) => toggled.get(key) ?? reveal,
      (key) => reveal || all.has(key),
    );
  });

  /** Au moins une famille a des variantes : grille arborescente, noms alignés après le chevron. */
  protected readonly tree = computed(() =>
    this.rows().some((r) => r.kind === 'more' || r.expanded !== null),
  );

  /** Ligne « Voir les N autres » qui a le focus (elle n'est pas une ligne active du store). */
  private readonly focusedMore = signal<string | null>(null);

  private readonly currentIndex = computed(() => {
    const rows = this.rows();
    const more = this.focusedMore();
    const i = more === null ? -1 : rows.findIndex((r) => r.id === more);
    return i >= 0 ? i : rows.findIndex((r) => r.kind === 'model' && r.id === this.active());
  });
  /** Ligne dans l'ordre de tabulation : l'active, sinon la première. */
  protected readonly tabbable = computed(() => Math.max(this.currentIndex(), 0));

  protected readonly scale = computed(() => scaleLabel(this.budget()));
  /** Fiche ouverte : sans Quant, Taille ni Ctx max (quant · taille passent en sous-ligne). */
  protected readonly columns = computed(() =>
    this.docked() ? this.allColumns.filter((c) => !DOCKED_HIDDEN.has(c.key)) : this.allColumns,
  );
  /** Colonnes couvertes par « Voir les N autres » (toutes sauf #). */
  protected readonly moreSpan = computed(() => this.columns().length - 1);
  protected readonly rowCount = computed(() => {
    const total = this.total();
    if (total === null || total <= this.items().length) return null;
    // Regroupées, les lignes restantes ne se comptent pas d'avance.
    return this.grouped() ? -1 : total + 1;
  });

  protected readonly ariaSort = ariaSort;

  protected sortBy(key: SortKey): void {
    this.sort.set(nextSort(this.sort(), key));
  }

  protected arrow(key: SortKey): IconName | null {
    const s = this.sort();
    if (s.active !== key || s.direction === '') return null;
    return s.direction === 'asc' ? 'arrow-up' : 'arrow-down';
  }

  protected variantWord(n: number): string {
    return n > 1 ? ' variantes' : ' variante';
  }

  /** « Mieux noté que le modèle officiel, Qwen3-8B : 91 contre 86 ». */
  protected betterHint(row: ModelRow): string {
    const head = row.family.head;
    const [mine, theirs] = [row.item, head].map((i) => Math.round(i.score.total));
    return `Mieux noté que le modèle officiel, ${head.displayName} : ${mine} contre ${theirs}`;
  }

  protected moreLabel(n: number): string {
    return n > 1 ? `Voir les ${n} autres variantes` : 'Voir l’autre variante';
  }

  protected onFocus(row: TreeRow): void {
    if (row.kind === 'more') {
      this.focusedMore.set(row.id);
    } else {
      this.focusedMore.set(null);
      this.activeChange.emit(row.id);
    }
  }

  /** Clic ou Entrée : ouvre la fiche, ou montre les variantes restantes. */
  protected activate(index: number): void {
    const row = this.rows()[index];
    if (!row) return;
    if (row.kind === 'model') {
      this.openModel.emit(row.id);
      return;
    }
    this.showingAll.update((s) => new Set(s).add(row.family.key));
    // La ligne « Voir les N autres » laisse place à la première variante montrée.
    afterNextRender(() => this.focusRow(index), { injector: this.injector });
  }

  /** Chevron : replie ou déplie sans ouvrir la fiche ; la ligne devient active. */
  protected toggle(event: Event, index: number): void {
    event.stopPropagation();
    const row = this.rows()[index];
    if (row?.kind !== 'model' || row.expanded === null) return;
    this.setExpanded(row.family.key, !row.expanded);
    this.focusRow(index);
  }

  protected onKeydown(event: KeyboardEvent, index: number): void {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const last = this.rows().length - 1;
    let next: number;
    switch (event.key) {
      case 'ArrowDown':
        next = Math.min(index + 1, last);
        break;
      case 'ArrowUp':
        next = Math.max(index - 1, 0);
        break;
      case 'PageDown':
        next = Math.min(index + PAGE_STEP, last);
        break;
      case 'PageUp':
        next = Math.max(index - PAGE_STEP, 0);
        break;
      case 'Home':
        next = 0;
        break;
      case 'End':
        next = last;
        break;
      case 'ArrowRight':
      case 'ArrowLeft':
        if (this.expandAt(index, event.key === 'ArrowRight')) event.preventDefault();
        return;
      case 'Enter':
        event.preventDefault();
        this.activate(index);
        return;
      default:
        return;
    }
    event.preventDefault();
    this.focusRow(next);
  }

  /**
   * ↑ ↓ hors du tableau : focus sur la ligne avant ou après l'active (la première s'il n'y en a
   * pas). `false` si la liste est vide.
   */
  move(step: 1 | -1): boolean {
    const count = this.rows().length;
    if (count === 0) return false;
    const i = this.currentIndex();
    this.focusRow(i < 0 ? 0 : Math.min(Math.max(i + step, 0), count - 1));
    return true;
  }

  /**
   * ← → hors du tableau : comme sur la ligne active (la première s'il n'y en a pas). `false` si
   * rien n'est à replier ou à déplier.
   */
  expand(open: boolean): boolean {
    return this.rows().length > 0 && this.expandAt(this.tabbable(), open);
  }

  /**
   * → sur une famille repliée : la déplie ; dépliée : descend sur sa première variante.
   * ← sur une famille dépliée : la replie ; sur une variante : remonte à sa famille.
   */
  private expandAt(index: number, open: boolean): boolean {
    const rows = this.rows();
    const row = rows[index];
    if (!row) return false;
    if (row.kind === 'model' && row.level === 1) {
      if (row.expanded === null || (!open && !row.expanded)) return false;
      if (open && row.expanded) {
        this.focusRow(index + 1);
      } else {
        this.setExpanded(row.family.key, open);
        this.focusRow(index);
      }
      return true;
    }
    if (open) return false;
    this.focusRow(rows.findIndex((r) => r.kind === 'model' && r.id === row.family.head.repoId));
    return true;
  }

  /** Focus sur la ligne d'un modèle (fiche refermée) ; `false` si elle n'est pas affichée. */
  focusModel(repoId: string): boolean {
    const i = this.rows().findIndex((r) => r.kind === 'model' && r.id === repoId);
    if (i < 0) return false;
    this.focusRow(i);
    return true;
  }

  private setExpanded(key: string, open: boolean): void {
    this.toggled.update((m) => new Map(m).set(key, open));
    // Replier oublie « Voir les N autres » : la famille rouverte en montre de nouveau 3.
    if (!open) {
      this.showingAll.update((s) => {
        const next = new Set(s);
        next.delete(key);
        return next;
      });
    }
  }

  private focusRow(index: number): void {
    const row = this.host.querySelectorAll<HTMLElement>('.body > [role="row"]')[index];
    if (!row) return;
    row.focus({ preventScroll: true });
    // `scroll-margin-top` laisse la ligne sous l'en-tête collant (jsdom n'a pas scrollIntoView).
    row.scrollIntoView?.({ block: 'nearest' });
  }
}
