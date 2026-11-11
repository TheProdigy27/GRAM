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
  /** Mémoire utilisable dans le mode (VRAM, VRAM + RAM, ou RAM ; en mémoire unifiée, la RAM
   * contient la part du GPU). */
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
 *
 * En mémoire unifiée (Mac Apple Silicon), la mémoire du GPU est prise sur la RAM : la mémoire
 * du mode hybride est la seule RAM, comme `MemoryBudget::combined` côté Rust.
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
  const combined = budget.unified ? Math.max(budget.ram, budget.vram) : budget.vram + budget.ram;
  const [vramLabel, combinedLabel] = budget.unified
    ? ['Mémoire du GPU', 'RAM utilisable (GPU compris)']
    : ['VRAM', 'VRAM + RAM'];

  let capacity: number;
  const marks: [string, number][] = [];
  switch (e.mode.kind) {
    case 'gpu':
      capacity = budget.vram;
      marks.push([budget.unified ? 'Mémoire du GPU' : 'VRAM utilisable', budget.vram]);
      break;
    case 'hybrid':
      capacity = combined;
      marks.push([vramLabel, budget.vram], [combinedLabel, capacity]);
      break;
    case 'cpu':
      capacity = budget.ram;
      marks.push(['RAM utilisable', budget.ram]);
      break;
    case 'tooBig':
      capacity = gpu ? combined : budget.ram;
      if (gpu) marks.push([vramLabel, budget.vram]);
      marks.push([gpu ? combinedLabel : 'RAM utilisable', capacity]);
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
