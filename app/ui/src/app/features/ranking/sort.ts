import type { RankItem } from '../../core/generated/RankItem';
import { MODE_ORDER } from '../../shared/labels';

/** Une ligne affichée, avec son rang dans le classement complet (colonne « # »). */
export interface RankedRow {
  rank: number;
  item: RankItem;
}

export type SortKey = 'rank' | 'model' | 'quant' | 'size' | 'mode' | 'tokS' | 'ctxMax' | 'score';
export type SortDir = 'asc' | 'desc' | '';

/** Tri choisi ; `active: ''` : l'ordre reçu du backend (score, ou proximité pour les trop gros). */
export interface SortState {
  active: SortKey | '';
  direction: SortDir;
}

export const NO_SORT: SortState = { active: '', direction: '' };

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

/** Sens du premier clic : le meilleur d'abord (plus rapide, plus long contexte, plus petit…). */
export const NATURAL_DIR: Record<SortKey, 'asc' | 'desc'> = {
  rank: 'asc',
  model: 'asc',
  quant: 'asc',
  size: 'asc',
  mode: 'asc',
  tokS: 'desc',
  ctxMax: 'desc',
  score: 'desc',
};

/** Libellés du bouton « Trier : … » et de son menu. */
export const SORT_LABELS: Record<SortKey, string> = {
  rank: 'rang',
  model: 'nom',
  quant: 'quantification',
  size: 'taille',
  mode: 'mode',
  tokS: 'vitesse',
  ctxMax: 'contexte max',
  score: 'score',
};

/**
 * Clic sur un en-tête : sens naturel, puis sens inverse, puis retour à l'ordre du classement.
 */
export function nextSort(current: SortState, key: SortKey): SortState {
  const natural = NATURAL_DIR[key];
  if (current.active !== key || current.direction === '')
    return { active: key, direction: natural };
  if (current.direction === natural) {
    return { active: key, direction: natural === 'asc' ? 'desc' : 'asc' };
  }
  return NO_SORT;
}

/** `aria-sort` d'un en-tête. */
export function ariaSort(current: SortState, key: SortKey): 'ascending' | 'descending' | null {
  if (current.active !== key || current.direction === '') return null;
  return current.direction === 'asc' ? 'ascending' : 'descending';
}

/**
 * Lignes dans l'ordre reçu (du meilleur score au moins bon), avec le rang calculé par le backend :
 * pendant une recherche, il reste celui du classement complet.
 */
export function rankedRows(items: readonly RankItem[]): RankedRow[] {
  return items.map((item) => ({ rank: item.rank, item }));
}

/** Tri par colonne ; sans tri actif, l'ordre reçu. À égalité, l'ordre du rang. */
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
