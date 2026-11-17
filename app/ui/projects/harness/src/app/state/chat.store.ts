import { Injectable, InjectionToken, computed, inject, signal } from '@angular/core';
import { Channel } from '@tauri-apps/api/core';
import type { ChatEvent } from '@gram/types/ChatEvent';
import type { ChatMessage } from '@gram/types/ChatMessage';
import type { ChatSend } from '@gram/types/ChatSend';
import type { Conversation } from '@gram/types/Conversation';
import type { ConversationSummary } from '@gram/types/ConversationSummary';
import { HarnessApi, errorMessage } from '../core/harness-api.service';
import {
  type Attachment,
  MAX_IMAGES,
  base64Of,
  imageProblem,
  readDataUrl,
} from '../shared/attachments';
import { modelLabel } from '../shared/conversations';
import { contextUsage } from '../shared/stats';
import { ImagesStore } from './images.store';
import { ModelsStore } from './models.store';
import { PrefsStore } from './prefs.store';

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
  /** Premier morceau de la réflexion, puis de la réponse (`Date.now()`) : durée en direct. */
  thinkingFrom: number | null;
  thinkingTo: number | null;
}

/**
 * Conversations et discussion : liste de la barre latérale, conversation affichée, réponse en
 * cours (une à la fois), texte et images en cours de saisie, réglages de la conversation
 * (instructions, réflexion).
 *
 * Une nouvelle conversation n'existe pas avant son premier message : elle s'affiche d'abord en
 * brouillon (identifiant vide), puis prend son identifiant à `started`. Ses instructions et sa
 * réflexion partent avec ce premier message (celles des paramètres, sauf changement). Les
 * morceaux de la réponse sont réunis et appliqués une fois par image (`NEXT_FRAME`).
 */
@Injectable({ providedIn: 'root' })
export class ChatStore {
  private readonly api = inject(HarnessApi);
  private readonly models = inject(ModelsStore);
  private readonly prefs = inject(PrefsStore);
  private readonly images = inject(ImagesStore);
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
  /** Images jointes au message en cours de saisie, rendues si l'envoi est refusé. */
  readonly attachments = signal<Attachment[]>([]);
  /** Incrémenté pour demander le focus de la zone de saisie. */
  readonly focusRequest = signal(0);

  /** Réglages d'une nouvelle conversation changés avant son premier message. */
  private readonly draftInstructions = signal<string | null>(null);
  private readonly draftThink = signal<boolean | null>(null);

  readonly currentId = computed(() => this.current()?.id || null);
  readonly messages = computed<readonly ChatMessage[]>(() => this.current()?.messages ?? []);
  readonly busy = computed(() => this.streaming() !== null);
  /** La réponse en cours est celle de la conversation affichée (brouillon compris). */
  readonly streamingHere = computed(() => {
    const s = this.streaming();
    const c = this.current();
    return !!s && !!c && (s.conversationId ?? '') === c.id;
  });
  /** Instructions de la conversation affichée (ou de la nouvelle) ; vides : aucune. */
  readonly instructions = computed(() => {
    const c = this.current();
    if (c?.id) return c.instructions ?? '';
    return this.draftInstructions() ?? this.prefs.instructions();
  });
  /** Bascule « Réflexion » de la conversation affichée (ou de la nouvelle). */
  readonly think = computed(() => {
    const c = this.current();
    if (c?.id) return c.think ?? this.prefs.think();
    return this.draftThink() ?? this.prefs.think();
  });
  /** Contexte occupé à la fin de la dernière réponse mesurée. */
  readonly context = computed(() => contextUsage(this.messages(), this.models.models()));
  /** La conversation affichée contient des images. */
  readonly hasImages = computed(() => this.messages().some((m) => m.images.length > 0));

  /** Requête en cours : les événements d'une requête précédente sont ignorés. */
  private token = 0;
  private started = false;
  private stopRequested = false;
  private pending = { content: '', thinking: '' };
  private frameScheduled = false;
  private openSeq = 0;
  /** Une réponse s'est terminée pendant l'ouverture de sa conversation : la relire. */
  private openStale = false;
  private nextAttachment = 1;

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

  /**
   * Nouvelle conversation (rien n'est enregistré avant le premier message), avec les
   * instructions par défaut et la dernière position de « Réflexion ».
   */
  newConversation(): void {
    this.openSeq++;
    this.opening.set(null);
    this.openError.set(null);
    this.sendError.set(null);
    this.draftInstructions.set(null);
    this.draftThink.set(null);
    this.current.set(null);
    this.focusRequest.update((n) => n + 1);
  }

  /**
   * Joint des images au message en cours de saisie (trombone, coller, glisser-déposer). Renvoie
   * ce qui a été refusé, en français (`null` : tout est joint) : modèle sans vision, format,
   * taille, nombre.
   */
  async attach(files: readonly File[]): Promise<string | null> {
    if (files.length === 0) return null;
    const model = this.models.selected();
    if (model && !model.vision) {
      const label = modelLabel(this.models.models(), model.name);
      return `${label} n’accepte pas d’images : choisir un modèle avec la vision`;
    }
    const problems: string[] = [];
    const accepted: File[] = [];
    for (const file of files) {
      const problem = imageProblem(file);
      if (problem) problems.push(problem);
      else accepted.push(file);
    }
    const room = MAX_IMAGES - this.attachments().length;
    if (accepted.length > room) {
      problems.push(`${MAX_IMAGES} images au plus par message`);
      accepted.splice(Math.max(room, 0));
    }
    const read = await Promise.all(
      accepted.map(async (file) => {
        try {
          return { file, url: await readDataUrl(file) };
        } catch {
          problems.push(`${file.name} : lecture impossible`);
          return null;
        }
      }),
    );
    const added = read
      .filter((r) => r !== null)
      .map(({ file, url }) => ({
        id: this.nextAttachment++,
        name: file.name,
        size: file.size,
        url,
      }));
    this.attachments.update((list) => [...list, ...added].slice(0, MAX_IMAGES));
    return problems.length > 0 ? problems.join(' · ') : null;
  }

  detach(id: number): void {
    this.attachments.update((list) => list.filter((a) => a.id !== id));
  }

  /**
   * Bascule « Réflexion » : retenue par la conversation (enregistrée tout de suite) et reprise
   * par les nouvelles conversations. Rejette si la conversation n'a pas pu être enregistrée.
   */
  async setThink(think: boolean): Promise<void> {
    this.prefs.setThink(think);
    const c = this.current();
    if (!c?.id) {
      this.draftThink.set(think);
      return;
    }
    this.current.set({ ...c, think });
    await this.api.setThink(c.id, think);
  }

  /**
   * Instructions de la conversation affichée ; pour une nouvelle conversation, elles partiront
   * avec le premier message. Rejette si elles n'ont pas pu être enregistrées.
   */
  async setInstructions(text: string): Promise<void> {
    const instructions = text.trim();
    const c = this.current();
    if (!c?.id) {
      this.draftInstructions.set(instructions);
      return;
    }
    await this.api.setInstructions(c.id, instructions || null);
    const now = this.current();
    if (now?.id === c.id) this.current.set({ ...now, instructions: instructions || null });
  }

  /**
   * Envoie le message (texte et images jointes) au modèle choisi ; `false` s'il n'est pas parti
   * (vide, réponse en cours, images pour un modèle sans vision…).
   */
  async send(text: string): Promise<boolean> {
    const content = text.trim();
    const model = this.models.selected();
    const attachments = this.attachments();
    if ((!content && attachments.length === 0) || !model || this.busy()) return false;
    if (attachments.length > 0 && !model.vision) return false;
    const now = new Date().toISOString();
    // Vignettes affichées par leurs adresses `data:`, jusqu'au message enregistré (`started`).
    const user = userMessage(
      content,
      attachments.map((a) => a.url),
      now,
    );
    const current = this.current();
    const instructions = current?.id ? null : this.instructions() || null;
    const think = model.thinking ? this.think() : null;
    this.current.set(
      current
        ? { ...current, messages: [...current.messages, user] }
        : {
            id: '',
            title: content,
            model: model.name,
            instructions,
            think,
            createdAt: now,
            updatedAt: now,
            messages: [user],
          },
    );
    this.draft.set('');
    this.attachments.set([]);
    await this.run(
      {
        conversationId: current?.id || null,
        model: model.name,
        content,
        images: attachments.map((a) => base64Of(a.url)),
        think,
        keepAlive: this.prefs.keepAlive(),
        instructions,
      },
      attachments,
    );
    return true;
  }

  /** Retire la dernière réponse (ou l'échec) et la redemande au modèle choisi. */
  async regenerate(): Promise<void> {
    const c = this.current();
    const model = this.models.selected();
    if (!c?.id || !model || this.busy()) return;
    const messages = [...c.messages];
    if (messages.at(-1)?.role === 'assistant') messages.pop();
    if (messages.at(-1)?.role !== 'user') return;
    this.current.set({ ...c, messages });
    await this.run(
      {
        conversationId: c.id,
        model: model.name,
        content: null,
        images: [],
        think: model.thinking ? this.think() : null,
        keepAlive: this.prefs.keepAlive(),
        instructions: null,
      },
      [],
    );
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
    this.images.forget(id);
    if (this.current()?.id === id) this.newConversation();
  }

  private async run(request: ChatSend, attachments: readonly Attachment[]): Promise<void> {
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
      thinkingFrom: null,
      thinkingTo: null,
    });
    const channel = new Channel<ChatEvent>();
    channel.onmessage = (event) => this.onEvent(token, event, attachments);
    try {
      await this.api.chatSend(request, channel);
    } catch (e) {
      if (token !== this.token) return;
      this.sendError.set(errorMessage(e));
      if (!this.started) await this.undo(request, attachments);
    } finally {
      if (token === this.token && this.streaming()) this.streaming.set(null);
    }
  }

  /** Envoi refusé avant toute réponse : la conversation redevient ce qu'elle était. */
  private async undo(request: ChatSend, attachments: readonly Attachment[]): Promise<void> {
    this.streaming.set(null);
    if (request.content && !this.draft()) this.draft.set(request.content);
    if (attachments.length > 0 && this.attachments().length === 0) {
      this.attachments.set([...attachments]);
    }
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

  private onEvent(token: number, event: ChatEvent, attachments: readonly Attachment[]): void {
    if (token !== this.token) return;
    switch (event.event) {
      case 'started': {
        const { conversation: summary, message: saved } = event.data;
        this.started = true;
        this.streaming.update((s) => s && { ...s, conversationId: summary.id });
        // Images enregistrées : déjà lues, inutile de les relire.
        saved?.images.forEach((name, i) => {
          const url = attachments[i]?.url;
          if (url) this.images.seed(summary.id, name, url);
        });
        const c = this.current();
        if (c && (c.id === summary.id || c.id === '')) {
          const messages = [...c.messages];
          if (saved && messages.at(-1)?.role === 'user') messages[messages.length - 1] = saved;
          this.current.set({
            ...c,
            id: summary.id,
            title: summary.title,
            model: summary.model,
            updatedAt: summary.updatedAt,
            messages,
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
    const now = Date.now();
    this.streaming.update((s) => {
      if (!s) return s;
      const thinkingFrom = s.thinkingFrom ?? (thinking ? now : null);
      const thinkingTo = s.thinkingTo ?? (content && thinkingFrom !== null ? now : null);
      return {
        ...s,
        content: s.content + content,
        thinking: s.thinking + thinking,
        thinkingFrom,
        thinkingTo,
      };
    });
  }
}

/** Message de l'utilisateur, avant son enregistrement. */
export function userMessage(content: string, images: string[], at: string): ChatMessage {
  return {
    role: 'user',
    content,
    images,
    thinking: null,
    thinkingMs: null,
    model: null,
    stats: null,
    interrupted: false,
    error: null,
    createdAt: at,
  };
}
