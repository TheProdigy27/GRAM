import { Injectable, inject } from '@angular/core';
import { ToastService } from '@gram/ui/toast/toast';
import { HarnessApi } from '../core/harness-api.service';
import { sameModelName } from '../shared/models';
import { ChatStore } from './chat.store';
import { ModelsStore } from './models.store';

/** Durée du toast « modèle absent » : un message à lire. */
const MISSING_TOAST_MS = 8000;

/**
 * « Ouvrir dans GRAM Harness » : GRAM ouvre le lien `gram-harness://open?model=…`, que le
 * backend analyse strictement, met de côté et annonce (`open-model`). Le modèle demandé est pris
 * au lancement, puis à chaque annonce, et choisi dans une nouvelle conversation (ou dans la
 * nouvelle conversation vide affichée). Le lien ne fait que choisir un modèle déjà dans Ollama.
 */
@Injectable({ providedIn: 'root' })
export class OpenLinks {
  private readonly api = inject(HarnessApi);
  private readonly models = inject(ModelsStore);
  private readonly chat = inject(ChatStore);
  private readonly toast = inject(ToastService);

  /**
   * Écoute les liens, puis prend celui du lancement (après l'abonnement : un lien reçu
   * entre-temps est pris ici ou annoncé, jamais perdu). Renvoie de quoi arrêter d'écouter.
   */
  start(): () => void {
    const unlisten = this.api.onOpenModel(() => void this.take());
    void unlisten.then(
      () => this.take(),
      () => this.take(),
    );
    return () => void unlisten.then((stop) => stop()).catch(() => undefined);
  }

  /** Prend le modèle demandé par le dernier lien, s'il y en a un, et l'ouvre. */
  async take(): Promise<void> {
    let name: string | null;
    try {
      name = await this.api.takeOpenedModel();
    } catch {
      return;
    }
    if (name) await this.open(name);
  }

  /**
   * Choisit le modèle dans une nouvelle conversation ; `false` s'il n'est pas dans Ollama
   * (toast, rien ne change). Ollama arrêté ou liste illisible : le modèle est retenu, et l'écran
   * dit quoi faire.
   */
  async open(name: string): Promise<boolean> {
    // Liste relue : le modèle vient peut-être d'être ajouté depuis GRAM.
    await this.models.refresh();
    const found = this.models.models().find((m) => sameModelName(m.name, name));
    if (!found && this.models.ollama() === 'running' && !this.models.error()) {
      this.toast.show(`Ce modèle (${name}) n’est pas dans Ollama : l’ajouter depuis GRAM.`, {
        duration: MISSING_TOAST_MS,
      });
      return false;
    }
    this.models.select(found?.name ?? name);
    if (this.chat.current() || this.chat.opening()) this.chat.newConversation();
    else this.chat.focusRequest.update((n) => n + 1);
    return true;
  }
}
