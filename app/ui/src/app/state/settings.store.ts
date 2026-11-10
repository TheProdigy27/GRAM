import { Injectable, inject, signal } from '@angular/core';
import type { Settings } from '../core/generated/Settings';
import { TauriService, errorMessage } from '../core/tauri.service';

/** Paramètres utilisateur (§12), enregistrés côté Rust (`settings.json`). */
@Injectable({ providedIn: 'root' })
export class SettingsStore {
  private readonly tauri = inject(TauriService);

  readonly settings = signal<Settings | null>(null);
  readonly error = signal<string | null>(null);
  /**
   * Incrémenté après un enregistrement du dialogue des paramètres : le classement et la fiche
   * sont recalculés (marges, efficacité, seuils… changent les estimations à requête égale).
   */
  readonly version = signal(0);

  async load(): Promise<void> {
    try {
      this.settings.set(await this.tauri.getSettings());
    } catch (e) {
      this.error.set(errorMessage(e));
    }
  }

  /** Enregistre tous les paramètres (dialogue) ; renvoie les valeurs ramenées dans leurs bornes. */
  async save(settings: Settings): Promise<Settings> {
    const saved = await this.tauri.saveSettings(settings);
    this.settings.set(saved);
    this.error.set(null);
    this.version.update((v) => v + 1);
    return saved;
  }

  /**
   * Modifie un champ suivi par la requête du classement (runtime) et l'enregistre ; en cas
   * d'échec, revient à la valeur précédente et rejette.
   */
  async update(patch: Partial<Settings>): Promise<void> {
    const current = this.settings();
    if (!current) throw new Error('Paramètres non chargés.');
    this.settings.set({ ...current, ...patch });
    try {
      this.settings.set(await this.tauri.saveSettings({ ...current, ...patch }));
    } catch (e) {
      this.settings.set(current);
      throw e;
    }
  }
}
