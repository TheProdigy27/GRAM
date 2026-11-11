import {
  Component,
  ElementRef,
  booleanAttribute,
  computed,
  inject,
  input,
  model,
  output,
} from '@angular/core';
import type { MemoryBudget } from '../../core/generated/MemoryBudget';
import type { RankItem } from '../../core/generated/RankItem';
import { CtxPipe, ParamsPipe, TokSPipe } from '../../shared/pipes';
import { Icon } from '../../shared/ui/icon/icon';
import type { IconName } from '../../shared/ui/icon/icons';
import { ModeBadge } from '../../shared/ui/mode-badge/mode-badge';
import { Tag } from '../../shared/ui/tag/tag';
import { Tooltip } from '../../shared/ui/tooltip/tooltip';
import { scaleLabel } from './footprint';
import { FootprintCell } from './footprint-cell';
import { ScoreCell } from './score-cell';
import {
  NO_SORT,
  type RankedRow,
  type SortKey,
  type SortState,
  ariaSort,
  nextSort,
  rankedRows,
  sortRows,
} from './sort';

/** Lignes sautées par Page précédente / Page suivante. */
const PAGE_STEP = 10;

/**
 * Une liste du classement (exécutables, lents ou trop gros) en grille ARIA : en-têtes triables,
 * empreinte sur l'échelle de la carte mémoire, badge de mode, score en mini-barres.
 *
 * Clavier : une seule ligne est dans l'ordre de tabulation (la ligne active) ; ↑ ↓, Début, Fin,
 * Page précédente / suivante passent d'une ligne à l'autre, Entrée ouvre la fiche. La ligne qui
 * prend le focus devient la ligne active (`activeChange`).
 */
@Component({
  selector: 'app-ranking-table',
  imports: [FootprintCell, Icon, ModeBadge, ScoreCell, Tag, Tooltip, CtxPipe, ParamsPipe, TokSPipe],
  templateUrl: './ranking-table.html',
  styleUrl: './ranking-table.scss',
  host: { '[attr.data-density]': 'compact() ? "compact" : null' },
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
  readonly sort = model<SortState>(NO_SORT);
  readonly openModel = output<string>();
  readonly activeChange = output<string>();

  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;

  protected readonly columns: readonly { key: SortKey; label: string; num?: boolean }[] = [
    { key: 'rank', label: '#', num: true },
    { key: 'model', label: 'Modèle' },
    { key: 'quant', label: 'Quant' },
    { key: 'size', label: 'Taille' },
    { key: 'mode', label: 'Mode' },
    { key: 'tokS', label: 'tok/s', num: true },
    { key: 'ctxMax', label: 'Ctx max', num: true },
    { key: 'score', label: 'Score', num: true },
  ];

  protected readonly rows = computed(() =>
    sortRows(rankedRows(this.items()), this.sort().active, this.sort().direction),
  );

  private readonly activeIndex = computed(() =>
    this.rows().findIndex((r) => r.item.repoId === this.active()),
  );
  /** Ligne dans l'ordre de tabulation : l'active, sinon la première. */
  protected readonly tabbable = computed(() => Math.max(this.activeIndex(), 0));

  protected readonly scale = computed(() => scaleLabel(this.budget()));
  protected readonly rowCount = computed(() => {
    const total = this.total();
    return total !== null && total > this.rows().length ? total + 1 : null;
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

  protected open(row: RankedRow): void {
    this.openModel.emit(row.item.repoId);
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
      case 'Enter':
        event.preventDefault();
        this.open(this.rows()[index]);
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
    const i = this.activeIndex();
    this.focusRow(i < 0 ? 0 : Math.min(Math.max(i + step, 0), count - 1));
    return true;
  }

  private focusRow(index: number): void {
    const row = this.host.querySelectorAll<HTMLElement>('.body > [role="row"]')[index];
    if (!row) return;
    row.focus({ preventScroll: true });
    // `scroll-margin-top` laisse la ligne sous l'en-tête collant (jsdom n'a pas scrollIntoView).
    row.scrollIntoView?.({ block: 'nearest' });
  }
}
