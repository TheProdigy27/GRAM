// Contenu de la palette Ctrl K : correspondance des commandes, modèles trouvés, groupes.

import type { RankItem } from '../../core/generated/RankItem';
import type { RankResponse } from '../../core/generated/RankResponse';
import type { IconName } from '../../shared/ui/icon/icons';

/** Modèles montrés au plus dans le groupe « Modèles ». */
export const MAX_MODELS = 6;

/** Une commande de la palette. */
export interface PaletteCommand {
  id: string;
  label: string;
  /** Précision à droite du libellé (« .md », « GPU, RAM, runtimes », « actuel »). */
  hint: string;
  icon: IconName;
  /** Touches du raccourci, affichées à droite (« E », « M ») ; vide : aucun. */
  keys: readonly string[];
  /** Mots cherchés en plus du libellé et de la précision. */
  keywords?: string;
  /** Proposée seulement avec une saisie (usages, contextes : trop nombreux sinon). */
  searchOnly?: boolean;
  run: () => void;
}

/** Liste du classement où un modèle a été trouvé. */
export type PaletteSection = 'items' | 'slow' | 'tooBig';

/** Un modèle de la palette ; `item` absent : récent qui n'est plus classé avec ces filtres. */
export interface PaletteModel {
  repoId: string;
  displayName: string;
  item: RankItem | null;
  section: PaletteSection | null;
}

export type PaletteOption =
  | { kind: 'filter'; query: string; count: number | null }
  | { kind: 'model'; model: PaletteModel }
  | { kind: 'command'; command: PaletteCommand };

export interface PaletteGroup {
  label: string;
  options: PaletteOption[];
}

/** Mots de la saisie, en minuscules ; aucun pour une saisie vide ou faite d'espaces. */
export function searchWords(query: string): string[] {
  return query.toLowerCase().split(/\s+/).filter(Boolean);
}

/** Sans accents : « ré-analyser » → « re-analyser ». */
function fold(s: string): string {
  return s.normalize('NFD').replace(/\p{M}/gu, '');
}

const ALPHANUMERIC = /^[\p{L}\p{N}]+$/u;
const SEPARATORS = /[^\p{L}\p{N}]/gu;

/**
 * Même règle que la recherche des modèles en Rust (`Search` de `gram-core/src/rank.rs`) :
 * chaque mot doit figurer dans un des textes, sans tenir compte de la casse ; un mot fait
 * seulement de lettres et de chiffres ignore aussi les séparateurs (« 32k » trouve
 * « Contexte : 32k », « reanalyser » trouve « Ré-analyser »). En plus, les accents sont ignorés.
 */
export function matchesWords(words: readonly string[], texts: readonly string[]): boolean {
  const names = texts.map((t) => fold(t.toLowerCase()));
  const compact = names.map((n) => n.replace(SEPARATORS, ''));
  return words.every((raw) => {
    const w = fold(raw);
    const within = ALPHANUMERIC.test(w) ? compact : names;
    return within.some((n) => n.includes(w));
  });
}

/** Commandes proposées : toutes celles qui correspondent à la saisie, sinon les principales. */
export function matchingCommands(
  commands: readonly PaletteCommand[],
  words: readonly string[],
): PaletteCommand[] {
  if (words.length === 0) return commands.filter((c) => !c.searchOnly);
  return commands.filter((c) => matchesWords(words, [c.label, c.hint, c.keywords ?? '']));
}

/** Modèles trouvés par le backend : exécutables, puis lents, puis trop gros. */
export function foundModels(r: RankResponse, max = MAX_MODELS): PaletteModel[] {
  const lists: [PaletteSection, RankItem[]][] = [
    ['items', r.items],
    ['slow', r.slow],
    ['tooBig', r.tooBig],
  ];
  return lists
    .flatMap(([section, items]) =>
      items.map((item): PaletteModel => ({
        repoId: item.repoId,
        displayName: item.displayName,
        item,
        section,
      })),
    )
    .slice(0, max);
}

/** Nombre de modèles trouvés par une recherche, trop gros compris (comme les onglets). */
export function foundCount(r: RankResponse): number {
  return r.totalItems + r.totalSlow + r.tooBigCount;
}

/**
 * Groupes affichés. Avec une saisie : « Recherche » (filtrer le classement), « Modèles »,
 * « Commandes ». Sans saisie : « Récents » et les commandes principales.
 */
export function paletteGroups(
  query: string,
  models: readonly PaletteModel[],
  commands: readonly PaletteCommand[],
  count: number | null,
): PaletteGroup[] {
  const words = searchWords(query);
  const groups: PaletteGroup[] = [];
  if (words.length > 0) {
    // Espaces superflues retirées, comme le backend (`RankResponse.search`).
    const text = query.trim().split(/\s+/).join(' ');
    groups.push({ label: 'Recherche', options: [{ kind: 'filter', query: text, count }] });
  }
  if (models.length > 0) {
    groups.push({
      label: words.length > 0 ? 'Modèles' : 'Récents',
      options: models.map((model) => ({ kind: 'model', model })),
    });
  }
  const found = matchingCommands(commands, words);
  if (found.length > 0) {
    groups.push({
      label: 'Commandes',
      options: found.map((command) => ({ kind: 'command', command })),
    });
  }
  return groups;
}
