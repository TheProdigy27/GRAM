import { Injectable } from '@angular/core';
import { type Channel, invoke, type InvokeArgs } from '@tauri-apps/api/core';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { openUrl } from '@tauri-apps/plugin-opener';
import type { Theme } from '@gram/theme';
import type { ChatEvent } from '@gram/types/ChatEvent';
import type { ChatSend } from '@gram/types/ChatSend';
import type { Conversation } from '@gram/types/Conversation';
import type { ConversationSummary } from '@gram/types/ConversationSummary';
import type { ModelList } from '@gram/types/ModelList';

/** Message lisible d'une erreur : le backend rejette `invoke` avec une chaîne en français. */
export function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === 'string') return e;
  return String(e);
}

/** Adresse qu'un lien d'une réponse peut ouvrir : http et https seulement. */
export function isWebLink(href: string | null | undefined): href is string {
  return !!href && /^https?:\/\/[^\s]+$/i.test(href);
}

/**
 * Seul point d'accès à l'IPC Tauri de GRAM Harness (`app/harness-tauri/src/commands.rs`). Les
 * types viennent des types générés de GRAM. Toutes les promesses rejetées le sont avec une
 * `Error` dont le message est en français.
 */
@Injectable({ providedIn: 'root' })
export class HarnessApi {
  /**
   * Modèles avec lesquels discuter (ceux de GRAM d'abord) et état d'Ollama : arrêté ou absent,
   * la liste est vide.
   */
  listModels(): Promise<ModelList> {
    return this.call('list_models');
  }

  /** Ouvre la page de téléchargement d'Ollama dans le navigateur du système. */
  openOllamaDownload(): Promise<void> {
    return this.call('open_ollama_download');
  }

  /**
   * Envoie un message (ou, sans `content`, régénère la dernière réponse). La réponse arrive par
   * `onEvent` (`started`, `delta`…, `done`) ; la promesse se résout après `done`. Rejetée avant
   * tout événement si la requête est refusée ; une erreur d'Ollama arrive dans `done`.
   */
  chatSend(request: ChatSend, onEvent: Channel<ChatEvent>): Promise<void> {
    return this.call('chat_send', { request, onEvent });
  }

  /** Arrête la réponse en cours dans cette conversation ; `false` s'il n'y en a pas. */
  chatStop(conversationId: string): Promise<boolean> {
    return this.call('chat_stop', { conversationId });
  }

  /** Conversations enregistrées, la plus récente d'abord. */
  listConversations(): Promise<ConversationSummary[]> {
    return this.call('list_conversations');
  }

  getConversation(id: string): Promise<Conversation> {
    return this.call('get_conversation', { id });
  }

  renameConversation(id: string, title: string): Promise<ConversationSummary> {
    return this.call('rename_conversation', { id, title });
  }

  /**
   * Instructions de la conversation (vides : aucune), envoyées en message `system` à partir du
   * prochain message.
   */
  setInstructions(id: string, instructions: string | null): Promise<void> {
    return this.call('set_instructions', { id, instructions });
  }

  /** Retient la bascule « Réflexion » de la conversation. */
  setThink(id: string, think: boolean): Promise<void> {
    return this.call('set_think', { id, think });
  }

  /**
   * Supprime la conversation et ses images (et arrête sa réponse en cours) ; `false` si elle
   * n'existait plus.
   */
  deleteConversation(id: string): Promise<boolean> {
    return this.call('delete_conversation', { id });
  }

  /** Image jointe à un message, en adresse `data:` (seule forme que la CSP autorise). */
  getImage(conversationId: string, name: string): Promise<string> {
    return this.call('get_image', { conversationId, name });
  }

  /** Copie un texte dans le presse-papiers. */
  copyText(text: string): Promise<void> {
    return writeText(text).catch(rethrow);
  }

  /** Ouvre un lien d'une réponse dans le navigateur du système (http et https seulement). */
  openLink(url: string): Promise<void> {
    if (!isWebLink(url)) return Promise.reject(new Error(`Lien non pris en charge : ${url}`));
    return openUrl(url).catch(rethrow);
  }

  /** Thème de la fenêtre (barre de titre native) ; `null` : celui du système. */
  setWindowTheme(theme: Theme | null): Promise<void> {
    return this.call('plugin:window|set_theme', { label: 'main', value: theme });
  }

  private call<T>(cmd: string, args?: InvokeArgs): Promise<T> {
    return invoke<T>(cmd, args).catch(rethrow);
  }
}

function rethrow(e: unknown): never {
  throw e instanceof Error ? e : new Error(errorMessage(e));
}
