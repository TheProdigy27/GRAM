import type { RankItem } from '../../core/generated/RankItem';
import { MODE_ORDER } from '../../shared/labels';

/** Une ligne affichée, avec son rang dans le classement complet (colonne « # »). */
export interface RankedRow {
  rank: number;
  item: RankItem;
}

export type SortKey = 'rank' | 'model' | 'quant' | 'size' | 'mode' | 'tokS' | 'ctxMax' | 'score';
export type SortDir = 'asc' | 'desc' | '';

const collator = new Intl.Collator('fr', { numeric: true, sensitivity: 'base' });

const KEYS: Record<SortKey, (r: RankedRow) => number | string> = {
  rank: (r) => r.rank,
  model: (r) => r.item.displayName,
  quant: (r) => r.item.best.variant.label,
  size: (r) => r.item.best.variant.sizeBytes,
  mode: (r) => MODE_ORDER[r.item.best.estimate.mode.kind],
  tokS: (r) => r.item.best.estimate.tokS,
  ctxMax: (r) => r.item.best.estimate.ctxMax,
  score: (r) => r.item.score.total,
};

/**
 * Lignes dans l'ordre reçu (du meilleur score au moins bon), avec le rang calculé par le backend :
 * pendant une recherche, il reste celui du classement complet.
 */
export function rankedRows(items: readonly RankItem[]): RankedRow[] {
  return items.map((item) => ({ rank: item.rank, item }));
}

/** Tri par colonne ; sans tri actif, l'ordre du score. À égalité, l'ordre du score. */
export function sortRows(rows: readonly RankedRow[], active: string, dir: SortDir): RankedRow[] {
  const key = KEYS[active as SortKey];
  if (!key || dir === '') return [...rows];
  const sign = dir === 'asc' ? 1 : -1;
  return [...rows].sort((a, b) => {
    const ka = key(a);
    const kb = key(b);
    const c =
      typeof ka === 'string' && typeof kb === 'string'
        ? collator.compare(ka, kb)
        : (ka as number) - (kb as number);
    return c * sign || a.rank - b.rank;
  });
}
