// Libellés français des énumérations venues du backend.

import type { DataSourceKind } from '../core/generated/DataSourceKind';
import type { FitLevel } from '../core/generated/FitLevel';
import type { RamKind } from '../core/generated/RamKind';
import type { RunMode } from '../core/generated/RunMode';
import type { Runtime } from '../core/generated/Runtime';
import type { Source } from '../core/generated/Source';
import type { SyncPhase } from '../core/generated/SyncPhase';
import type { UseCase } from '../core/generated/UseCase';

export const USE_CASES: readonly { value: UseCase; label: string }[] = [
  { value: 'general', label: 'Polyvalent' },
  { value: 'code', label: 'Code' },
  { value: 'reasoning', label: 'Raisonnement' },
  { value: 'fastChat', label: 'Chat rapide' },
  { value: 'vision', label: 'Vision' },
];

export const RUNTIMES: readonly { value: Runtime; label: string }[] = [
  { value: 'ollama', label: 'Ollama' },
  { value: 'lmStudio', label: 'LM Studio' },
  { value: 'llamaCpp', label: 'llama.cpp' },
];

export function useCaseLabel(u: UseCase): string {
  return USE_CASES.find((x) => x.value === u)?.label ?? u;
}

export function runtimeLabel(r: Runtime): string {
  return RUNTIMES.find((x) => x.value === r)?.label ?? r;
}

export const MODE_LABELS: Record<RunMode['kind'], string> = {
  gpu: 'GPU',
  hybrid: 'Hybride',
  cpu: 'CPU',
  tooBig: 'Trop gros',
};

export const FIT_LABELS: Record<FitLevel, string> = {
  perfect: 'Parfait',
  good: 'Bon',
  marginal: 'Limite',
  tooTight: 'Trop juste',
};

/** Origine d'une valeur matérielle, affichée à côté de celle-ci. */
export const SOURCE_LABELS: Record<Source, string> = {
  detected: 'détectée',
  nvml: 'NVML',
  table: 'table de référence',
  estimated: 'estimée',
  default: 'valeur par défaut',
  manual: 'corrigée',
};

export const RAM_KIND_LABELS: Record<RamKind, string> = {
  ddr3: 'DDR3',
  ddr4: 'DDR4',
  ddr5: 'DDR5',
  lpddr4: 'LPDDR4',
  lpddr4x: 'LPDDR4X',
  lpddr5: 'LPDDR5',
  lpddr5x: 'LPDDR5X',
  unknown: '',
};

export const DATA_SOURCE_LABELS: Record<DataSourceKind, string> = {
  online: 'en ligne',
  cache: 'cache local',
  snapshot: 'catalogue embarqué',
};

export const SYNC_PHASE_LABELS: Record<SyncPhase, string> = {
  listing: 'Liste des modèles',
  details: 'Tailles des fichiers',
  architecture: 'Architectures',
};

/** Ordre des modes pour le tri : du plus confortable au moins confortable. */
export const MODE_ORDER: Record<RunMode['kind'], number> = { gpu: 0, hybrid: 1, cpu: 2, tooBig: 3 };
