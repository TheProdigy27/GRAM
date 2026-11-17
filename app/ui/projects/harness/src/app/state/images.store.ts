import { Injectable, inject, signal } from '@angular/core';
import { HarnessApi } from '../core/harness-api.service';

/**
 * Images jointes aux messages enregistrés, en adresses `data:` lues une fois par image
 * (`get_image`). Une image juste envoyée y est déposée telle quelle (`seed`) : pas de relecture.
 */
@Injectable({ providedIn: 'root' })
export class ImagesStore {
  private readonly api = inject(HarnessApi);
  /** Par `conversation/nom` ; `null` : introuvable. */
  private readonly urls = signal<ReadonlyMap<string, string | null>>(new Map());
  private readonly loading = new Set<string>();

  /**
   * Adresse d'une image, chargée au premier appel : `undefined` pendant le chargement, `null`
   * si elle est introuvable. Une adresse `data:` (message pas encore enregistré) est rendue
   * telle quelle.
   */
  url(conversationId: string, name: string): string | null | undefined {
    if (name.startsWith('data:')) return name;
    const key = `${conversationId}/${name}`;
    const known = this.urls().get(key);
    if (known !== undefined || !conversationId) return known;
    if (!this.loading.has(key)) {
      this.loading.add(key);
      void this.load(key, conversationId, name);
    }
    return undefined;
  }

  /** Image qu'on vient d'envoyer, déjà lue. */
  seed(conversationId: string, name: string, url: string): void {
    this.urls.update((map) => new Map(map).set(`${conversationId}/${name}`, url));
  }

  /** Conversation supprimée : ses images sortent du cache. */
  forget(conversationId: string): void {
    const prefix = `${conversationId}/`;
    this.urls.update((map) => new Map([...map].filter(([key]) => !key.startsWith(prefix))));
  }

  private async load(key: string, conversationId: string, name: string): Promise<void> {
    let url: string | null;
    try {
      url = await this.api.getImage(conversationId, name);
    } catch {
      url = null;
    } finally {
      this.loading.delete(key);
    }
    this.urls.update((map) => new Map(map).set(key, url));
  }
}
