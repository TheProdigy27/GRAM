import {
  DestroyRef,
  Injectable,
  computed,
  effect,
  inject,
  resource,
  signal,
  untracked,
} from '@angular/core';
import type { AddOutcome } from '../core/generated/AddOutcome';
import type { HarnessAddRequest } from '../core/generated/HarnessAddRequest';
import type { HarnessEntry } from '../core/generated/HarnessEntry';
import type { HarnessStatus } from '../core/generated/HarnessStatus';
import type { InstallDone } from '../core/generated/InstallDone';
import type { InstallProgress } from '../core/generated/InstallProgress';
import type { OllamaModel } from '../core/generated/OllamaModel';
import type { Preflight } from '../core/generated/Preflight';
import type { RemoveOutcome } from '../core/generated/RemoveOutcome';
import type { SharedModel } from '../core/generated/SharedModel';
import { TauriService, errorMessage } from '../core/tauri.service';
import { ctxK, fileSize } from '../shared/format';
import { type VariantRef, sameVariant } from '../shared/harness';
import { installedModels } from '../shared/installed';
import { ToastService } from '../shared/ui/toast/toast';
import { sameJson } from './async';
import { DetailStore } from './detail.store';
import { HardwareStore } from './hardware.store';

/** Erreur du dernier ajout, pour la quantification concernée. */
export interface HarnessError extends VariantRef {
  message: string;
}

/** Durée des toasts qui portent un message à lire (échec, avertissements). */
const LONG_TOAST_MS = 10_000;

/** Délai minimal entre deux relectures déclenchées par le focus de la fenêtre. */
export const FOCUS_REFRESH_MS = 2000;

/**
 * Ajout à GRAM Harness (phase 2 côté Rust) : modèles ajoutés, ajout en cours (événements
 * `harness-progress` et `harness-done`), préflight de la quantification affichée dans la
 * fiche, présence de GRAM Harness et actions. Un seul ajout à la fois ; il continue fiche
 * fermée, et après un rechargement de l'interface (`harness_status` donne sa progression).
 *
 * Tant que GRAM Harness n'est pas installé, le retour dans la fenêtre relit l'état : installé
 * entre-temps (« Télécharger GRAM Harness »), la fiche propose « Ouvrir dans GRAM Harness ».
 *
 * Toasts de fin d'ajout : depuis la promesse pour un ajout lancé ici, depuis `harness-done`
 * pour un ajout lancé avant un rechargement, jamais les deux.
 */
@Injectable({ providedIn: 'root' })
export class HarnessStore {
  private readonly tauri = inject(TauriService);
  private readonly hardware = inject(HardwareStore);
  private readonly detail = inject(DetailStore);
  private readonly toast = inject(ToastService);

  readonly status = signal<HarnessStatus | null>(null);
  /** Ajout en cours, ici ou avant un rechargement. */
  readonly install = signal<InstallProgress | null>(null);
  readonly error = signal<HarnessError | null>(null);
  readonly cancelling = signal(false);
  /** Modèle dérivé en cours de retrait. */
  readonly removing = signal<string | null>(null);
  /** Avertissements du dernier ajout de chaque modèle dérivé (vision absente…). */
  readonly addedWarnings = signal<Readonly<Record<string, readonly string[]>>>({});

  readonly entries = computed<readonly HarnessEntry[]>(() => this.status()?.models ?? []);
  /** GRAM Harness, s'il est installé (il a écrit `harness.json` et son exécutable est là). */
  readonly presence = computed(() => this.status()?.presence ?? null);
  readonly ollamaRunning = computed(() => !!this.hardware.runtimes()?.ollama.running);
  /** Lignes de la liste « Modèles installés » (vide si Ollama ne répond pas). */
  readonly installed = computed(() =>
    installedModels(this.hardware.runtimes()?.ollamaModels ?? [], this.entries()),
  );

  /** Ajout lancé ici, en attente de sa réponse. */
  private pending: VariantRef | null = null;
  /** Dernier ajout terminé : un événement de progression en retard est ignoré. */
  private finished: VariantRef | null = null;
  /** Seule la réponse du dernier `harness_status` demandé compte. */
  private refreshSeq = 0;
  private lastFocusRefresh = 0;

  // ── Préflight de la quantification affichée ───────────────────────────────────────────────

  /** Requête d'ajout de la quantification affichée dans la fiche (filtres de la fiche). */
  readonly request = computed<HarnessAddRequest | null>(
    () => {
      const d = this.detail.detail();
      const v = this.detail.selected();
      if (!d || !v) return null;
      return { repoId: d.model.repoId, label: v.variant.label, useCase: d.useCase, ctx: d.ctx };
    },
    { equal: sameJson },
  );

  /** Dernier préflight reçu : gardé pendant un recalcul (pas de clignotement). */
  private readonly lastPreflight = signal<Preflight | null>(null);

  private readonly preflightQuery = resource({
    params: () => {
      const request = this.request();
      if (!request || !this.ollamaRunning()) return undefined;
      // Recalculé avec la fiche (matériel, paramètres, modèles d'Ollama après un ajout).
      return { request, detail: this.detail.detail() };
    },
    loader: async ({ params: { request }, abortSignal }) => {
      const p = await this.tauri.harnessPreflight(request);
      if (!abortSignal.aborted) this.lastPreflight.set(p);
      return p;
    },
  });

  /** Préflight de la quantification affichée, s'il est connu et qu'Ollama répond. */
  readonly preflight = computed<Preflight | null>(() => {
    const p = this.lastPreflight();
    if (!p || !this.ollamaRunning() || this.preflightQuery.status() === 'error') return null;
    return sameVariant(p, this.request()) ? p : null;
  });
  readonly preflightLoading = this.preflightQuery.isLoading;
  readonly preflightError = computed(() => this.preflightQuery.error()?.message ?? null);

  /**
   * Quantification de la fiche quand « Ajouter à GRAM Harness » est possible (palette) : Ollama
   * répond, rien ne l'empêche, aucun ajout en cours, pas encore ajoutée.
   */
  readonly addable = computed(() => {
    const request = this.request();
    const preflight = this.preflight();
    if (!request || !preflight || preflight.blocked || this.install()) return null;
    return this.entryFor(request) ? null : { request, preflight };
  });

  /**
   * Modèle de la quantification de la fiche quand « Ouvrir dans GRAM Harness » est possible
   * (palette) : ajoutée, et GRAM Harness installé.
   */
  readonly openable = computed<SharedModel | null>(() => {
    const request = this.request();
    if (!request || !this.presence()) return null;
    return this.entryFor(request)?.model ?? null;
  });

  constructor() {
    const listeners = [
      this.tauri.listen('harness-progress', (p) => this.onProgress(p)),
      this.tauri.listen('harness-done', (d) => void this.onDone(d)),
    ];
    const onFocus = () => this.onWindowFocus();
    window.addEventListener('focus', onFocus);
    inject(DestroyRef).onDestroy(() => {
      for (const l of listeners) void l.then((f) => f()).catch(() => undefined);
      window.removeEventListener('focus', onFocus);
    });
    // Au démarrage, puis à chaque détection des runtimes (« Actualiser » d'Ollama…).
    effect(() => {
      this.hardware.runtimes();
      untracked(() => void this.refresh());
    });
  }

  /** Entrée du manifeste d'une quantification (par dépôt canonique et quantification). */
  entryFor(v: VariantRef): HarnessEntry | undefined {
    return this.entries().find((e) =>
      sameVariant({ repoId: e.model.repoId, label: e.model.quant }, v),
    );
  }

  /** Relit les modèles ajoutés et l'ajout en cours ; un échec garde l'état connu. */
  async refresh(): Promise<void> {
    const seq = ++this.refreshSeq;
    let s: HarnessStatus;
    try {
      s = await this.tauri.harnessStatus();
    } catch {
      return;
    }
    if (seq !== this.refreshSeq) return;
    this.status.set(s);
    if (s.install) {
      if (!sameVariant(s.install, this.finished)) this.install.set(s.install);
    } else if (!this.pending) {
      this.install.set(null);
    }
  }

  /**
   * Ajoute une quantification à GRAM Harness, ou refait son modèle dérivé (autre contexte).
   * Ne rejette pas : l'issue est annoncée par un toast, l'erreur gardée dans `error`.
   * `null` si l'ajout est annulé ou en échec.
   */
  async add(request: HarnessAddRequest): Promise<AddOutcome | null> {
    if (this.pending) return null;
    const key: VariantRef = { repoId: request.repoId, label: request.label };
    const name = this.displayName(key);
    // Contexte du modèle dérivé déjà ajouté : « Régler le contexte » le refait.
    const previousCtx = this.entryFor(key)?.model.ctx ?? null;
    this.pending = key;
    this.finished = null;
    this.error.set(null);
    // Retour immédiat à l'écran, avant le premier événement.
    if (!this.install()) {
      const ollamaName =
        this.preflightFor(key)?.ollamaName ?? this.entryFor(key)?.model.ollamaName ?? '';
      this.install.set({
        ...key,
        displayName: name,
        ollamaName,
        phase: 'preparing',
        completed: 0,
        total: 0,
        bytesPerS: null,
      });
    }
    try {
      const outcome = await this.tauri.harnessAdd(request);
      if (outcome) this.announceAdded(outcome.model, outcome.warnings, previousCtx);
      else this.announceCancelled(name, key, this.install());
      return outcome;
    } catch (e) {
      this.announceFailed(name, key, errorMessage(e));
      return null;
    } finally {
      // Les modèles ajoutés d'abord : la fiche passe de la progression à « ajoutée », sans
      // repasser par « pas encore ajoutée ».
      await this.refresh();
      this.pending = null;
      this.end(key);
      void this.hardware.refreshRuntimes();
    }
  }

  /** Annule l'ajout en cours ; sa fin arrive par `add` ou `harness-done`. */
  async cancel(): Promise<void> {
    if (!this.install() || this.cancelling()) return;
    this.cancelling.set(true);
    try {
      // `false` : l'ajout venait de finir.
      if (!(await this.tauri.harnessCancel())) {
        this.cancelling.set(false);
        void this.refresh();
      }
    } catch (e) {
      this.cancelling.set(false);
      this.toast.show(`Annulation impossible : ${errorMessage(e)}`, { kind: 'error' });
    }
  }

  /**
   * Ouvre un modèle dans GRAM Harness, qui le choisit dans une nouvelle conversation. Ne rejette
   * pas : un échec est annoncé par un toast, et l'état relu (GRAM Harness désinstallé…).
   */
  async open(model: SharedModel): Promise<boolean> {
    try {
      await this.tauri.harnessOpen(model.ollamaName);
      return true;
    } catch (e) {
      this.toast.show(`Ouverture impossible : ${errorMessage(e)}`, {
        kind: 'error',
        duration: LONG_TOAST_MS,
      });
      void this.refresh();
      return false;
    }
  }

  /** Retire un modèle de GRAM Harness (et sa base, si demandé et permis). */
  async remove(model: SharedModel, removeBase: boolean): Promise<RemoveOutcome | null> {
    if (this.removing()) return null;
    this.removing.set(model.ollamaName);
    try {
      const r = await this.tauri.harnessRemove(model.ollamaName, removeBase);
      this.addedWarnings.update((w) =>
        Object.fromEntries(Object.entries(w).filter(([name]) => name !== model.ollamaName)),
      );
      this.hardware.forgetOllamaModels(
        r.baseRemoved ? [model.ollamaName, model.baseName] : [model.ollamaName],
      );
      const base = r.baseRemoved
        ? ` ${model.baseName} est supprimé d’Ollama (${fileSize(model.size)} libérés).`
        : '';
      this.toast.show(`${model.displayName} ${model.quant} retiré de GRAM Harness.${base}`, {
        kind: 'success',
        duration: base ? 6000 : 4000,
      });
      return r;
    } catch (e) {
      this.toast.show(`Retrait impossible : ${errorMessage(e)}`, {
        kind: 'error',
        duration: LONG_TOAST_MS,
      });
      return null;
    } finally {
      await this.refresh();
      this.removing.set(null);
      void this.hardware.refreshRuntimes();
    }
  }

  /**
   * Supprime d'Ollama un modèle installé sans GRAM Harness (liste « Modèles installés »). Ne
   * rejette pas : l'issue est annoncée par un toast ; `false` en cas d'échec.
   */
  async deleteModel(model: OllamaModel): Promise<boolean> {
    if (this.removing()) return false;
    this.removing.set(model.name);
    try {
      const removed = await this.tauri.ollamaDelete(model.name);
      this.hardware.forgetOllamaModels([model.name]);
      // Pas de place libérée annoncée : un fichier partagé avec un autre modèle reste.
      this.toast.show(
        removed
          ? `${model.name} est supprimé d’Ollama.`
          : `${model.name} n’était déjà plus dans Ollama.`,
        { kind: 'success', duration: 6000 },
      );
      return true;
    } catch (e) {
      this.toast.show(`Suppression impossible : ${errorMessage(e)}`, {
        kind: 'error',
        duration: LONG_TOAST_MS,
      });
      return false;
    } finally {
      await this.refresh();
      this.removing.set(null);
      void this.hardware.refreshRuntimes();
    }
  }

  // ── Événements ────────────────────────────────────────────────────────────────────────────

  /** De retour dans la fenêtre : GRAM Harness a peut-être été installé entre-temps. */
  private onWindowFocus(): void {
    if (this.presence()) return;
    const now = Date.now();
    if (now - this.lastFocusRefresh < FOCUS_REFRESH_MS) return;
    this.lastFocusRefresh = now;
    void this.refresh();
  }

  private onProgress(p: InstallProgress): void {
    if (!sameVariant(p, this.finished)) this.install.set(p);
  }

  /** Fin d'un ajout : annoncée ici seulement s'il n'a pas été lancé depuis cette interface. */
  private async onDone(d: InstallDone): Promise<void> {
    if (sameVariant(d, this.pending) || sameVariant(d, this.finished)) return;
    const key: VariantRef = { repoId: d.repoId, label: d.label };
    this.finished = key;
    const name = `${d.displayName} ${d.label}`;
    // Dernière progression connue, avant que la relecture ne l'efface.
    const last = this.install();
    await this.refresh();
    switch (d.result.kind) {
      case 'added': {
        const entry = this.entryFor(key);
        if (entry) this.announceAdded(entry.model, d.result.warnings);
        else this.toast.show(`${name} ajouté à GRAM Harness.`, { kind: 'success' });
        break;
      }
      case 'cancelled':
        this.announceCancelled(d.displayName, key, last);
        break;
      case 'failed':
        this.announceFailed(d.displayName, key, d.result.message);
        break;
    }
    this.end(key);
    void this.hardware.refreshRuntimes();
  }

  // ── Outils ────────────────────────────────────────────────────────────────────────────────

  private end(key: VariantRef): void {
    this.finished = key;
    if (sameVariant(this.install(), key)) this.install.set(null);
    this.cancelling.set(false);
  }

  /** `previousCtx` : contexte du modèle dérivé remplacé, s'il y en avait un. */
  private announceAdded(
    model: SharedModel,
    warnings: readonly string[],
    previousCtx: number | null = null,
  ): void {
    this.addedWarnings.update((w) => ({ ...w, [model.ollamaName]: warnings }));
    const what = `${model.displayName} ${model.quant}`;
    const done =
      previousCtx !== null && previousCtx !== model.ctx
        ? `${what} réglé au contexte ${ctxK(model.ctx)} dans GRAM Harness.`
        : `${what} ajouté à GRAM Harness.`;
    if (warnings.length === 0) {
      this.toast.show(done, { kind: 'success' });
    } else {
      this.toast.show(`${done} ${warnings.join(' ')}`, { duration: LONG_TOAST_MS });
    }
  }

  /** `last` : dernière progression de l'ajout, pour dire si une partie est téléchargée. */
  private announceCancelled(name: string, key: VariantRef, last: InstallProgress | null): void {
    const p = last;
    const partial =
      sameVariant(p, key) && p!.phase === 'downloading' && p!.completed > 0
        ? ' Ollama garde la partie téléchargée : un nouvel ajout reprendra là.'
        : '';
    this.toast.show(`Ajout de ${name} ${key.label} annulé.${partial}`, {
      duration: partial ? 6000 : 4000,
    });
  }

  private announceFailed(name: string, key: VariantRef, message: string): void {
    this.error.set({ ...key, message });
    this.toast.show(`Ajout de ${name} ${key.label} impossible : ${message}`, {
      kind: 'error',
      duration: LONG_TOAST_MS,
    });
  }

  private preflightFor(key: VariantRef): Preflight | null {
    const p = this.lastPreflight();
    return sameVariant(p, key) ? p : null;
  }

  /** Nom lisible d'un modèle : préflight, manifeste, fiche, sinon le dépôt. */
  private displayName(key: VariantRef): string {
    const d = this.detail.detail();
    return (
      this.preflightFor(key)?.displayName ??
      this.entryFor(key)?.model.displayName ??
      (d?.model.repoId === key.repoId ? d.model.displayName : key.repoId)
    );
  }
}
