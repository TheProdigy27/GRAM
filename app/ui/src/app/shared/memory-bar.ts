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

/** Portion de la barre, en % (début et largeur). */
export interface BarRegion {
  start: number;
  width: number;
}

export interface MemoryBarModel {
  segments: BarSegment[];
  markers: BarMarker[];
  /** Part en RAM d'un modèle hybride (hachures), à la fin du besoin ; `null` sinon. */
  ram: BarRegion | null;
  /** Octets en RAM (mode hybride). */
  ramBytes: number;
  /** Besoin au-delà de la mémoire utilisable (modèle trop gros) ; `null` sinon. */
  over: BarRegion | null;
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

/** Le plus grand du besoin et de la mémoire de référence occupe 90 % de la barre. */
const FILL = 0.9;

/**
 * Barre mémoire de la fiche détail : besoin poste par poste (poids, cache KV, vision, surcoût)
 * sur une échelle qui contient la mémoire de référence du mode, la VRAM utilisable sur une
 * machine à GPU, avec un repère à cette limite. En hybride, la part en RAM est la fin du besoin,
 * au-delà de la VRAM (hachures). Un modèle trop gros montre aussi le repère de la mémoire
 * utilisable (VRAM + RAM) et ce qui la dépasse.
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
    : ['VRAM utilisable', 'VRAM + RAM utilisables'];

  let capacity: number;
  /** Mémoire que l'échelle doit contenir. */
  let reference: number;
  const marks: [string, number][] = [];
  switch (e.mode.kind) {
    case 'gpu':
      capacity = reference = budget.vram;
      marks.push([vramLabel, budget.vram]);
      break;
    case 'hybrid':
      capacity = combined;
      reference = budget.vram;
      marks.push([vramLabel, budget.vram]);
      break;
    case 'cpu':
      capacity = reference = budget.ram;
      marks.push(['RAM utilisable', budget.ram]);
      break;
    case 'tooBig':
      capacity = reference = gpu ? combined : budget.ram;
      if (gpu) marks.push([vramLabel, budget.vram]);
      marks.push([gpu ? combinedLabel : 'RAM utilisable', capacity]);
      break;
  }

  const scale = Math.max(total, reference, 1) / FILL;
  const pct = (v: number) => (v / scale) * 100;
  const ramBytes = e.mode.kind === 'hybrid' ? Math.min(Math.max(e.memCpu, 0), total) : 0;
  const overflow = total > capacity;
  return {
    segments: parts
      .filter(([, v]) => v > 0)
      .map(([key, bytes]) => ({ key, label: SEGMENT_LABELS[key], bytes, pct: pct(bytes) })),
    markers: marks.map(([label, bytes]) => ({ label, bytes, pct: pct(bytes) })),
    ram: ramBytes > 0 ? { start: pct(total - ramBytes), width: pct(ramBytes) } : null,
    ramBytes,
    over:
      e.mode.kind === 'tooBig' && overflow
        ? { start: pct(capacity), width: pct(total - capacity) }
        : null,
    total,
    capacity,
    overflow,
  };
}
