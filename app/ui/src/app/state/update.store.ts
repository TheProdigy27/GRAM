import { DestroyRef, Injectable, InjectionToken, computed, inject, signal } from '@angular/core';
import { errorMessage } from '../core/errors';
import type { UpdateInfo } from '../core/generated/UpdateInfo';
import type { UpdateProgress } from '../core/generated/UpdateProgress';
import { UpdateApi } from '../core/update-api';

export { RELEASES_URL } from '../core/update-api';

/** Application qui se met à jour, dans les textes du dialogue (GRAM Harness fournit la sienne). */
export interface UpdatedApp {
  /** « GRAM », « GRAM Harness ». */
  name: string;
  /** Ce que la mise à jour garde : « vos paramètres… sont conservés ». */
  kept: string;
}

export const UPDATED_APP = new InjectionToken<UpdatedApp>('UPDATED_APP', {
  factory: () => ({ name: 'GRAM', kept: 'vos paramètres, corrections et mesures' }),
});

/** Délai de la vérification automatique au lancement : le premier classement passe d'abord. */
export const UPDATE_CHECK_DELAY_MS = new InjectionToken<number>('UPDATE_CHECK_DELAY_MS', {
  factory: () => 5000,
});

export type UpdateStatus = 'idle' | 'checking' | 'upToDate' | 'available' | 'installing' | 'error';

/**
 * Mises à jour depuis les releases GitHub : vérification (automatique au lancement, ou depuis le
 * dialogue), puis téléchargement, vérification de la signature, installation et redémarrage, le
 * tout côté Rust (crate `gram-update`). Partagé avec GRAM Harness (alias `@gram/updates`).
 */
@Injectable({ providedIn: 'root' })
export class UpdateStore {
  private readonly api = inject(UpdateApi);

  readonly status = signal<UpdateStatus>('idle');
  /** Version en cours, connue après la première vérification réussie. */
  readonly currentVersion = signal<string | null>(null);
  /** Mise à jour proposée par la dernière vérification réussie. */
  readonly available = signal<UpdateInfo | null>(null);
  readonly progress = signal<UpdateProgress | null>(null);
  readonly error = signal<string | null>(null);

  /** Part téléchargée en %, `null` tant que la taille du paquet est inconnue. */
  readonly percent = computed(() => {
    const p = this.progress();
    return p?.total ? Math.min(100, (100 * p.downloaded) / p.total) : null;
  });
  /** Paquet entièrement reçu : vérification de la signature puis installation. */
  readonly downloaded = computed(() => {
    const p = this.progress();
    return p?.total != null && p.total > 0 && p.downloaded >= p.total;
  });

  constructor() {
    const progress = this.api.onProgress((p) => this.progress.set(p));
    inject(DestroyRef).onDestroy(() => {
      void progress.then((f) => f()).catch(() => undefined);
    });
  }

  /**
   * Cherche une mise à jour. `silent` (vérification automatique) : un échec (hors ligne, GitHub
   * injoignable…) n'est pas affiché. Renvoie la mise à jour proposée, s'il y en a une.
   */
  async check(silent = false): Promise<UpdateInfo | null> {
    const status = this.status();
    if (status === 'checking' || status === 'installing') return this.available();
    this.status.set('checking');
    this.error.set(null);
    try {
      const { currentVersion, update } = await this.api.check();
      this.currentVersion.set(currentVersion);
      this.available.set(update);
      this.status.set(update ? 'available' : 'upToDate');
      return update;
    } catch (e) {
      if (silent) {
        this.status.set(status);
      } else {
        this.error.set(errorMessage(e));
        this.status.set('error');
      }
      return null;
    }
  }

  /** Installe la mise à jour proposée ; l'application redémarre (ne revient qu'en cas d'erreur). */
  async install(): Promise<void> {
    if (this.status() === 'installing') return;
    this.status.set('installing');
    this.progress.set(null);
    this.error.set(null);
    try {
      await this.api.install();
    } catch (e) {
      this.error.set(errorMessage(e));
      this.status.set('error');
    }
  }
}

/** Bloc des notes de version : titre, élément de liste ou paragraphe. */
export interface NoteBlock {
  kind: 'heading' | 'item' | 'text';
  text: string;
}

/**
 * Notes de version (Markdown simple) en blocs affichés comme du texte : titres `#`, listes `-`
 * ou `*`, paragraphes. Le reste de la syntaxe est laissé tel quel (rien n'est interprété comme
 * du HTML).
 */
export function noteBlocks(markdown: string): NoteBlock[] {
  const blocks: NoteBlock[] = [];
  for (const raw of markdown.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) continue;
    const heading = /^#{1,6}\s+(.*)$/.exec(line);
    const item = /^[-*]\s+(.*)$/.exec(line);
    if (heading) blocks.push({ kind: 'heading', text: heading[1] });
    else if (item) blocks.push({ kind: 'item', text: item[1] });
    else blocks.push({ kind: 'text', text: line });
  }
  return blocks;
}
