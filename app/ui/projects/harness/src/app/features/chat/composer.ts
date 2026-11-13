import {
  Component,
  ElementRef,
  afterRenderEffect,
  computed,
  inject,
  viewChild,
} from '@angular/core';
import { Button } from '@gram/ui/button/button';
import { Icon } from '@gram/ui/icon/icon';
import { Tooltip } from '@gram/ui/tooltip/tooltip';
import { modelLabel } from '../../shared/conversations';
import { ChatStore } from '../../state/chat.store';
import { ModelsStore } from '../../state/models.store';

/** Hauteur maximale de la zone de saisie, en pixels (comme `max-height`) ; au-delà, elle défile. */
export const COMPOSER_MAX_HEIGHT = 200;

/**
 * Zone de saisie : hauteur automatique, Entrée envoie, Maj + Entrée va à la ligne ;
 * « Envoyer » devient « Arrêter » pendant la réponse (Échap aussi, raccourci de la coque).
 */
@Component({
  selector: 'app-composer',
  imports: [Button, Icon, Tooltip],
  template: `
    <div class="box">
      <textarea
        #input
        class="input"
        rows="1"
        data-testid="composer-input"
        aria-label="Message"
        aria-describedby="composer-hint"
        [placeholder]="placeholder()"
        [value]="store.draft()"
        (input)="onInput($event)"
        (keydown)="onKeydown($event)"
      ></textarea>
      @if (store.streamingHere()) {
        <button
          gButton
          class="action"
          data-testid="composer-stop"
          gTooltip="Arrêter la réponse"
          gTooltipShortcut="Échap"
          (click)="stop()"
        >
          <g-icon name="stop" [size]="16" />
          Arrêter
        </button>
      } @else {
        <button
          gButton
          class="action"
          variant="primary"
          data-testid="composer-send"
          [disabled]="!canSend()"
          (click)="submit()"
        >
          <g-icon name="arrow-up" [size]="16" />
          Envoyer
        </button>
      }
    </div>
    <p id="composer-hint" class="hint" data-testid="composer-hint">{{ hint() }}</p>
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      gap: 6px;
    }
    .box {
      display: flex;
      align-items: flex-end;
      gap: 8px;
      padding: 8px 8px 8px 14px;
      border: 1px solid var(--g-input);
      border-radius: 12px;
      background: var(--g-field);
      box-shadow: var(--g-shadow-pop);
    }
    .box:focus-within {
      border-color: var(--g-accent);
      box-shadow:
        0 0 0 1px var(--g-accent),
        var(--g-shadow-pop);
    }
    // Une ligne de 34 px (hauteur du bouton), puis la hauteur du texte, bornée.
    .input {
      flex: 1;
      min-width: 0;
      min-height: 34px;
      max-height: 200px;
      padding: 7px 0;
      border: 0;
      background: transparent;
      color: var(--g-fg);
      font: inherit;
      font-size: 14px;
      line-height: 20px;
      resize: none;
      overflow-y: auto;
      outline: none;
    }
    .input::placeholder {
      color: var(--g-muted);
    }
    .action {
      flex: none;
    }
    .hint {
      margin: 0;
      padding: 0 4px;
      font-size: 11.5px;
      color: var(--g-muted);
      text-align: center;
    }
  `,
})
export class Composer {
  protected readonly store = inject(ChatStore);
  private readonly models = inject(ModelsStore);
  private readonly input = viewChild.required<ElementRef<HTMLTextAreaElement>>('input');

  /** Raison qui empêche d'envoyer, à part un texte vide. */
  protected readonly blocked = computed<string | null>(() => {
    if (!this.models.selected()) {
      return 'Aucun modèle disponible : lancer Ollama ou ajouter un modèle depuis GRAM, puis Actualiser';
    }
    if (this.store.busy() && !this.store.streamingHere()) {
      return 'Une réponse est en cours dans une autre conversation';
    }
    return null;
  });
  protected readonly canSend = computed(
    () => !this.blocked() && !this.store.busy() && this.store.draft().trim().length > 0,
  );
  protected readonly hint = computed(() => {
    if (this.store.streamingHere()) return 'Réponse en cours · Échap pour l’arrêter';
    return this.blocked() ?? 'Entrée pour envoyer · Maj + Entrée pour aller à la ligne';
  });
  protected readonly placeholder = computed(() => {
    const m = this.models.selected();
    return m ? `Écrire à ${modelLabel(this.models.models(), m.name)}…` : 'Écrire un message…';
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
