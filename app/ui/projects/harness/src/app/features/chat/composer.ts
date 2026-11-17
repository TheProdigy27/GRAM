import {
  Component,
  ElementRef,
  PendingTasks,
  afterRenderEffect,
  computed,
  inject,
  viewChild,
} from '@angular/core';
import { Button } from '@gram/ui/button/button';
import { DialogService } from '@gram/ui/dialog/dialog';
import { Icon } from '@gram/ui/icon/icon';
import { Switch } from '@gram/ui/switch/switch';
import { ToastService } from '@gram/ui/toast/toast';
import { Tooltip } from '@gram/ui/tooltip/tooltip';
import { errorMessage } from '../../core/harness-api.service';
import { IMAGE_ACCEPT, MAX_IMAGES, filesOf } from '../../shared/attachments';
import { modelLabel } from '../../shared/conversations';
import { ChatStore } from '../../state/chat.store';
import { ModelsStore } from '../../state/models.store';

/** Hauteur maximale de la zone de saisie, en pixels (comme `max-height`) ; au-delà, elle défile. */
export const COMPOSER_MAX_HEIGHT = 200;

/**
 * Zone de saisie : vignettes des images jointes, texte (hauteur automatique, Entrée envoie,
 * Maj + Entrée va à la ligne), puis une barre d'outils : trombone (modèles avec la vision),
 * bascule « Réflexion » (modèles qui savent réfléchir), « Instructions » et « Envoyer », qui
 * devient « Arrêter » pendant la réponse (Échap aussi, raccourci de la coque). Coller une image
 * la joint.
 */
@Component({
  selector: 'app-composer',
  imports: [Button, Icon, Switch, Tooltip],
  templateUrl: './composer.html',
  styleUrl: './composer.scss',
})
export class Composer {
  protected readonly store = inject(ChatStore);
  private readonly models = inject(ModelsStore);
  private readonly toast = inject(ToastService);
  private readonly dialogs = inject(DialogService);
  private readonly pendingTasks = inject(PendingTasks);
  private readonly input = viewChild.required<ElementRef<HTMLTextAreaElement>>('input');
  private readonly instructionsButton = viewChild('instructions', { read: ElementRef });
  protected readonly accept = IMAGE_ACCEPT;
  protected readonly maxImages = MAX_IMAGES;

  private readonly label = computed(() => {
    const m = this.models.selected();
    return m ? modelLabel(this.models.models(), m.name) : '';
  });
  /** Le modèle choisi accepte des images. */
  protected readonly vision = computed(() => !!this.models.selected()?.vision);
  /** Le modèle choisi sait réfléchir. */
  protected readonly thinking = computed(() => !!this.models.selected()?.thinking);

  /** Raison qui empêche d'envoyer, à part un message vide. */
  protected readonly blocked = computed<string | null>(() => {
    if (!this.models.selected()) {
      return 'Aucun modèle disponible : lancer Ollama ou ajouter un modèle depuis GRAM, puis Actualiser';
    }
    if (this.store.busy() && !this.store.streamingHere()) {
      return 'Une réponse est en cours dans une autre conversation';
    }
    if (this.store.attachments().length > 0 && !this.vision()) {
      return `${this.label()} n’accepte pas d’images : retirer les images, ou choisir un modèle avec la vision`;
    }
    return null;
  });
  protected readonly canSend = computed(
    () =>
      !this.blocked() &&
      !this.store.busy() &&
      (this.store.draft().trim().length > 0 || this.store.attachments().length > 0),
  );
  protected readonly hint = computed(() => {
    if (this.store.streamingHere()) return 'Réponse en cours · Échap pour l’arrêter';
    const blocked = this.blocked();
    if (blocked) return blocked;
    if (this.store.hasImages() && !this.vision()) {
      return `Les images de la conversation ne sont pas envoyées à ${this.label()} (pas de vision)`;
    }
    return 'Entrée pour envoyer · Maj + Entrée pour aller à la ligne';
  });
  protected readonly placeholder = computed(() => {
    const label = this.label();
    return label ? `Écrire à ${label}…` : 'Écrire un message…';
  });
  protected readonly instructionsTip = computed(() => {
    const text = this.store.instructions();
    if (!text) return 'Instructions de la conversation : aucune';
    return `Instructions : ${text.length > 80 ? `${text.slice(0, 79)}…` : text}`;
  });

  constructor() {
    // Hauteur suivant le texte, y compris quand il est vidé après l'envoi.
    afterRenderEffect(() => {
      this.store.draft();
      this.resize();
    });
    afterRenderEffect(() => {
      if (this.store.focusRequest() > 0) this.input().nativeElement.focus();
    });
  }

  protected onInput(event: Event): void {
    this.store.draft.set((event.target as HTMLTextAreaElement).value);
    this.resize();
  }

  protected onKeydown(event: KeyboardEvent): void {
    // Entrée pendant une composition (accents, IME) : elle la valide, sans envoyer.
    if (event.key !== 'Enter' || event.shiftKey || event.isComposing || event.keyCode === 229) {
      return;
    }
    event.preventDefault();
    this.submit();
  }

  /**
   * Coller une image (capture d'écran…) la joint. Du texte brut collé l'emporte : un tableur
   * met aussi une image de ses cellules dans le presse-papiers.
   */
  protected onPaste(event: ClipboardEvent): void {
    const files = filesOf(event.clipboardData).filter((f) => f.type.startsWith('image/'));
    if (files.length === 0 || event.clipboardData?.getData('text/plain')) return;
    event.preventDefault();
    void this.addFiles(files);
  }

  protected onFiles(event: Event): void {
    const input = event.target as HTMLInputElement;
    const files = Array.from(input.files ?? []);
    // Le même fichier peut être choisi de nouveau.
    input.value = '';
    void this.addFiles(files);
  }

  /** Joint des images ; un refus s'affiche en toast. */
  async addFiles(files: readonly File[]): Promise<void> {
    const problem = await this.store.attach(files);
    if (problem) this.toast.show(problem, { kind: 'error' });
    this.input().nativeElement.focus();
  }

  protected detach(id: number): void {
    this.store.detach(id);
    this.input().nativeElement.focus();
  }

  protected setThink(think: boolean): void {
    this.store.setThink(think).catch((e: unknown) => {
      this.toast.show(`Réglage non enregistré : ${errorMessage(e)}`, { kind: 'error' });
    });
  }

  /** Dialogue « Instructions », chargé à la demande (tâche suivie par Angular). */
  protected openInstructions(): void {
    const origin = this.instructionsButton()?.nativeElement as HTMLElement | undefined;
    const draft = !this.store.currentId();
    this.pendingTasks.run(async () => {
      const { InstructionsDialog } = await import('./instructions-dialog');
      const ref = this.dialogs.open<string>(InstructionsDialog, {
        data: { instructions: this.store.instructions(), draft },
        width: '560px',
        autoFocus: '[data-testid="instructions-input"]',
        restoreFocus: origin ?? true,
      });
      ref.closed.subscribe((text) => {
        if (text === undefined) return;
        this.store.setInstructions(text).then(
          () =>
            this.toast.show(text.trim() ? 'Instructions enregistrées' : 'Instructions retirées', {
              kind: 'success',
            }),
          (e: unknown) =>
            this.toast.show(`Instructions non enregistrées : ${errorMessage(e)}`, {
              kind: 'error',
            }),
        );
      });
    });
  }

  protected submit(): void {
    if (!this.canSend()) return;
    void this.store.send(this.store.draft());
  }

  protected stop(): void {
    void this.store.stop();
    this.input().nativeElement.focus();
  }

  private resize(): void {
    const el = this.input().nativeElement;
    el.style.height = 'auto';
    el.style.height = `${Math.min(el.scrollHeight, COMPOSER_MAX_HEIGHT)}px`;
  }
}
