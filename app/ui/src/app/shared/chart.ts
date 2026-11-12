// Axes des petits graphiques de la fiche (vitesse par quantification, mesure réelle).

/** Pas « ronds » d'un axe, multipliés par une puissance de 10. */
const STEPS = [1, 2, 2.5, 5, 10];

export interface NiceAxis {
  /** Fin de l'axe (0 en est le début). */
  max: number;
  step: number;
  /** Graduations de 0 à `max`. */
  ticks: number[];
}

/**
 * Axe de 0 à un maximum « rond » qui contient `value`, en au plus `maxIntervals` intervalles de
 * 1, 2, 2,5 ou 5 × 10ⁿ : 82 → 0, 25, 50, 75, 100 ; 9,7 → 0, 2,5, 5, 7,5, 10. Une valeur nulle
 * ou invalide donne l'axe de 0 à 1.
 */
export function niceAxis(value: number, maxIntervals = 4): NiceAxis {
  const v = value > 0 && Number.isFinite(value) ? value : 1;
  const raw = v / Math.max(maxIntervals, 1);
  const magnitude = 10 ** Math.floor(Math.log10(raw));
  const step = magnitude * (STEPS.find((s) => s * magnitude >= raw * (1 - 1e-9)) ?? 10);
  const count = Math.max(Math.ceil(v / step - 1e-9), 1);
  const ticks = Array.from({ length: count + 1 }, (_, i) => Math.round(i * step * 1e6) / 1e6);
  return { max: ticks[count], step, ticks };
}
