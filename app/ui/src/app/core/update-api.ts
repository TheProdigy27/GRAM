import { Injectable } from '@angular/core';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { openUrl } from '@tauri-apps/plugin-opener';
import { rethrow } from './errors';
import type { UpdateCheck } from './generated/UpdateCheck';
import type { UpdateProgress } from './generated/UpdateProgress';

/** Page des releases : téléchargement manuel si la mise à jour automatique échoue. */
export const RELEASES_URL = 'https://github.com/TheProdigy27/GRAM/releases/latest';

/**
 * Commandes de mise à jour, les mêmes dans GRAM et dans GRAM Harness (crate `gram-update`) :
 * `check_update`, `install_update` et l'événement `update-progress`. Seul accès à l'IPC hors de
 * `TauriService` (GRAM) et de `HarnessApi` (GRAM Harness) : le magasin et le dialogue des mises
 * à jour servent aux deux applications (alias `@gram/updates`, `@gram/update-dialog`).
 */
@Injectable({ providedIn: 'root' })
export class UpdateApi {
  /** Version en cours et mise à jour proposée par la dernière release (`update: null` si à jour). */
  check(): Promise<UpdateCheck> {
    return invoke<UpdateCheck>('check_update').catch(rethrow);
  }

  /**
   * Télécharge la mise à jour (événements `update-progress`), vérifie sa signature, l'installe
   * et relance l'application : la promesse ne se résout pas, sauf erreur.
   */
  install(): Promise<void> {
    return invoke<void>('install_update').catch(rethrow);
  }

  onProgress(handler: (progress: UpdateProgress) => void): Promise<UnlistenFn> {
    return listen<UpdateProgress>('update-progress', (e) => handler(e.payload));
  }

  /** Page des releases dans le navigateur du système (autorisée dans les deux applications). */
  openReleases(): Promise<void> {
    return openUrl(RELEASES_URL).catch(rethrow);
  }
}
