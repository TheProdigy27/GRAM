// Faux backend pour les tests : répond aux commandes IPC avec les fixtures, enregistre les
// appels, et simule les événements (`emit` de `@tauri-apps/api/event`).

import { mockIPC } from '@tauri-apps/api/mocks';
import type { InvokeArgs } from '@tauri-apps/api/core';
import { HARDWARE, RANKING, RUNTIMES, SETTINGS, SYNC_SUMMARY, makeDetail } from './fixtures';

export interface IpcCall {
  cmd: string;
  args: Record<string, unknown>;
}

export type Handler = (args: Record<string, unknown>) => unknown;

const DEFAULTS: Record<string, Handler> = {
  ping: () => ({ message: 'pong', appVersion: '0.1.0', coreVersion: '0.1.0' }),
  get_hardware: () => HARDWARE,
  rescan_hardware: () => HARDWARE,
  detect_runtimes: () => RUNTIMES,
  get_settings: () => SETTINGS,
  save_settings: (a) => a['settings'],
  rank: () => RANKING,
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
