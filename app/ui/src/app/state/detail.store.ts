import {
  Injectable,
  computed,
  effect,
  inject,
  linkedSignal,
  resource,
  signal,
  untracked,
} from '@angular/core';
import type { DetailRequest } from '../core/generated/DetailRequest';
import type { ModelDetail } from '../core/generated/ModelDetail';
import type { VariantDetail } from '../core/generated/VariantDetail';
import { TauriService } from '../core/tauri.service';
import { delay, sameJson } from './async';
import { HardwareStore } from './hardware.store';
import { FILTER_DEBOUNCE_MS, RankingStore } from './ranking.store';
import { SettingsStore } from './settings.store';

/**
 * Fiche détail du modèle sélectionné, avec les filtres du classement (cas d'usage, contexte,
 * runtime, taille maximale), et la quantification choisie dans son tableau.
 */
@Injectable({ providedIn: 'root' })
export class DetailStore {
  private readonly tauri = inject(TauriService);
  private readonly ranking = inject(RankingStore);
  private readonly hardware = inject(HardwareStore);
  private readonly settings = inject(SettingsStore);
  private readonly debounceMs = inject(FILTER_DEBOUNCE_MS);

  /** Modèle ouvert ; `null` : panneau fermé. */
  readonly repoId = signal<string | null>(null);
  readonly isOpen = computed(() => this.repoId() !== null);

  /** Requête de la fiche ; ne change pas avec la pagination ni l'affichage des trop gros. */
  private readonly request = computed<DetailRequest | undefined>(
    () => {
      const repoId = this.repoId();
      if (!repoId) return undefined;
      const { useCase, ctx, runtime, maxSize } = this.ranking.request();
      return { repoId, useCase, ctx, runtime, maxSize };
    },
    { equal: sameJson },
  );

  /** Dernier modèle demandé : le debounce ne s'applique qu'aux changements de filtres. */
  private lastRepoId: string | null = null;

  /** Dernière fiche reçue : gardée pendant un recalcul (changement de filtre). */
  private readonly last = signal<ModelDetail | undefined>(undefined);

  private readonly query = resource({
    params: () => {
      const request = this.request();
      // Recalcul aussi quand le matériel (corrections, simulation) ou les paramètres changent.
      return request
        ? { request, hardware: this.hardware.version(), settings: this.settings.version() }
        : undefined;
    },
    loader: async ({ params: { request }, abortSignal, previous }) => {
      const sameModel = request.repoId === this.lastRepoId;
      this.lastRepoId = request.repoId;
      if (sameModel && previous.status !== 'idle') await delay(this.debounceMs, abortSignal);
      const detail = await this.tauri.getModelDetail(request);
      if (!abortSignal.aborted) this.last.set(detail);
      return detail;
    },
  });

  /** Fiche affichée : seulement celle du modèle ouvert. */
  readonly detail = computed(() => {
    const d = this.last();
    return d && d.model.repoId === this.repoId() ? d : undefined;
  });

  readonly loading = this.query.isLoading;
  readonly error = computed(() => this.query.error()?.message ?? null);

  /** Quantification demandée à l'ouverture (conseil « Le plus rapide »…), si elle existe. */
  private wanted: { repoId: string; label: string } | null = null;

  /**
   * Quantification sélectionnée (par libellé, unique dans un modèle) : la meilleure à
   * l'ouverture (ou celle demandée), puis celle choisie tant qu'elle existe pour ce modèle.
   */
  readonly selectedLabel = linkedSignal<ModelDetail | undefined, string | null>({
    source: this.detail,
    computation: (d, previous) => {
      if (!d) return null;
      const has = (label: string | undefined) =>
        !!label && d.variants.some((v) => v.variant.label === label);
      const kept = previous?.value ?? undefined;
      const sameModel = previous?.source?.model.repoId === d.model.repoId;
      if (sameModel && has(kept)) return kept!;
      const wanted = this.wanted?.repoId === d.model.repoId ? this.wanted.label : undefined;
      if (has(wanted)) return wanted!;
      const best = d.best ?? 0;
      return d.variants[best]?.variant.label ?? null;
    },
  });

  readonly selected = computed<VariantDetail | undefined>(() => {
    const d = this.detail();
    const label = this.selectedLabel();
    return d?.variants.find((v) => v.variant.label === label);
  });

  constructor() {
    // À la première ouverture, le backend charge les tailles réelles du modèle : si sa ligne du
    // classement était estimée, le classement est recalculé pour en tenir compte.
    effect(() => {
      const d = this.query.hasValue() ? this.query.value() : undefined;
      if (!d || d.model.variants.length === 0) return;
      const row = untracked(() => this.ranking.findItem(d.model.repoId));
      if (row && !row.detailed) untracked(() => this.ranking.reload());
    });
  }

  /** Ouvre la fiche d'un modèle, sur sa meilleure quantification ou sur `label`. */
  open(repoId: string, label: string | null = null): void {
    this.wanted = label ? { repoId, label } : null;
    this.repoId.set(repoId);
    // Fiche déjà affichée pour ce modèle : la sélection ne se recalcule pas d'elle-même.
    const d = this.detail();
    if (label && d?.model.repoId === repoId && d.variants.some((v) => v.variant.label === label)) {
      this.selectedLabel.set(label);
    }
  }

  close(): void {
    this.repoId.set(null);
  }

  select(label: string): void {
    this.selectedLabel.set(label);
  }
}
