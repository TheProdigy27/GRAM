import type { Estimate } from '../core/generated/Estimate';
import type { MemoryBudget } from '../core/generated/MemoryBudget';

export type SegmentKey = 'weights' | 'kv' | 'vision' | 'overhead';

export interface BarSegment {
  key: SegmentKey;
  label: string;
  bytes: number;
  /** Largeur, en % de la barre. */
  pct: number;
}

export interface BarMarker {
  label: string;
  bytes: number;
  /** Position, en % de la barre. */
  pct: number;
}

export interface MemoryBarModel {
  segments: BarSegment[];
  markers: BarMarker[];
  /** Besoin total au contexte retenu. */
  total: number;
  /** Mémoire utilisable dans le mode (VRAM, VRAM + RAM, ou RAM). */
  capacity: number;
  /** Le besoin dépasse la mémoire utilisable. */
  overflow: boolean;
}

const SEGMENT_LABELS: Record<SegmentKey, string> = {
  weights: 'Poids',
  kv: 'Cache KV',
  vision: 'Projecteur vision',
  overhead: 'Surcoût',
};

/**
 * Barre mémoire de la fiche détail : besoin poste par poste face à la mémoire utilisable du
 * mode. L'échelle est le plus grand des deux, pour qu'un dépassement reste visible. En hybride
 * (et pour un modèle trop gros, s'il y a un GPU), un repère marque la fin de la VRAM.
 */
export function memoryBar(e: Estimate, budget: MemoryBudget): MemoryBarModel {
  const b = e.breakdown;
  const parts: [SegmentKey, number][] = [
    ['weights', b.weights],
    ['kv', b.kv],
    ['vision', b.vision],
    ['overhead', b.overhead],
  ];
  const total = parts.reduce((sum, [, v]) => sum + v, 0);
  const gpu = budget.vram > 0;

  let capacity: number;
  const marks: [string, number][] = [];
  switch (e.mode.kind) {
    case 'gpu':
      capacity = budget.vram;
      marks.push(['VRAM utilisable', budget.vram]);
      break;
    case 'hybrid':
      capacity = budget.vram + budget.ram;
      marks.push(['VRAM', budget.vram], ['VRAM + RAM', capacity]);
      break;
    case 'cpu':
      capacity = budget.ram;
      marks.push(['RAM utilisable', budget.ram]);
      break;
    case 'tooBig':
      capacity = gpu ? budget.vram + budget.ram : budget.ram;
      if (gpu) marks.push(['VRAM', budget.vram]);
      marks.push([gpu ? 'VRAM + RAM' : 'RAM utilisable', capacity]);
      break;
  }

  const scale = Math.max(total, capacity, 1);
  const pct = (v: number) => (v / scale) * 100;
  return {
    segments: parts
      .filter(([, v]) => v > 0)
      .map(([key, bytes]) => ({ key, label: SEGMENT_LABELS[key], bytes, pct: pct(bytes) })),
    markers: marks.map(([label, bytes]) => ({ label, bytes, pct: pct(bytes) })),
    total,
    capacity,
    overflow: total > capacity,
  };
}
