import { Injectable, computed, inject, signal } from '@angular/core';
import type { HarnessModel } from '@gram/types/HarnessModel';
import type { ModelList } from '@gram/types/ModelList';
import { HarnessApi, errorMessage } from '../core/harness-api.service';
import { PrefsStore } from './prefs.store';

/**
 * Modèles d'Ollama avec lesquels discuter, état d'Ollama et modèle choisi. La liste est relue
 * au lancement, par « Actualiser » (R) et quand la fenêtre reprend le focus (un modèle ajouté
 * depuis GRAM apparaît sans rien faire). Le modèle choisi est retenu d'un lancement à l'autre ;
 * s'il a disparu d'Ollama, le premier de la liste le remplace.
 */
@Injectable({ providedIn: 'root' })
export class ModelsStore {
  private readonly api = inject(HarnessApi);
  private readonly prefs = inject(PrefsStore);

  /** Dernière réponse ; `null` avant la première. */
  readonly list = signal<ModelList | null>(null);
  readonly loading = signal(false);
  /** Échec de la dernière lecture (Ollama a répondu de travers…), en français. */
  readonly error = signal<string | null>(null);

  readonly ollama = computed(() => this.list()?.ollama ?? null);
  readonly models = computed(() => this.list()?.models ?? []);
  readonly selected = computed<HarnessModel | null>(() => {
    const models = this.models();
    const wanted = this.prefs.model();
    return models.find((m) => m.name === wanted) ?? models[0] ?? null;
  });

  /** Relit la liste ; sans effet si une lecture est déjà en cours. */
  async refresh(): Promise<void> {
    if (this.loading()) return;
    this.loading.set(true);
    try {
      this.list.set(await this.api.listModels());
      this.error.set(null);
    } catch (e) {
      this.error.set(errorMessage(e));
    } finally {
      this.loading.set(false);
    }
  }

  select(name: string): void {
    this.prefs.setModel(name);
  }
}
