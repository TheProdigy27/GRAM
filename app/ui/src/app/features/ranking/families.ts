import type { RankItem } from '../../core/generated/RankItem';
import { type SortDir, rankedRows, sortRows } from './sort';

/** Variantes montrées sous une famille dépliée, avant « Voir les N autres variantes ». */
export const VARIANTS_SHOWN = 3;

/**
 * Une famille du classement : le modèle officiel et ses variantes (fine-tunes, versions
 * « abliterated »…) présents dans la liste.
 */
export interface Family {
  /** Dépôt canonique du modèle officiel (`family ?? repoId`). */
  key: string;
  /** Ligne de famille : le modèle officiel s'il est dans la liste, sinon la première variante. */
  head: RankItem;
  /** La ligne de famille est le modèle officiel. */
  official: boolean;
  /** Les autres modèles de la famille, dans l'ordre de la liste. */
  variants: RankItem[];
}

/** Une ligne affichée : un modèle (ligne de famille ou variante) ou « Voir les N autres ». */
export type TreeRow = ModelRow | MoreRow;

export interface ModelRow {
  kind: 'model';
  /** `repoId` du modèle. */
  id: string;
  item: RankItem;
  family: Family;
  /** 1 : ligne de famille (ou modèle seul) ; 2 : variante. */
  level: 1 | 2;
  /** Famille dépliée ou non ; `null` : pas de variante (ou ligne de variante). */
  expanded: boolean | null;
  /** Variante mieux notée que le modèle officiel de sa famille. */
  better: boolean;
}

export interface MoreRow {
  kind: 'more';
  id: string;
  family: Family;
  /** Variantes pas encore montrées. */
  hidden: number;
}

export function familyKey(item: RankItem): string {
  return item.family ?? item.repoId;
}

/**
 * Regroupe une liste du classement par famille. Une famille prend la place de sa ligne de
 * famille : celle du modèle officiel s'il est dans la liste (une variante mieux classée descend
 * avec lui), sinon celle de sa meilleure variante.
 */
export function groupFamilies(items: readonly RankItem[]): Family[] {
  const members = new Map<string, RankItem[]>();
  for (const item of items) {
    const key = familyKey(item);
    const list = members.get(key);
    if (list) list.push(item);
    else members.set(key, [item]);
  }
  const position = new Map(items.map((item, i) => [item, i]));
  return [...members]
    .map(([key, list]): Family => {
      const head = list.find((i) => i.repoId === key) ?? list[0];
      return { key, head, official: head.repoId === key, variants: list.filter((i) => i !== head) };
    })
    .sort((a, b) => position.get(a.head)! - position.get(b.head)!);
}

/**
 * Tri par colonne : les familles d'après leur ligne de famille, les variantes entre elles. Sans
 * tri actif, l'ordre de la liste.
 */
export function sortFamilies(families: readonly Family[], active: string, dir: SortDir): Family[] {
  const byHead = new Map(families.map((f) => [f.head, f]));
  return sortRows(rankedRows(families.map((f) => f.head)), active, dir).map(({ item }) => {
    const f = byHead.get(item)!;
    const variants = sortRows(rankedRows(f.variants), active, dir).map((r) => r.item);
    return { ...f, variants };
  });
}

/**
 * Lignes affichées : chaque famille, puis, si elle est dépliée, ses premières variantes et
 * « Voir les N autres » (toutes avec `showsAll`).
 */
export function treeRows(
  families: readonly Family[],
  isExpanded: (key: string) => boolean,
  showsAll: (key: string) => boolean,
): TreeRow[] {
  const rows: TreeRow[] = [];
  for (const family of families) {
    const { key, head, variants } = family;
    const open = variants.length > 0 && isExpanded(key);
    rows.push({
      kind: 'model',
      id: head.repoId,
      item: head,
      family,
      level: 1,
      expanded: variants.length > 0 ? open : null,
      better: false,
    });
    if (!open) continue;
    const shown = showsAll(key) ? variants : variants.slice(0, VARIANTS_SHOWN);
    // Scores comparés tels qu'affichés : pas de « mieux noté » à 97 contre 97.
    const headScore = Math.round(head.score.total);
    for (const item of shown) {
      rows.push({
        kind: 'model',
        id: item.repoId,
        item,
        family,
        level: 2,
        expanded: null,
        better: family.official && Math.round(item.score.total) > headScore,
      });
    }
    if (shown.length < variants.length) {
      rows.push({
        kind: 'more',
        id: `more:${key}`,
        family,
        hidden: variants.length - shown.length,
      });
    }
  }
  return rows;
}

/** Lignes sans regroupement : un modèle par ligne, dans l'ordre du tri. */
export function flatRows(items: readonly RankItem[], active: string, dir: SortDir): TreeRow[] {
  return sortRows(rankedRows(items), active, dir).map(({ item }) => ({
    kind: 'model',
    id: item.repoId,
    item,
    family: { key: item.repoId, head: item, official: true, variants: [] },
    level: 1,
    expanded: null,
    better: false,
  }));
}
