// Suggestion de la fiche : la quantification choisie déborde en RAM (ou tourne sur le CPU)
// alors qu'une autre tient entièrement en VRAM, bien plus vite.

import type { VariantDetail } from '../../core/generated/VariantDetail';
import { ctxK, signedPercent, tokS } from '../../shared/format';

/** Gain de vitesse minimal pour suggérer une autre quantification. */
const MIN_GAIN = 0.1;

export interface GpuSuggestion {
  /** Quantification suggérée. */
  label: string;
  /** « ~53 tok/s ». */
  speed: string;
  /**
   * Fin de la phrase, espace initiale comprise : « (+60 %) pour une qualité à peine moindre,
   * mais 10k de contexte au plus. » (sans le gain si la vitesse actuelle est inconnue).
   */
  after: string;
}

/** Qualité perdue (axe qualité du score, de 0 à 100) dite en mots. */
function qualityWords(loss: number): { text: string; contrast: boolean } {
  if (loss <= 2) return { text: 'pour une qualité quasi identique', contrast: false };
  if (loss <= 6) return { text: 'pour une qualité à peine moindre', contrast: false };
  if (loss <= 12) return { text: 'pour une qualité un peu moindre', contrast: false };
  return { text: 'mais une qualité nettement moindre', contrast: true };
}

/**
 * Si la quantification choisie est hybride (ou sur le CPU) et qu'une quantification tient
 * entièrement en VRAM, suggère la plus grosse de celles-ci (la plus fidèle), de préférence sous
 * la taille maximale demandée, quand elle est au moins 10 % plus rapide.
 */
export function gpuSuggestion(
  variants: readonly VariantDetail[],
  selected: string | null,
): GpuSuggestion | null {
  const current = variants.find((v) => v.variant.label === selected);
  const kind = current?.estimate.mode.kind;
  if (!current || (kind !== 'hybrid' && kind !== 'cpu')) return null;

  const gpu = variants.filter((v) => v.estimate.mode.kind === 'gpu');
  const pool = gpu.some((v) => v.withinSize) ? gpu.filter((v) => v.withinSize) : gpu;
  if (pool.length === 0) return null;
  const pick = pool.reduce((a, b) => (b.variant.sizeBytes > a.variant.sizeBytes ? b : a));

  const before = current.estimate.tokS;
  const after = pick.estimate.tokS;
  if (after <= 0) return null;
  const gain = before > 0 ? after / before - 1 : null;
  if (gain !== null && gain < MIN_GAIN) return null;

  const quality = qualityWords(current.score.quality - pick.score.quality);
  const shorter = pick.estimate.ctxMax < current.estimate.ctxMax;
  const context = shorter
    ? `${quality.contrast ? ' et' : ', mais'} ${ctxK(pick.estimate.ctxMax)} de contexte au plus`
    : '';
  const gainText = gain === null ? '' : ` (${signedPercent(gain)})`;
  return {
    label: pick.variant.label,
    speed: `${tokS(after)} tok/s`,
    after: `${gainText} ${quality.text}${context}.`,
  };
}
