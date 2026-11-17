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
import { draggingFiles, filesOf } from '../../shared/attachments';
import { modelLabel } from '../../shared/conversations';
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
 * remonté : « ↓ Nouveau contenu » le ramène en bas. Des images glissées sur la zone sont
 * jointes au message ; un encadré prévient quand le contexte est presque plein.
 */
@Component({
  selector: 'app-chat-view',
  imports: [Button, Callout, Composer, Home, Icon, Message, Spinner],
  templateUrl: './chat-view.html',
  styleUrl: './chat-view.scss',
  host: {
    '(dragenter)': 'onDragEnter($event)',
    '(dragover)': 'onDragOver($event)',
    '(dragleave)': 'onDragLeave($event)',
    '(drop)': 'onDrop($event)',
  },
})
export class ChatView {
  protected readonly store = inject(ChatStore);
  protected readonly models = inject(ModelsStore);
  private readonly scroller = viewChild.required<ElementRef<HTMLElement>>('scroller');
  private readonly thread = viewChild<ElementRef<HTMLElement>>('thread');
  private readonly composer = viewChild(Composer);

  protected readonly showHome = computed(
    () => this.store.messages().length === 0 && !this.store.opening() && !this.store.openError(),
  );
  protected readonly showComposer = computed(
    () => this.models.selected() !== null || this.store.messages().length > 0,
  );
  /** Horloge de la réflexion en cours, avancée chaque seconde. */
  private readonly now = signal(Date.now());
  /** Réponse en cours de la conversation affichée, comme un message. */
  protected readonly streamingMessage = computed<ChatMessage | null>(() => {
    const s = this.store.streaming();
    if (!s || !this.store.streamingHere()) return null;
    const from = s.thinkingFrom;
    return {
      role: 'assistant',
      content: s.content,
      images: [],
      thinking: s.thinking || null,
      thinkingMs: from === null ? null : Math.max(0, (s.thinkingTo ?? this.now()) - from),
      model: s.model,
      stats: null,
      interrupted: false,
      error: null,
      createdAt: '',
    };
  });
  /** Du contenu est arrivé pendant que l'utilisateur lisait plus haut. */
  protected readonly newContent = signal(false);
  /** Des fichiers sont glissés au-dessus de la zone. */
  protected readonly dragging = signal(false);
  /** Le modèle choisi accepte des images (texte de la zone de dépôt). */
  protected readonly dropText = computed(() => {
    const m = this.models.selected();
    if (!m || m.vision) return 'Déposer les images pour les joindre au message';
    return `${modelLabel(this.models.models(), m.name)} n’accepte pas d’images`;
  });
  /** Le fil suit le bas (l'utilisateur n'est pas remonté). */
  private stick = true;
  /** `dragenter` et `dragleave` arrivent aussi des éléments intérieurs. */
  private dragDepth = 0;

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
    // Réflexion en cours : sa durée avance chaque seconde.
    effect((onCleanup) => {
      const s = this.store.streaming();
      if (!s || s.thinkingFrom === null || s.thinkingTo !== null) return;
      this.now.set(Date.now());
      const timer = setInterval(() => this.now.set(Date.now()), 1000);
      onCleanup(() => clearInterval(timer));
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

  protected onDragEnter(event: DragEvent): void {
    if (!draggingFiles(event) || !this.showComposer()) return;
    event.preventDefault();
    this.dragDepth++;
    this.dragging.set(true);
  }

  protected onDragOver(event: DragEvent): void {
    if (!draggingFiles(event) || !this.showComposer()) return;
    // Sans quoi le dépôt n'est pas permis.
    event.preventDefault();
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = this.models.selected()?.vision ? 'copy' : 'none';
    }
  }

  protected onDragLeave(event: DragEvent): void {
    if (!draggingFiles(event)) return;
    this.dragDepth = Math.max(0, this.dragDepth - 1);
    if (this.dragDepth === 0) this.dragging.set(false);
  }

  protected onDrop(event: DragEvent): void {
    if (!draggingFiles(event)) return;
    event.preventDefault();
    this.dragDepth = 0;
    this.dragging.set(false);
    const files = filesOf(event.dataTransfer);
    if (files.length > 0) void this.composer()?.addFiles(files);
  }

  private onGrow(): void {
    if (this.stick) this.scrollToBottom();
    else if (this.store.streamingHere()) this.newContent.set(true);
  }
}
