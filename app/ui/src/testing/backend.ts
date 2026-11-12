// Faux backend pour les tests : répond aux commandes IPC avec les fixtures, enregistre les
// appels, et simule les événements (`emit` de `@tauri-apps/api/event`).

import { mockIPC } from '@tauri-apps/api/mocks';
import type { InvokeArgs } from '@tauri-apps/api/core';
import type { HardwareProfile } from '../app/core/generated/HardwareProfile';
import type { RankRequest } from '../app/core/generated/RankRequest';
import type { RankResponse } from '../app/core/generated/RankResponse';
import { matchesWords, searchWords } from '../app/features/palette/palette-model';
import {
  CACHE_INFO,
  CALIBRATION_EMPTY,
  HARDWARE,
  HARDWARE_OPTIONS,
  MEASUREMENT,
  RANKING,
  RUNTIMES,
  SETTINGS,
  SYNC_SUMMARY,
  makeDetail,
  makeResponse,
} from './fixtures';

export interface IpcCall {
  cmd: string;
  args: Record<string, unknown>;
}

export type Handler = (args: Record<string, unknown>) => unknown;

const DEFAULTS: Record<string, Handler> = {
  ping: () => ({ message: 'pong', appVersion: '0.1.0', coreVersion: '0.1.0' }),
  get_hardware: () => HARDWARE,
  rescan_hardware: () => HARDWARE,
  // Le profil renvoyé porte les corrections demandées (sans les appliquer aux valeurs).
  set_hardware_overrides: (a) =>
    ({
      ...HARDWARE,
      overrides: { ...HARDWARE.overrides, ...(a['overrides'] as object) },
    }) satisfies HardwareProfile,
  hardware_options: () => HARDWARE_OPTIONS,
  detect_runtimes: () => RUNTIMES,
  get_settings: () => SETTINGS,
  get_default_settings: () => SETTINGS,
  save_settings: (a) => a['settings'],
  has_hf_token: () => false,
  set_hf_token: () => null,
  clear_hf_token: () => null,
  get_cache_info: () => CACHE_INFO,
  clear_cache: () => ({ ...CACHE_INFO, files: 0, bytes: 0 }),
  rank: () => RANKING,
  search_ranking: (a) => searchRanking(a['request'] as RankRequest),
  // Sans chemin, le vrai backend ouvre « Enregistrer sous » : ici, un chemin choisi d'office.
  export_ranking: (a) =>
    (a['path'] as string | undefined) ??
    `C:\\Users\\x\\Documents\\classement-gram.${a['format'] as string}`,
  sync_catalog: () => SYNC_SUMMARY,
  get_model_detail: () => makeDetail(),
  get_calibration: () => CALIBRATION_EMPTY,
  ollama_measure: () => MEASUREMENT,
  apply_calibration: () => ({ ...SETTINGS, estimate: { ...SETTINGS.estimate, gpuEfficiency: 0.48 } }),
  clear_measurements: () => CALIBRATION_EMPTY,
  check_update: () => ({ currentVersion: '0.1.0', update: null }),
  install_update: () => new Promise(() => undefined), // l'application redémarre
  'plugin:clipboard-manager|write_text': () => null,
  'plugin:opener|open_url': () => null,
};

/**
 * `search_ranking` (palette) sur le classement des fixtures : la recherche garde les lignes dont
 * le nom, le dépôt ou le modèle de base contient chaque mot, `limit` coupe la liste.
 */
export function searchRanking(request: RankRequest, ranking: RankResponse = RANKING): RankResponse {
  const words = searchWords(request.search ?? '');
  const items = ranking.items.filter((i) =>
    matchesWords(words, [i.displayName, i.repoId, i.baseModel ?? '']),
  );
  return makeResponse(items.slice(0, request.limit ?? items.length), {
    totalItems: items.length,
    search: words.length > 0 ? words.join(' ') : null,
  });
}

/** Installe le faux backend ; `handlers` remplace des réponses (une erreur = `throw`). */
export function mockBackend(handlers: Record<string, Handler> = {}): IpcCall[] {
  const calls: IpcCall[] = [];
  const all = { ...DEFAULTS, ...handlers };
  mockIPC(
    (cmd: string, args?: InvokeArgs) => {
      const a = (args ?? {}) as Record<string, unknown>;
      calls.push({ cmd, args: a });
      const handler = all[cmd];
      if (!handler) throw new Error(`commande non simulée : ${cmd}`);
      return handler(a);
    },
    { shouldMockEvents: true },
  );
  return calls;
}

/** Arguments des appels à une commande. */
export function argsOf(calls: IpcCall[], cmd: string): Record<string, unknown>[] {
  return calls.filter((c) => c.cmd === cmd).map((c) => c.args);
}
