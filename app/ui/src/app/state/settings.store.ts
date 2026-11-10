import { Injectable, inject, signal } from '@angular/core';
import type { Settings } from '../core/generated/Settings';
import { TauriService, errorMessage } from '../core/tauri.service';

/** Paramètres utilisateur (§12), enregistrés côté Rust. */
@Injectable({ providedIn: 'root' })
export class SettingsStore {
  private readonly tauri = inject(TauriService);

  readonly settings = signal<Settings | null>(null);
  readonly error = signal<string | null>(null);

  async load(): Promise<void> {
    try {
      this.settings.set(await this.tauri.getSettings());
    } catch (e) {
      this.error.set(errorMessage(e));
    }
  }

  /** Modifie et enregistre ; en cas d'échec, revient à la valeur précédente et rejette. */
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
