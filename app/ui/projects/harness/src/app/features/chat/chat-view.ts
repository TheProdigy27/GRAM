import {
  Component,
  ElementRef,
  afterRenderEffect,
  computed,
  effect,
  inject,
  signal,
  untracked,
  viewChild,
} from '@angular/core';
import { Button } from '@gram/ui/button/button';
import { Callout } from '@gram/ui/callout/callout';
import { Icon } from '@gram/ui/icon/icon';
import { Spinner } from '@gram/ui/spinner/spinner';
import type { ChatMessage } from '@gram/types/ChatMessage';
import { ChatStore } from '../../state/chat.store';
import { ModelsStore } from '../../state/models.store';
import { Home } from '../home/home';
import { Composer } from './composer';
import { Message } from './message';

/** En deçà de cette distance du bas, le fil suit la réponse. */
const BOTTOM_SLACK = 32;

/**
 * Zone centrale : le fil de la conversation (ou, sans message, le modèle choisi et les états
 * d'Ollama), puis la zone de saisie. Le fil suit la réponse en cours, sauf si l'utilisateur est
 * remonté : « ↓ Nouveau contenu » le ramène en bas.
 */
@Component({
  selector: 'app-chat-view',
  imports: [Button, Callout, Composer, Home, Icon, Message, Spinner],
  templateUrl: './chat-view.html',
  styleUrl: './chat-view.scss',
})
export class ChatView {
  protected readonly store = inject(ChatStore);
  protected readonly models = inject(ModelsStore);
  private readonly scroller = viewChild.required<ElementRef<HTMLElement>>('scroller');
  private readonly thread = viewChild<ElementRef<HTMLElement>>('thread');

  protected readonly showHome = computed(
    () => this.store.messages().length === 0 && !this.store.opening() && !this.store.openError(),
  );
  protected readonly showComposer = computed(
    () => this.models.selected() !== null || this.store.messages().length > 0,
  );
  /** Réponse en cours de la conversation affichée, comme un message. */
  protected readonly streamingMessage = computed<ChatMessage | null>(() => {
    const s = this.store.streaming();
    if (!s || !this.store.streamingHere()) return null;
    return {
      role: 'assistant',
      content: s.content,
      thinking: s.thinking || null,
      model: s.model,
      stats: null,
      interrupted: false,
      error: null,
      createdAt: '',
    };
  });
  /** Du contenu est arrivé pendant que l'utilisateur lisait plus haut. */
  protected readonly newContent = signal(false);
  /** Le fil suit le bas (l'utilisateur n'est pas remonté). */
  private stick = true;

  constructor() {
    // Le fil grandit (morceau reçu, Markdown redessiné, message ajouté).
    effect((onCleanup) => {
      const el = this.thread()?.nativeElement;
      if (!el || typeof ResizeObserver === 'undefined') return;
      const observer = new ResizeObserver(() => this.onGrow());
      observer.observe(el);
      onCleanup(() => observer.disconnect());
    });
    // Message envoyé : le fil revient en bas et le suit.
    effect(() => {
      const last = this.store.messages().at(-1);
      if (last?.role === 'user') untracked(() => (this.stick = true));
    });
    // Autre conversation : en bas.
    afterRenderEffect(() => {
      this.store.currentId();
      untracked(() => this.scrollToBottom());
    });
  }

  protected onScroll(): void {
    const el = this.scroller().nativeElement;
    this.stick = el.scrollHeight - el.scrollTop - el.clientHeight < BOTTOM_SLACK;
    if (this.stick) this.newContent.set(false);
  }

  protected scrollToBottom(): void {
    const el = this.scroller().nativeElement;
    el.scrollTop = el.scrollHeight;
    this.stick = true;
    this.newContent.set(false);
  }

  private onGrow(): void {
    if (this.stick) this.scrollToBottom();
    else if (this.store.streamingHere()) this.newContent.set(true);
  }
}
