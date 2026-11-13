import { Component, computed, inject, signal } from '@angular/core';
import { Button } from '@gram/ui/button/button';
import { Callout } from '@gram/ui/callout/callout';
import { EmptyState } from '@gram/ui/empty-state/empty-state';
import { Icon } from '@gram/ui/icon/icon';
import { Spinner } from '@gram/ui/spinner/spinner';
import { Tag } from '@gram/ui/tag/tag';
import { HarnessApi, errorMessage } from '../../core/harness-api.service';
import { capabilities, modelFacts } from '../../shared/models';
import { ModelsStore } from '../../state/models.store';

/** Ce que montre la zone centrale. */
export type HomeView = 'error' | 'loading' | 'absent' | 'stopped' | 'empty' | 'model';

/**
 * Début d'une nouvelle conversation : le modèle choisi, ou ce qu'il faut faire pour en avoir un
 * (installer ou lancer Ollama, ajouter des modèles depuis GRAM).
 */
@Component({
  selector: 'app-home',
  imports: [Button, Callout, EmptyState, Icon, Spinner, Tag],
  templateUrl: './home.html',
  styleUrl: './home.scss',
})
export class Home {
  protected readonly store = inject(ModelsStore);
  private readonly api = inject(HarnessApi);

  /** Échec de l'ouverture du navigateur. */
  protected readonly openError = signal<string | null>(null);

  protected readonly view = computed<HomeView>(() => {
    if (this.store.error()) return 'error';
    const list = this.store.list();
    if (!list) return 'loading';
    if (list.ollama !== 'running') return list.ollama;
    return this.store.selected() ? 'model' : 'empty';
  });

  protected readonly facts = computed(() => {
    const m = this.store.selected();
    return m ? modelFacts(m) : [];
  });
  protected readonly caps = computed(() => {
    const m = this.store.selected();
    return m ? capabilities(m) : [];
  });

  protected refresh(): void {
    void this.store.refresh();
  }

  protected async openDownload(): Promise<void> {
    this.openError.set(null);
    try {
      await this.api.openOllamaDownload();
    } catch (e) {
      this.openError.set(errorMessage(e));
    }
  }
}
