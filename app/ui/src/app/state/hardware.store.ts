import { DestroyRef, Injectable, inject, signal } from '@angular/core';
import type { HardwareProfile } from '../core/generated/HardwareProfile';
import type { RuntimesStatus } from '../core/generated/RuntimesStatus';
import { TauriService, errorMessage } from '../core/tauri.service';

/** Matériel détecté et runtimes installés (bandeau du haut). */
@Injectable({ providedIn: 'root' })
export class HardwareStore {
  private readonly tauri = inject(TauriService);

  readonly profile = signal<HardwareProfile | null>(null);
  readonly runtimes = signal<RuntimesStatus | null>(null);
  readonly scanning = signal(false);
  readonly error = signal<string | null>(null);
  /** Incrémenté quand le matériel change (nouvelle analyse) : le classement est recalculé. */
  readonly version = signal(0);

  constructor() {
    // Le backend émet `hardware-ready` à la fin de chaque détection.
    const unlisten = this.tauri.listen('hardware-ready', (p) => this.profile.set(p));
    inject(DestroyRef).onDestroy(() => void unlisten.then((f) => f()).catch(() => undefined));
  }

  /** Premier chargement : profil (détecté au lancement par le backend) et runtimes. */
  async load(): Promise<void> {
    await Promise.all([
      this.tauri
        .getHardware()
        .then((p) => this.profile.set(p))
        .catch((e) => this.error.set(errorMessage(e))),
      this.loadRuntimes(),
    ]);
  }

  /** Bouton « Ré-analyser » : nouvelle détection du matériel et des runtimes. */
  async rescan(): Promise<void> {
    if (this.scanning()) return;
    this.scanning.set(true);
    this.error.set(null);
    try {
      const [profile] = await Promise.all([this.tauri.rescanHardware(), this.loadRuntimes()]);
      this.profile.set(profile);
      this.version.update((v) => v + 1);
    } catch (e) {
      this.error.set(errorMessage(e));
      throw e;
    } finally {
      this.scanning.set(false);
    }
  }

  private async loadRuntimes(): Promise<void> {
    try {
      this.runtimes.set(await this.tauri.detectRuntimes());
    } catch {
      // Non bloquant : le bandeau affiche « runtimes non détectés ».
    }
  }
}
