// Empreinte d'une ligne du classement, sur l'échelle de la carte mémoire (0 → VRAM + RAM
// utilisables) : on voit d'un coup d'œil ce qui tient en VRAM.

import type { Estimate } from '../../core/generated/Estimate';
import type { MemoryBudget } from '../../core/generated/MemoryBudget';
import { gb, int } from '../../shared/format';
import { type Segment, footprint, memoryScale } from '../../shared/memory-scale';

const GIB = 2 ** 30;

export interface RowFootprint {
  /** Fin de la zone VRAM teintée, en fraction de l'échelle (toute la barre en mémoire unifiée). */
  vramEnd: number;
  unified: boolean;
  /** Repère « GPU jusqu'à X Go » en mémoire unifiée. */
  gpuLimit: number | null;
  /** Part en VRAM (part du GPU en mémoire unifiée). */
  gpu: Segment | null;
  ram: Segment | null;
  /** Modèle trop gros : mémoire nécessaire, rognée à la fin de l'échelle. */
  over: Segment | null;
  /** « 13,9 Go en VRAM + 7,6 Go en RAM » (infobulle et lecteurs d'écran). */
  label: string;
}

/** « 0–62 Go » : l'échelle de la colonne, la même que celle de la carte mémoire. */
export function scaleLabel(budget: MemoryBudget): string {
  return `0–${int(Math.round(memoryScale(budget).total / GIB))} Go`;
}

export function rowFootprint(budget: MemoryBudget, e: Estimate): RowFootprint {
  const scale = memoryScale(budget);
  const base = {
    vramEnd: scale.unified ? 1 : scale.vramEnd,
    unified: scale.unified,
    gpuLimit: scale.gpuLimit,
  };

  if (e.mode.kind === 'tooBig') {
    const b = e.breakdown;
    const need = b.weights + b.kv + b.overhead + b.vision;
    const width = scale.total > 0 ? Math.min(need / scale.total, 1) : 1;
    return {
      ...base,
      gpu: null,
      ram: null,
      over: width > 0 ? { start: 0, width } : null,
      label: `Environ ${gb(need)} nécessaires, pour ${gb(scale.total)} utilisables`,
    };
  }

  const fp = footprint(scale, e.memGpu, e.memCpu);
  const [gpuPart, cpuPart] = scale.unified ? ['pour le GPU', 'pour le CPU'] : ['en VRAM', 'en RAM'];
  const parts = [
    e.memGpu > 0 ? `${gb(e.memGpu)} ${gpuPart}` : '',
    e.memCpu > 0 ? `${gb(e.memCpu)} ${cpuPart}` : '',
  ].filter(Boolean);
  return {
    ...base,
    gpu: fp.gpu,
    ram: fp.ram,
    over: null,
    label:
      (parts.join(' + ') || 'Aucune mémoire') +
      (fp.overflow ? ' : plus que la mémoire utilisable' : ''),
  };
}
