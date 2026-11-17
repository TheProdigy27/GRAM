// Libellés français des énumérations venues du backend.

import type { DataSourceKind } from '../core/generated/DataSourceKind';
import type { FitLevel } from '../core/generated/FitLevel';
import type { PickKind } from '../core/generated/PickKind';
import type { RamKind } from '../core/generated/RamKind';
import type { RunMode } from '../core/generated/RunMode';
import type { Runtime } from '../core/generated/Runtime';
import type { Source } from '../core/generated/Source';
import type { SyncPhase } from '../core/generated/SyncPhase';
import type { UseCase } from '../core/generated/UseCase';
import type { IconName } from './ui/icon/icons';

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

/** Icône de chaque usage (filtres, palette). */
export const USE_CASE_ICONS: Record<UseCase, IconName> = {
  general: 'grid',
  code: 'code',
  reasoning: 'bulb',
  fastChat: 'bolt',
  vision: 'eye',
};

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

/** Nom des trois conseils, dans leur ordre d'affichage. */
export const PICK_LABELS: Record<PickKind, string> = {
  best: 'Meilleur choix',
  fastest: 'Le plus rapide',
  longestContext: 'Le plus long contexte',
};

/**
 * Ressenti d'une vitesse de génération : « instantané » dès 80 tok/s, « très fluide » dès 30,
 * « fluide » dès 15, « correct » jusqu'au seuil de lenteur du cas d'usage, « lent » en dessous.
 */
export function speedWord(tokS: number, slowTokS: number): string {
  if (tokS >= 80) return 'instantané';
  if (tokS >= 30) return 'très fluide';
  if (tokS >= 15) return 'fluide';
  if (tokS >= slowTokS) return 'correct';
  return 'lent';
}

/**
 * Nom d'une quantification à l'affichage. Un fichier au nom exotique a pour libellé le nom du
 * fichier entier (« Huihui-Qwen3.8-27B-abliterated-Underdog-Saluki-Swift-1.5-IQ2 ») : le nom du
 * modèle, que le dépôt porte déjà, en est retiré (« Underdog-Saluki-Swift-1.5-IQ2 »). Le libellé
 * complet reste l'identifiant de la quantification (commandes, choix, infobulles).
 */
export function quantName(variant: { label: string; repoId: string }): string {
  const { label, repoId } = variant;
  const model = repoId.slice(repoId.lastIndexOf('/') + 1).replace(/[-_.]gguf$/i, '');
  if (model === '' || label.length <= model.length) return label;
  if (label.slice(0, model.length).toLowerCase() !== model.toLowerCase()) return label;
  if (!/[-_. ]/.test(label[model.length])) return label;
  const rest = label.slice(model.length).replace(/^[-_. ]+/, '');
  return rest === '' ? label : rest;
}
