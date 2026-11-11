import type { Score } from '../core/generated/Score';
import type { ScoreWeights } from '../core/generated/ScoreWeights';
import { decimal } from './format';

export type ScoreAxisKey = keyof ScoreWeights;

/** Les quatre axes du score, dans l'ordre des couleurs `--g-axis-*`. */
export const SCORE_AXES: readonly { key: ScoreAxisKey; label: string }[] = [
  { key: 'quality', label: 'Qualité' },
  { key: 'speed', label: 'Vitesse' },
  { key: 'fit', label: 'Marge mémoire' },
  { key: 'context', label: 'Contexte' },
];

export interface ScoreAxis {
  key: ScoreAxisKey;
  label: string;
  /** De 0 à 100. */
  value: number;
  /** Poids du cas d'usage (la somme fait 1). */
  weight: number;
}

export function scoreAxes(s: Score): ScoreAxis[] {
  return SCORE_AXES.map(({ key, label }) => ({
    key,
    label,
    value: s[key],
    weight: s.weights[key],
  }));
}

/** « Moyenne pondérée 82,2 × 0,95 + popularité 3,2 ». */
export function scoreFormula(s: Score): string {
  return `Moyenne pondérée ${decimal(s.weighted, 1)} × 0,95 + popularité ${decimal(s.popularity, 1)}`;
}

/** Facteurs qui réduisent le score : cas d'usage non couvert, estimation approximative. */
export function scoreFactors(s: Score): string[] {
  const lines: string[] = [];
  if (s.useCaseFactor < 1) {
    lines.push(`× ${decimal(s.useCaseFactor)} : ne correspond pas au cas d’usage`);
  }
  if (s.confidence < 1) {
    lines.push(`× ${decimal(s.confidence)} : estimation approximative`);
  }
  return lines;
}

/**
 * Détail d'un score pour l'infobulle (une ligne par élément), selon la formule du §9 :
 * `(moyenne pondérée × 0,95 + popularité) × facteur d'usage × confiance`.
 */
export function scoreTooltip(s: Score): string {
  return [
    `Score ${Math.round(s.total)} sur 100`,
    ...scoreAxes(s).map((a) => `${a.label} ${Math.round(a.value)} (poids ${decimal(a.weight)})`),
    scoreFormula(s),
    ...scoreFactors(s),
  ].join('\n');
}
