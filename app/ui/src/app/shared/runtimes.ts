// État des runtimes pour l'affichage : bande « Votre machine » et filtre Runtime.

import type { Runtime } from '../core/generated/Runtime';
import type { RuntimesStatus } from '../core/generated/RuntimesStatus';
import { RUNTIMES } from './labels';

export type RuntimeState = 'running' | 'installed' | 'absent';

export const RUNTIME_STATE_LABELS: Record<RuntimeState, string> = {
  running: 'en cours',
  installed: 'installé',
  absent: 'absent',
};

export interface RuntimeRow {
  value: Runtime;
  label: string;
  state: RuntimeState;
  /** « en cours », « installé », « absent ». */
  stateLabel: string;
  /** Version exposée par le runtime (API locale d'Ollama) ; `null` si inconnue. */
  version: string | null;
  /** Version, modèles installés (Ollama) et chemin, une ligne chacun ; vide si rien n'est connu. */
  tooltip: string;
}

/** Les trois runtimes dans l'ordre de `RUNTIMES`, avec leur état détecté. */
export function runtimeRows(status: RuntimesStatus): RuntimeRow[] {
  return RUNTIMES.map(({ value, label }) => {
    const s = status[value];
    const state: RuntimeState = s.running ? 'running' : s.installed ? 'installed' : 'absent';
    const models = value === 'ollama' ? status.ollamaModels.length : 0;
    return {
      value,
      label,
      state,
      stateLabel: RUNTIME_STATE_LABELS[state],
      version: s.version,
      tooltip: [
        s.version && `version ${s.version}`,
        models > 0 && `${models} modèle${models > 1 ? 's' : ''} installé${models > 1 ? 's' : ''}`,
        s.path,
      ]
        .filter(Boolean)
        .join('\n'),
    };
  });
}
