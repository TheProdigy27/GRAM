import { Injectable, InjectionToken, computed, inject, resource, signal } from '@angular/core';
import type { RankItem } from '../core/generated/RankItem';
import type { RankRequest } from '../core/generated/RankRequest';
import type { RankResponse } from '../core/generated/RankResponse';
import type { Runtime } from '../core/generated/Runtime';
import type { UseCase } from '../core/generated/UseCase';
import { TauriService } from '../core/tauri.service';
import { gbToBytes } from '../shared/format';
import { delay, sameJson } from './async';
import { CatalogStore } from './catalog.store';
import { HardwareStore } from './hardware.store';
import { SettingsStore } from './settings.store';

/** Debounce des filtres (curseur de contexte, saisie de la taille ou de la recherche), en ms. */
export const FILTER_DEBOUNCE_MS = new InjectionToken<number>('FILTER_DEBOUNCE_MS', {
  factory: () => 150,
});

/** Lignes par page tant que les paramètres ne sont pas chargés (défaut du §12). */
const DEFAULT_PAGE_SIZE = 25;

/**
 * Filtres du panneau de gauche et classement qui en résulte. Le calcul est fait en Rust à
 * chaque changement (après un court debounce) ; le classement précédent reste affiché pendant
 * le calcul.
 */
@Injectable({ providedIn: 'root' })
export class RankingStore {
  private readonly tauri = inject(TauriService);
  private readonly settings = inject(SettingsStore);
  private readonly hardware = inject(HardwareStore);
  private readonly catalog = inject(CatalogStore);
  private readonly debounceMs = inject(FILTER_DEBOUNCE_MS);

  // ── Filtres ──────────────────────────────────────────────────────────────────────────────

  readonly useCase = signal<UseCase>('general');
  /** Contexte souhaité ; `null` : celui du cas d'usage (paramètres). */
  readonly ctx = signal<number | null>(null);
  /** Runtime choisi pendant la session ; `null` : celui des paramètres. */
  readonly runtime = signal<Runtime | null>(null);
  /** Taille maximale d'une quantification, en Go ; `null` : pas de limite. */
  readonly maxSizeGb = signal<number | null>(null);
  /** Recherche par nom, telle que saisie ; vide : tous les modèles. */
  readonly search = signal('');
  readonly showTooBig = signal(false);
  /** Pages affichées (« Voir plus »). */
  readonly pages = signal(1);

  readonly pageSize = computed(() => this.settings.settings()?.pageSize ?? DEFAULT_PAGE_SIZE);

  /** Requête envoyée à `rank` (champs absents : valeurs des paramètres côté Rust). */
  readonly request = computed<RankRequest>(
    () => {
      const ctx = this.ctx();
      const runtime = this.runtime();
      const maxGb = this.maxSizeGb();
      const search = this.search().trim();
      return {
        useCase: this.useCase(),
        ...(ctx !== null ? { ctx } : {}),
        ...(runtime !== null ? { runtime } : {}),
        ...(maxGb !== null ? { maxSize: gbToBytes(maxGb) } : {}),
        ...(search !== '' ? { search } : {}),
        showTooBig: this.showTooBig(),
        limit: this.pageSize() * this.pages(),
      };
    },
    { equal: sameJson },
  );

  /** Dernier classement reçu : gardé pendant un nouveau calcul et après une erreur. */
  private readonly last = signal<RankResponse | undefined>(undefined);
  readonly response = this.last.asReadonly();

  private readonly query = resource({
    params: () => ({
      request: this.request(),
      // Recalcul quand le matériel, le catalogue ou les paramètres changent, à requête égale.
      hardware: this.hardware.version(),
      catalog: this.catalog.version(),
      settings: this.settings.version(),
    }),
    loader: async ({ params, abortSignal, previous }) => {
      // Pas d'attente au premier calcul : le classement s'affiche dès le lancement.
      if (previous.status !== 'idle') await delay(this.debounceMs, abortSignal);
      const response = await this.tauri.rank(params.request);
      // Une requête remplacée entre-temps n'écrase pas la plus récente.
      if (!abortSignal.aborted) this.last.set(response);
      return response;
    },
  });

  readonly loading = this.query.isLoading;
  readonly error = computed(() => this.query.error()?.message ?? null);

  /** Contexte affiché : celui demandé, sinon celui retenu par le backend. */
  readonly effectiveCtx = computed(() => this.ctx() ?? this.response()?.ctx ?? null);
  /** Runtime affiché : celui choisi, sinon celui du dernier classement ou des paramètres. */
  readonly effectiveRuntime = computed<Runtime>(
    () =>
      this.runtime() ?? this.response()?.runtime ?? this.settings.settings()?.runtime ?? 'ollama',
  );

  // ── Actions ──────────────────────────────────────────────────────────────────────────────

  /** Changer de cas d'usage reprend aussi son contexte par défaut. */
  setUseCase(useCase: UseCase): void {
    this.useCase.set(useCase);
    this.ctx.set(null);
    this.pages.set(1);
  }

  setCtx(ctx: number | null): void {
    this.ctx.set(ctx);
  }

  setRuntime(runtime: Runtime): void {
    this.runtime.set(runtime);
  }

  setMaxSizeGb(gb: number | null): void {
    this.maxSizeGb.set(gb !== null && Number.isFinite(gb) && gb > 0 ? gb : null);
    this.pages.set(1);
  }

  setSearch(text: string): void {
    this.search.set(text);
    this.pages.set(1);
  }

  setShowTooBig(show: boolean): void {
    this.showTooBig.set(show);
  }

  showMore(): void {
    this.pages.update((p) => p + 1);
  }

  reload(): void {
    this.query.reload();
  }

  /** Ligne du classement d'un modèle (toutes sections confondues). */
  findItem(repoId: string): RankItem | undefined {
    const r = this.response();
    if (!r) return undefined;
    return [...r.items, ...r.slow, ...r.tooBig].find((i) => i.repoId === repoId);
  }
}
