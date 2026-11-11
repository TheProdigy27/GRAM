import { Component, computed, input, output, signal } from '@angular/core';
import { MatIconModule } from '@angular/material/icon';
import { MatSortModule, type Sort } from '@angular/material/sort';
import { MatTableModule } from '@angular/material/table';
import { MatTooltipModule } from '@angular/material/tooltip';
import type { RankItem } from '../../core/generated/RankItem';
import { ModeBadge } from '../../shared/ui/mode-badge/mode-badge';
import { CtxPipe, GbPipe, ParamsPipe, TokSPipe } from '../../shared/pipes';
import { scoreTooltip } from '../../shared/score';
import { type RankedRow, type SortDir, rankedRows, sortRows } from './sort';

/**
 * Une liste du classement (exécutables, lents ou trop gros) : tri par colonne, badge de mode,
 * détail du score en infobulle. Un clic (ou Entrée) sur une ligne ouvre la fiche détail.
 */
@Component({
  selector: 'app-ranking-table',
  imports: [
    MatIconModule,
    MatSortModule,
    MatTableModule,
    MatTooltipModule,
    ModeBadge,
    CtxPipe,
    GbPipe,
    ParamsPipe,
    TokSPipe,
  ],
  templateUrl: './ranking-table.html',
  styleUrl: './ranking-table.scss',
})
export class RankingTable {
  readonly items = input.required<readonly RankItem[]>();
  /** Nom de la liste pour les lecteurs d'écran. */
  readonly label = input.required<string>();
  /** Modèle dont la fiche est ouverte (ligne surlignée). */
  readonly selected = input<string | null>(null);
  readonly openModel = output<string>();

  protected readonly columns = [
    'rank',
    'model',
    'quant',
    'size',
    'mode',
    'tokS',
    'ctxMax',
    'score',
  ];
  protected readonly sort = signal<Sort>({ active: '', direction: '' });
  protected readonly rows = computed(() =>
    sortRows(rankedRows(this.items()), this.sort().active, this.sort().direction as SortDir),
  );

  protected readonly scoreTooltip = scoreTooltip;
  protected readonly trackRow = (_: number, row: RankedRow) => row.item.repoId;

  protected open(row: RankedRow): void {
    this.openModel.emit(row.item.repoId);
  }
}
