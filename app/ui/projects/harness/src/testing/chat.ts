// Faux backend de la discussion (tests, vérification visuelle) : il enregistre les
// conversations en mémoire et répond par le canal de la requête, comme
// `app/harness-tauri/src/chat.rs` (`started`, `delta`…, `done`).

import type { Channel } from '@tauri-apps/api/core';
import type { ChatEvent } from '@gram/types/ChatEvent';
import type { ChatMessage } from '@gram/types/ChatMessage';
import type { ChatSend } from '@gram/types/ChatSend';
import type { Conversation } from '@gram/types/Conversation';
import type { ConversationSummary } from '@gram/types/ConversationSummary';
import type { Handler } from './backend';

/** Réponse en cours, que le test fait avancer (`delta`, `finish`). */
export interface ActiveReply {
  request: ChatSend;
  conversation: Conversation;
  content: string;
  thinking: string;
}

const copy = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

export function summary(c: Conversation): ConversationSummary {
  return {
    id: c.id,
    title: c.title,
    model: c.model,
    createdAt: c.createdAt,
    updatedAt: c.updatedAt,
  };
}

export class FakeChat {
  readonly conversations = new Map<string, Conversation>();
  /** Réponse automatique : réflexion puis texte, en morceaux. */
  reply = { thinking: '', content: 'Bonjour ! **Comment** puis-je vous aider ?' };
  /** La réponse attend le test (`delta`, `finish`, `fail`) au lieu de partir seule. */
  manual = false;
  /** Requête refusée avant toute réponse, avec ce message. */
  refuse: string | null = null;
  active: ActiveReply | null = null;
  readonly requests: ChatSend[] = [];
  /** Images enregistrées, en adresses `data:`, par `conversation/nom`. */
  readonly images = new Map<string, string>();
  private channel: Channel<ChatEvent> | null = null;
  private resolve: (() => void) | null = null;
  private nextId = 1;

  constructor(conversations: readonly Conversation[] = []) {
    for (const c of conversations) this.conversations.set(c.id, copy(c));
  }

  handlers(): Record<string, Handler> {
    return {
      chat_send: (a) => this.send(a['request'] as ChatSend, a['onEvent'] as Channel<ChatEvent>),
      chat_stop: (a) => this.stop(a['conversationId'] as string),
      list_conversations: () =>
        [...this.conversations.values()]
          .map(summary)
          .sort((x, y) => y.updatedAt.localeCompare(x.updatedAt)),
      get_conversation: (a) => copy(this.get(a['id'] as string)),
      rename_conversation: (a) => {
        const c = this.get(a['id'] as string);
        c.title = (a['title'] as string).trim();
        return summary(c);
      },
      set_instructions: (a) => {
        const instructions = ((a['instructions'] as string | null) ?? '').trim();
        this.get(a['id'] as string).instructions = instructions || null;
        return null;
      },
      set_think: (a) => {
        this.get(a['id'] as string).think = a['think'] as boolean;
        return null;
      },
      get_image: (a) => {
        const url = this.images.get(`${a['conversationId'] as string}/${a['name'] as string}`);
        if (!url)
          throw 'Image introuvable : elle a peut-être été supprimée du dossier de la conversation';
        return url;
      },
      delete_conversation: (a) => {
        if (this.active?.conversation.id === a['id']) this.stop(a['id'] as string);
        return this.conversations.delete(a['id'] as string);
      },
    };
  }

  /** Morceau de réponse ou de réflexion. */
  delta(content: string, thinking = ''): void {
    const active = this.require();
    active.content += content;
    active.thinking += thinking;
    this.emit({ event: 'delta', data: { content, thinking } });
  }

  /** Fin de la réponse (statistiques par défaut, ou `patch`). */
  finish(patch: Partial<ChatMessage> = {}): ChatMessage {
    const active = this.require();
    const message: ChatMessage = {
      role: 'assistant',
      content: active.content,
      images: [],
      thinking: active.thinking || null,
      thinkingMs: active.thinking ? 1200 : null,
      model: active.request.model,
      stats: {
        doneReason: 'stop',
        promptEvalCount: 18,
        evalCount: 42,
        evalDuration: 252_300_000,
        loadDuration: 2_000_000,
        promptEvalDuration: 12_000_000,
        totalDuration: 280_000_000,
        contextLength: 16384,
      },
      interrupted: false,
      error: null,
      createdAt: new Date().toISOString(),
      ...patch,
    };
    active.conversation.messages.push(message);
    active.conversation.updatedAt = message.createdAt;
    const channel = this.channel;
    const resolve = this.resolve;
    this.active = null;
    this.channel = null;
    this.resolve = null;
    channel?.onmessage({ event: 'done', data: { message: copy(message) } });
    resolve?.();
    return message;
  }

  /** Échec en cours de route (Ollama arrêté…) : le début est gardé. */
  fail(error: string): ChatMessage {
    return this.finish({ error, stats: null });
  }

  private send(request: ChatSend, channel: Channel<ChatEvent>): Promise<void> {
    this.requests.push(copy(request));
    if (this.refuse) throw this.refuse;
    if (this.active?.conversation.id === request.conversationId) {
      throw 'Une réponse est déjà en cours dans cette conversation : l’arrêter d’abord';
    }
    const now = new Date().toISOString();
    let conversation: Conversation;
    let saved: ChatMessage | null = null;
    if (request.conversationId) {
      conversation = this.get(request.conversationId);
      if (request.content !== null) {
        saved = user(request.content, now);
        conversation.messages.push(saved);
      } else if (conversation.messages.at(-1)?.role === 'assistant') {
        conversation.messages.pop();
      }
      conversation.model = request.model;
      if (request.think !== null) conversation.think = request.think;
      conversation.updatedAt = now;
    } else {
      const content = request.content ?? '';
      saved = user(content, now);
      conversation = {
        id: `nouvelle-${this.nextId++}`,
        title: content.slice(0, 60) || 'Nouvelle conversation',
        model: request.model,
        instructions: request.instructions?.trim() || null,
        think: request.think,
        createdAt: now,
        updatedAt: now,
        messages: [saved],
      };
      this.conversations.set(conversation.id, conversation);
    }
    // Images enregistrées sous un nom de fichier, comme le backend.
    if (saved) {
      saved.images = request.images.map((data, i) => {
        const name = `${now.replace(/\D/g, '').slice(0, 17)}-${i + 1}.png`;
        this.images.set(`${conversation.id}/${name}`, `data:image/png;base64,${data}`);
        return name;
      });
    }
    this.active = { request, conversation, content: '', thinking: '' };
    this.channel = channel;
    channel.onmessage({
      event: 'started',
      data: { conversation: summary(conversation), message: saved && copy(saved) },
    });
    const done = new Promise<void>((resolve) => (this.resolve = resolve));
    if (!this.manual) {
      if (this.reply.thinking) this.delta('', this.reply.thinking);
      const middle = Math.ceil(this.reply.content.length / 2);
      this.delta(this.reply.content.slice(0, middle));
      this.delta(this.reply.content.slice(middle));
      this.finish();
    }
    return done;
  }

  private stop(id: string): boolean {
    if (this.active?.conversation.id !== id) return false;
    this.finish({ interrupted: true, stats: null });
    return true;
  }

  private get(id: string): Conversation {
    const c = this.conversations.get(id);
    if (!c) throw 'Conversation introuvable : elle a peut-être été supprimée';
    return c;
  }

  private require(): ActiveReply {
    if (!this.active) throw new Error('aucune réponse en cours');
    return this.active;
  }

  private emit(event: ChatEvent): void {
    this.channel?.onmessage(event);
  }
}

function user(content: string, at: string): ChatMessage {
  return {
    role: 'user',
    content,
    images: [],
    thinking: null,
    thinkingMs: null,
    model: null,
    stats: null,
    interrupted: false,
    error: null,
    createdAt: at,
  };
}
