import { Injectable, InjectionToken, computed, inject, signal } from '@angular/core';
import { Channel } from '@tauri-apps/api/core';
import type { ChatEvent } from '@gram/types/ChatEvent';
import type { ChatMessage } from '@gram/types/ChatMessage';
import type { ChatSend } from '@gram/types/ChatSend';
import type { Conversation } from '@gram/types/Conversation';
import type { ConversationSummary } from '@gram/types/ConversationSummary';
import { HarnessApi, errorMessage } from '../core/harness-api.service';
import { ModelsStore } from './models.store';

/**
 * Report à la prochaine image : le message en cours est redessiné au plus une fois par image,
 * quel que soit le nombre de morceaux reçus.
 */
export const NEXT_FRAME = new InjectionToken<(run: () => void) => void>('NEXT_FRAME', {
  factory: () => (run) => {
    if (typeof requestAnimationFrame === 'function') requestAnimationFrame(() => run());
    else setTimeout(run, 16);
  },
});

/** Réponse en cours. */
export interface Streaming {
  /** Conversation ; `null` jusqu'à `started` pour une nouvelle conversation. */
  conversationId: string | null;
  model: string;
  content: string;
  thinking: string;
}

/**
 * Conversations et discussion : liste de la barre latérale, conversation affichée, réponse en
 * cours (une à la fois), texte en cours de saisie.
 *
 * Une nouvelle conversation n'existe pas avant son premier message : elle s'affiche d'abord en
 * brouillon (identifiant vide), puis prend son identifiant à `started`. Les morceaux de la
 * réponse sont réunis et appliqués une fois par image (`NEXT_FRAME`).
 */
@Injectable({ providedIn: 'root' })
export class ChatStore {
  private readonly api = inject(HarnessApi);
  private readonly models = inject(ModelsStore);
  private readonly nextFrame = inject(NEXT_FRAME);

  /** Conversations, la plus récente d'abord. */
  readonly conversations = signal<ConversationSummary[]>([]);
  readonly listError = signal<string | null>(null);
  /** Conversation affichée ; `null` : nouvelle conversation. */
  readonly current = signal<Conversation | null>(null);
  /** Conversation en cours d'ouverture. */
  readonly opening = signal<string | null>(null);
  readonly openError = signal<string | null>(null);
  readonly streaming = signal<Streaming | null>(null);
  /** Envoi refusé avant toute réponse (ou réponse non enregistrée), en français. */
  readonly sendError = signal<string | null>(null);
  /** Texte de la zone de saisie, rendu si l'envoi est refusé. */
  readonly draft = signal('');
  /** Incrémenté pour demander le focus de la zone de saisie. */
  readonly focusRequest = signal(0);

  readonly currentId = computed(() => this.current()?.id || null);
  readonly messages = computed<readonly ChatMessage[]>(() => this.current()?.messages ?? []);
  readonly busy = computed(() => this.streaming() !== null);
  /** La réponse en cours est celle de la conversation affichée (brouillon compris). */
  readonly streamingHere = computed(() => {
    const s = this.streaming();
    const c = this.current();
    return !!s && !!c && (s.conversationId ?? '') === c.id;
  });

  /** Requête en cours : les événements d'une requête précédente sont ignorés. */
  private token = 0;
  private started = false;
  private stopRequested = false;
  private pending = { content: '', thinking: '' };
  private frameScheduled = false;
  private openSeq = 0;
  /** Une réponse s'est terminée pendant l'ouverture de sa conversation : la relire. */
  private openStale = false;

  async load(): Promise<void> {
    try {
      this.conversations.set(await this.api.listConversations());
      this.listError.set(null);
    } catch (e) {
      this.listError.set(errorMessage(e));
    }
  }

  /** Affiche une conversation, et choisit son modèle s'il est encore dans Ollama. */
  async open(id: string): Promise<void> {
    if (this.current()?.id === id && !this.openError()) return;
    const seq = ++this.openSeq;
    this.openStale = false;
    this.opening.set(id);
    this.openError.set(null);
    this.sendError.set(null);
    try {
      let conversation = await this.api.getConversation(id);
      if (seq === this.openSeq && this.openStale) {
        conversation = await this.api.getConversation(id);
      }
      if (seq !== this.openSeq) return;
      this.current.set(conversation);
      if (this.models.models().some((m) => m.name === conversation.model)) {
        this.models.select(conversation.model);
      }
      this.focusRequest.update((n) => n + 1);
    } catch (e) {
      if (seq !== this.openSeq) return;
      this.openError.set(errorMessage(e));
      void this.load();
    } finally {
      if (seq === this.openSeq) this.opening.set(null);
    }
  }

  /** Nouvelle conversation (rien n'est enregistré avant le premier message). */
  newConversation(): void {
    this.openSeq++;
    this.opening.set(null);
    this.openError.set(null);
    this.sendError.set(null);
    this.current.set(null);
    this.focusRequest.update((n) => n + 1);
  }

  /** Envoie un message au modèle choisi ; `false` s'il n'est pas parti (vide, réponse en cours…). */
  async send(text: string): Promise<boolean> {
    const content = text.trim();
    const model = this.models.selected()?.name;
    if (!content || !model || this.busy()) return false;
    const now = new Date().toISOString();
    const user: ChatMessage = {
      role: 'user',
      content,
      thinking: null,
      model: null,
      stats: null,
      interrupted: false,
      error: null,
      createdAt: now,
    };
    const current = this.current();
    this.current.set(
      current
        ? { ...current, messages: [...current.messages, user] }
        : {
            id: '',
            title: content,
            model,
            instructions: null,
            createdAt: now,
            updatedAt: now,
            messages: [user],
          },
    );
    this.draft.set('');
    await this.run({ conversationId: current?.id || null, model, content });
    return true;
  }

  /** Retire la dernière réponse (ou l'échec) et la redemande au modèle choisi. */
  async regenerate(): Promise<void> {
    const c = this.current();
    const model = this.models.selected()?.name;
    if (!c?.id || !model || this.busy()) return;
    const messages = [...c.messages];
    if (messages.at(-1)?.role === 'assistant') messages.pop();
    if (messages.at(-1)?.role !== 'user') return;
    this.current.set({ ...c, messages });
    await this.run({ conversationId: c.id, model, content: null });
  }

  /** Arrête la réponse en cours ; son début est gardé. */
  async stop(): Promise<void> {
    const s = this.streaming();
    if (!s) return;
    if (!s.conversationId) {
      this.stopRequested = true;
      return;
    }
    try {
      await this.api.chatStop(s.conversationId);
    } catch (e) {
      this.sendError.set(errorMessage(e));
    }
  }

  async rename(id: string, title: string): Promise<void> {
    const summary = await this.api.renameConversation(id, title);
    this.conversations.update((list) => list.map((c) => (c.id === id ? summary : c)));
    const c = this.current();
    if (c?.id === id) this.current.set({ ...c, title: summary.title });
  }

  async remove(id: string): Promise<void> {
    await this.api.deleteConversation(id);
    this.conversations.update((list) => list.filter((c) => c.id !== id));
    if (this.current()?.id === id) this.newConversation();
  }

  private async run(request: ChatSend): Promise<void> {
    const token = ++this.token;
    this.started = false;
    this.stopRequested = false;
    this.pending = { content: '', thinking: '' };
    this.sendError.set(null);
    this.streaming.set({
      conversationId: request.conversationId,
      model: request.model,
      content: '',
      thinking: '',
    });
    const channel = new Channel<ChatEvent>();
    channel.onmessage = (event) => this.onEvent(token, event);
    try {
      await this.api.chatSend(request, channel);
    } catch (e) {
      if (token !== this.token) return;
      this.sendError.set(errorMessage(e));
      if (!this.started) await this.undo(request);
    } finally {
      if (token === this.token && this.streaming()) this.streaming.set(null);
    }
  }

  /** Envoi refusé avant toute réponse : la conversation redevient ce qu'elle était. */
  private async undo(request: ChatSend): Promise<void> {
    this.streaming.set(null);
    if (request.content && !this.draft()) this.draft.set(request.content);
    const c = this.current();
    if (!request.conversationId) {
      if (c?.id === '') this.current.set(null);
      return;
    }
    if (c?.id !== request.conversationId) return;
    try {
      const fresh = await this.api.getConversation(request.conversationId);
      if (this.current()?.id === fresh.id) this.current.set(fresh);
    } catch {
      // Message déjà affiché ; la conversation reste en l'état.
    }
  }

  private onEvent(token: number, event: ChatEvent): void {
    if (token !== this.token) return;
    switch (event.event) {
      case 'started': {
        const summary = event.data.conversation;
        this.started = true;
        this.streaming.update((s) => s && { ...s, conversationId: summary.id });
        const c = this.current();
        if (c && (c.id === summary.id || c.id === '')) {
          this.current.set({
            ...c,
            id: summary.id,
            title: summary.title,
            model: summary.model,
            updatedAt: summary.updatedAt,
          });
        }
        this.conversations.update((list) => [summary, ...list.filter((x) => x.id !== summary.id)]);
        if (this.stopRequested) void this.stop();
        break;
      }
      case 'delta':
        this.pending.content += event.data.content;
        this.pending.thinking += event.data.thinking;
        if (!this.frameScheduled) {
          this.frameScheduled = true;
          this.nextFrame(() => {
            this.frameScheduled = false;
            if (token === this.token) this.flush();
          });
        }
        break;
      case 'done': {
        this.flush();
        const message = event.data.message;
        const id = this.streaming()?.conversationId;
        this.streaming.set(null);
        // Ollama arrêté, modèle disparu : la liste des modèles et l'état d'Ollama le disent.
        if (message.error) void this.models.refresh();
        if (!id) break;
        const c = this.current();
        if (c?.id === id) {
          this.current.set({
            ...c,
            messages: [...c.messages, message],
            updatedAt: message.createdAt,
          });
        } else if (this.opening() === id) {
          this.openStale = true;
        }
        this.conversations.update((list) => {
          const found = list.find((x) => x.id === id);
          return found
            ? [{ ...found, updatedAt: message.createdAt }, ...list.filter((x) => x.id !== id)]
            : list;
        });
        break;
      }
    }
  }

  /** Applique les morceaux reçus depuis la dernière image. */
  private flush(): void {
    const { content, thinking } = this.pending;
    if (!content && !thinking) return;
    this.pending = { content: '', thinking: '' };
    this.streaming.update(
      (s) => s && { ...s, content: s.content + content, thinking: s.thinking + thinking },
    );
  }
}
