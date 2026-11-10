import type { Score } from '../core/generated/Score';
import { decimal } from './format';

/**
 * Détail d'un score pour l'infobulle (une ligne par élément), selon la formule du §9 :
 * `(moyenne pondérée × 0,95 + popularité) × facteur d'usage × confiance`.
 */
export function scoreTooltip(s: Score): string {
  const w = s.weights;
  const lines = [
    `Score ${Math.round(s.total)} sur 100`,
    `Qualité ${Math.round(s.quality)} (poids ${decimal(w.quality)})`,
    `Vitesse ${Math.round(s.speed)} (poids ${decimal(w.speed)})`,
    `Marge mémoire ${Math.round(s.fit)} (poids ${decimal(w.fit)})`,
    `Contexte ${Math.round(s.context)} (poids ${decimal(w.context)})`,
    `Moyenne pondérée ${decimal(s.weighted, 1)} × 0,95 + popularité ${decimal(s.popularity, 1)}`,
  ];
  if (s.useCaseFactor < 1) {
    lines.push(`× ${decimal(s.useCaseFactor)} : ne correspond pas au cas d’usage`);
  }
  if (s.confidence < 1) {
    lines.push(`× ${decimal(s.confidence)} : estimation approximative`);
  }
  return lines.join('\n');
}
