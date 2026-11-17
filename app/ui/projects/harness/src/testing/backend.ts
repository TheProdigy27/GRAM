// Faux backend de GRAM Harness pour les tests et la vérification visuelle : répond aux
// commandes IPC avec les fixtures (la discussion par `FakeChat`), enregistre les appels et
// simule les événements (`emit` de `@tauri-apps/api/event`).

import type { InvokeArgs } from '@tauri-apps/api/core';
import { mockIPC } from '@tauri-apps/api/mocks';
import { FakeChat } from './chat';
import { MODELS, UP_TO_DATE } from './fixtures';

export interface IpcCall {
  cmd: string;
  args: Record<string, unknown>;
}

export type Handler = (args: Record<string, unknown>) => unknown;

const DEFAULTS: Record<string, Handler> = {
  list_models: () => MODELS,
  open_ollama_download: () => null,
  take_opened_model: () => null,
  check_update: () => UP_TO_DATE,
  install_update: () => null,
  'plugin:app|version': () => UP_TO_DATE.currentVersion,
  'plugin:window|set_theme': () => null,
  'plugin:clipboard-manager|write_text': () => null,
  'plugin:opener|open_url': () => null,
};

/**
 * Installe le faux backend ; `chat` répond aux commandes de la discussion et des conversations,
 * `handlers` remplace des réponses (une erreur = `throw`).
 */
export function mockBackend(
  handlers: Record<string, Handler> = {},
  chat: FakeChat = new FakeChat(),
): IpcCall[] {
  const calls: IpcCall[] = [];
  const all = { ...DEFAULTS, ...chat.handlers(), ...handlers };
  mockIPC(
    (cmd: string, args?: InvokeArgs) => {
      const a = (args ?? {}) as Record<string, unknown>;
      calls.push({ cmd, args: a });
      const handler = all[cmd];
      if (!handler) throw new Error(`commande non simulée : ${cmd}`);
      return handler(a);
    },
    // Événements (`emit` de `@tauri-apps/api/event`) : lien profond `open-model`.
    { shouldMockEvents: true },
  );
  return calls;
}

/** Nombre d'appels à une commande. */
export function countOf(calls: IpcCall[], cmd: string): number {
  return calls.filter((c) => c.cmd === cmd).length;
}
