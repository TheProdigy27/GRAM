// Faux backend pour les tests : répond aux commandes IPC avec les fixtures, enregistre les
// appels, et simule les événements (`emit` de `@tauri-apps/api/event`).

import { mockIPC } from '@tauri-apps/api/mocks';
import type { InvokeArgs } from '@tauri-apps/api/core';
import type { HardwareProfile } from '../app/core/generated/HardwareProfile';
import {
  CACHE_INFO,
  HARDWARE,
  HARDWARE_OPTIONS,
  RANKING,
  RUNTIMES,
  SETTINGS,
  SYNC_SUMMARY,
  makeDetail,
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
  // Sans chemin, le vrai backend ouvre « Enregistrer sous » : ici, un chemin choisi d'office.
  export_ranking: (a) =>
    (a['path'] as string | undefined) ??
    `C:\\Users\\x\\Documents\\classement-gram.${a['format'] as string}`,
  sync_catalog: () => SYNC_SUMMARY,
  get_model_detail: () => makeDetail(),
  'plugin:clipboard-manager|write_text': () => null,
  'plugin:opener|open_url': () => null,
};

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
