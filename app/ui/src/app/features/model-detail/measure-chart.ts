// Graphiques « haltère » de la mesure réelle : la valeur mesurée (point plein) et l'estimation
// de GRAM (point creux) sur un même axe, reliées par un trait, avec l'écart en %.

import type { Measurement } from '../../core/generated/Measurement';
import type { MemoryBudget } from '../../core/generated/MemoryBudget';
import { niceAxis } from '../../shared/chart';
import { decimal, gbValue, int, signedPercent, tokS } from '../../shared/format';

export interface DumbbellPoint {
  /** Position, en % de l'axe. */
  pct: number;
  /** Valeur affichée près du point. */
  label: string;
  /** Côté du texte : à gauche du point le plus petit, à droite du plus grand. */
  side: 'left' | 'right';
}

export interface Dumbbell {
  key: 'speed' | 'memory';
  title: string;
  /** « +14 % » : (estimé − mesuré) / mesuré. */
  gap: string;
  measured: DumbbellPoint;
  estimated: DumbbellPoint;
  /** Trait entre les deux points, en % de l'axe. */
  span: { start: number; width: number };
  /** Graduations : début, milieu et fin de l'axe (avec l'unité). */
  ticks: { pct: number; label: string }[];
  /** Repère de la VRAM utilisable (graphique mémoire). */
  marker: { pct: number; label: string } | null;
  /** Phrase complète pour les lecteurs d'écran. */
  ariaLabel: string;
}

const GIB = 2 ** 30;

function between(a: DumbbellPoint, b: DumbbellPoint): Dumbbell['span'] {
  return { start: Math.min(a.pct, b.pct), width: Math.abs(a.pct - b.pct) };
}

function points(
  measured: number,
  estimated: number,
  max: number,
  labels: [string, string],
): [DumbbellPoint, DumbbellPoint] {
  const pct = (v: number) => (Math.min(Math.max(v, 0), max) / max) * 100;
  const lower = measured <= estimated;
  return [
    { pct: pct(measured), label: labels[0], side: lower ? 'left' : 'right' },
    { pct: pct(estimated), label: labels[1], side: lower ? 'right' : 'left' },
  ];
}

/** Trois graduations : 0, le milieu et la fin, l'unité sur la dernière. */
function ticks(max: number, unit: string, format: (v: number) => string) {
  return [0, max / 2, max].map((v, i) => ({
    pct: (v / max) * 100,
    label: i === 2 ? `${format(v)} ${unit}` : format(v),
  }));
}

/** Vitesse de génération mesurée et estimée. */
export function speedDumbbell(m: Measurement): Dumbbell {
  const axis = niceAxis(Math.max(m.tokS, m.estimatedTokS) * 1.15, 4);
  const measuredLabel = decimal(m.tokS, 1);
  const estimatedLabel = tokS(m.estimatedTokS).replace('~', '');
  const [measured, estimated] = points(m.tokS, m.estimatedTokS, axis.max, [
    measuredLabel,
    estimatedLabel,
  ]);
  const gap = signedPercent(m.tokSError);
  return {
    key: 'speed',
    title: 'Vitesse de génération',
    gap,
    measured,
    estimated,
    span: between(measured, estimated),
    ticks: ticks(axis.max, 'tok/s', (v) => int(v)),
    marker: null,
    ariaLabel: `Vitesse mesurée ${measuredLabel} tokens par seconde, estimée ${estimatedLabel} : écart ${gap}.`,
  };
}

/**
 * Mémoire occupée d'après Ollama et estimée, avec le repère de la VRAM utilisable ; `null` si
 * Ollama n'a pas donné la mémoire.
 */
export function memoryDumbbell(m: Measurement, budget: MemoryBudget): Dumbbell | null {
  if (m.memTotal === null) return null;
  const vram = budget.vram > 0 ? budget.vram : null;
  const top = Math.max(m.memTotal, m.estimatedMemTotal, vram ?? 0) * 1.12;
  const axis = niceAxis(top / GIB, 4);
  const max = axis.max * GIB;
  const [measured, estimated] = points(m.memTotal, m.estimatedMemTotal, max, [
    gbValue(m.memTotal),
    gbValue(m.estimatedMemTotal),
  ]);
  const gap = signedPercent(m.memError);
  const vramName = budget.unified ? 'Mémoire du GPU' : 'VRAM';
  return {
    key: 'memory',
    title: 'Mémoire occupée',
    gap,
    measured,
    estimated,
    span: between(measured, estimated),
    ticks: ticks(axis.max, 'Go', (v) => decimal(v, 1)),
    marker: vram ? { pct: (vram / max) * 100, label: `${vramName} ${gbValue(vram)}` } : null,
    ariaLabel:
      `Mémoire mesurée ${gbValue(m.memTotal)} Go, estimée ${gbValue(m.estimatedMemTotal)} Go : écart ${gap}.` +
      (vram ? ` ${vramName} utilisable : ${gbValue(vram)} Go.` : ''),
  };
}
