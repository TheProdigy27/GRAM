// Échelle mémoire des empreintes du classement (même échelle pour toutes les lignes) :
// de 0 à « VRAM + RAM utilisables », zone VRAM d'abord (rapide), puis zone RAM. En mémoire
// unifiée (Mac Apple Silicon), une seule zone de la taille de la RAM : VRAM et RAM ne
// s'additionnent pas.

import type { MemoryBudget } from '../core/generated/MemoryBudget';

export interface MemoryScale {
  /** Fin de l'échelle, en octets. */
  total: number;
  /** Fin de la zone VRAM, en fraction de l'échelle ; 0 sans zone VRAM (pas de GPU, unifiée). */
  vramEnd: number;
  /** Repère « GPU jusqu'à X Go » en mémoire unifiée, en fraction de l'échelle. */
  gpuLimit: number | null;
  unified: boolean;
}

/** Portion de l'échelle, en fractions (0 à 1). */
export interface Segment {
  start: number;
  width: number;
}

export interface Footprint {
  /** Part en VRAM (ou part du GPU en mémoire unifiée). */
  gpu: Segment | null;
  /** Part en RAM. */
  ram: Segment | null;
  /** L'empreinte dépasse la mémoire utilisable (modèle trop gros) : segments rognés. */
  overflow: boolean;
}

export function memoryScale(b: MemoryBudget): MemoryScale {
  if (b.unified) {
    const total = Math.max(b.ram, 0);
    return {
      total,
      vramEnd: 0,
      gpuLimit: total > 0 && b.vram > 0 ? Math.min(b.vram / total, 1) : null,
      unified: true,
    };
  }
  const vram = Math.max(b.vram, 0);
  const total = vram + Math.max(b.ram, 0);
  return { total, vramEnd: total > 0 ? vram / total : 0, gpuLimit: null, unified: false };
}

/**
 * Place une empreinte (octets en VRAM, octets en RAM) sur l'échelle : la part en VRAM part de
 * 0, la part en RAM part du début de la zone RAM (juste après la part du GPU en mémoire
 * unifiée).
 */
export function footprint(scale: MemoryScale, memGpu: number, memCpu: number): Footprint {
  if (scale.total <= 0) return { gpu: null, ram: null, overflow: memGpu + memCpu > 0 };
  const gpuWidth = Math.max(memGpu, 0) / scale.total;
  const ramWidth = Math.max(memCpu, 0) / scale.total;
  const ramStart = scale.unified ? Math.min(gpuWidth, 1) : scale.vramEnd;
  const gpuLimit = scale.unified ? 1 : scale.vramEnd;
  const overflow = gpuWidth > gpuLimit + 1e-9 || ramStart + ramWidth > 1 + 1e-9;
  const clip = (start: number, width: number, end: number): Segment | null => {
    const w = Math.min(width, end - start);
    return w > 0 ? { start, width: w } : null;
  };
  return {
    gpu: clip(0, gpuWidth, Math.max(gpuLimit, 0)),
    ram: clip(ramStart, ramWidth, 1),
    overflow,
  };
}
