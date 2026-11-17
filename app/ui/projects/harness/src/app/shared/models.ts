// Mise en forme des modèles : options du sélecteur, capacités, faits affichés.

import { ctxK, fileSize, tokS } from '@gram/format';
import type { IconName } from '@gram/ui/icon/icons';
import type { SelectOption } from '@gram/ui/select/select';
import type { HarnessModel } from '@gram/types/HarnessModel';

export interface Capability {
  key: 'vision' | 'thinking' | 'tools';
  label: string;
  icon: IconName;
  /** Infobulle : ce que la capacité permet. */
  description: string;
}

const CAPABILITIES: readonly Capability[] = [
  { key: 'vision', label: 'Vision', icon: 'eye', description: 'Accepte des images' },
  {
    key: 'thinking',
    label: 'Réflexion',
    icon: 'bulb',
    description: 'Peut réfléchir avant de répondre',
  },
  { key: 'tools', label: 'Outils', icon: 'settings', description: 'Sait appeler des outils' },
];

/** Même modèle pour Ollama, qui ne tient pas compte de la casse des noms. */
export function sameModelName(a: string, b: string): boolean {
  return a.trim().toLowerCase() === b.trim().toLowerCase();
}

/** Capacités du modèle, dans un ordre fixe. */
export function capabilities(m: HarnessModel): Capability[] {
  return CAPABILITIES.filter((c) => m[c.key]);
}

/** Groupes du sélecteur. */
export const GRAM_GROUP = 'Ajoutés depuis GRAM';
export const OTHER_GROUP = 'Autres modèles d’Ollama';

/**
 * Options du sélecteur : modèles de GRAM (nom lisible) puis les autres (nom technique), dans
 * l'ordre du backend ; quantification et taille en précision, capacités en étiquettes.
 */
export function modelOptions(models: readonly HarnessModel[]): SelectOption<string>[] {
  return models.map((m) => ({
    value: m.name,
    label: m.fromGram && m.quant ? `${m.displayName} ${m.quant}` : m.displayName,
    hint: fileSize(m.size),
    tags: capabilities(m).map((c) => c.label.toLowerCase()),
    group: m.fromGram ? GRAM_GROUP : OTHER_GROUP,
  }));
}

/**
 * Faits d'un modèle, dans l'ordre : quantification, taille, contexte, vitesses estimée et
 * mesurée par GRAM (« Q4_K_M », « 4,7 Go », « contexte 16k », « ~45 tok/s estimés »…).
 */
export function modelFacts(m: HarnessModel): string[] {
  return [
    m.quant,
    fileSize(m.size),
    m.ctx ? `contexte ${ctxK(m.ctx)}` : 'contexte choisi par Ollama',
    m.estimatedTokS ? `${tokS(m.estimatedTokS)} tok/s estimés` : null,
    m.measuredTokS ? `${tokS(m.measuredTokS)} tok/s mesurés` : null,
  ].filter((f): f is string => !!f);
}
