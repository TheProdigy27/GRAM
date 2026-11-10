import { DestroyRef, Injectable, inject, signal } from '@angular/core';
import type { RateLimitInfo } from '../core/generated/RateLimitInfo';
import type { SyncProgress } from '../core/generated/SyncProgress';
import type { SyncSummary } from '../core/generated/SyncSummary';
import { TauriService, errorMessage } from '../core/tauri.service';

/** Synchronisation du catalogue Hugging Face (barre d'état). */
@Injectable({ providedIn: 'root' })
export class CatalogStore {
  private readonly tauri = inject(TauriService);

  readonly syncing = signal(false);
  readonly progress = signal<SyncProgress | null>(null);
  readonly rateLimit = signal<RateLimitInfo | null>(null);
  readonly summary = signal<SyncSummary | null>(null);
  readonly error = signal<string | null>(null);
  /** Incrémenté à chaque catalogue remplacé : le classement est recalculé. */
  readonly version = signal(0);

  constructor() {
    const subscriptions = [
      this.tauri.listen('sync-progress', (p) => this.progress.set(p)),
      this.tauri.listen('rate-limit', (r) => this.rateLimit.set(r)),
    ];
    inject(DestroyRef).onDestroy(() => {
      for (const s of subscriptions) void s.then((f) => f()).catch(() => undefined);
    });
  }

  /**
   * `force = false` au lancement : rien n'est téléchargé si le cache a moins de 24 h.
   * Hors ligne, le backend se rabat sur le cache puis le catalogue embarqué (sans erreur).
   */
  async sync(force: boolean): Promise<void> {
    if (this.syncing()) return;
    this.syncing.set(true);
    this.progress.set(null);
    this.error.set(null);
    try {
      const summary = await this.tauri.syncCatalog(force);
      this.summary.set(summary);
      if (summary.rateLimit) this.rateLimit.set(summary.rateLimit);
      this.version.update((v) => v + 1);
    } catch (e) {
      this.error.set(errorMessage(e));
    } finally {
      this.syncing.set(false);
      this.progress.set(null);
    }
  }
}
