// Faux backend de GRAM Harness pour les tests et la vérification visuelle : répond aux
// commandes IPC avec les fixtures et enregistre les appels.

import type { InvokeArgs } from '@tauri-apps/api/core';
import { mockIPC } from '@tauri-apps/api/mocks';
import { MODELS } from './fixtures';

export interface IpcCall {
  cmd: string;
  args: Record<string, unknown>;
}

export type Handler = (args: Record<string, unknown>) => unknown;

const DEFAULTS: Record<string, Handler> = {
  list_models: () => MODELS,
  open_ollama_download: () => null,
  'plugin:window|set_theme': () => null,
};

/** Installe le faux backend ; `handlers` remplace des réponses (une erreur = `throw`). */
export function mockBackend(handlers: Record<string, Handler> = {}): IpcCall[] {
  const calls: IpcCall[] = [];
  const all = { ...DEFAULTS, ...handlers };
  mockIPC((cmd: string, args?: InvokeArgs) => {
    const a = (args ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args: a });
    const handler = all[cmd];
    if (!handler) throw new Error(`commande non simulée : ${cmd}`);
    return handler(a);
  });
  return calls;
}

/** Nombre d'appels à une commande. */
export function countOf(calls: IpcCall[], cmd: string): number {
  return calls.filter((c) => c.cmd === cmd).length;
}
