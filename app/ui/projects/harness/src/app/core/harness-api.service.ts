import { Injectable } from '@angular/core';
import { invoke, type InvokeArgs } from '@tauri-apps/api/core';
import type { Theme } from '@gram/theme';
import type { ModelList } from '@gram/types/ModelList';

/** Message lisible d'une erreur : le backend rejette `invoke` avec une chaîne en français. */
export function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === 'string') return e;
  return String(e);
}

/**
 * Seul point d'accès à l'IPC Tauri de GRAM Harness (`app/harness-tauri/src/commands.rs`). Les
 * types viennent des types générés de GRAM. Toutes les promesses rejetées le sont avec une
 * `Error` dont le message est en français.
 */
@Injectable({ providedIn: 'root' })
export class HarnessApi {
  /**
   * Modèles avec lesquels discuter (ceux de GRAM d'abord) et état d'Ollama : arrêté ou absent,
   * la liste est vide.
   */
  listModels(): Promise<ModelList> {
    return this.call('list_models');
  }

  /** Ouvre la page de téléchargement d'Ollama dans le navigateur du système. */
  openOllamaDownload(): Promise<void> {
    return this.call('open_ollama_download');
  }

  /** Thème de la fenêtre (barre de titre native) ; `null` : celui du système. */
  setWindowTheme(theme: Theme | null): Promise<void> {
    return this.call('plugin:window|set_theme', { label: 'main', value: theme });
  }

  private call<T>(cmd: string, args?: InvokeArgs): Promise<T> {
    return invoke<T>(cmd, args).catch((e: unknown) => {
      throw e instanceof Error ? e : new Error(errorMessage(e));
    });
  }
}
