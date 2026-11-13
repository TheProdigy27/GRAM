import { TestBed } from '@angular/core/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import { countOf, mockBackend, type Handler } from '../../testing/backend';
import { FakeChat } from '../../testing/chat';
import {
  CONVERSATIONS,
  CONVERSATION_TODAY,
  CONVERSATION_WEEK,
  GRAM_MODEL,
  OTHER_MODEL,
} from '../../testing/fixtures';
import { ChatStore, NEXT_FRAME } from './chat.store';
import { ModelsStore } from './models.store';

/** Images en attente : le test les déclenche (`frames.run()`). */
class Frames {
  readonly queue: (() => void)[] = [];
  readonly schedule = (run: () => void) => void this.queue.push(run);
  run(): void {
    this.queue.splice(0).forEach((f) => f());
  }
}

async function setup(chat = new FakeChat(), handlers: Record<string, Handler> = {}) {
  const frames = new Frames();
  TestBed.configureTestingModule({
    providers: [{ provide: NEXT_FRAME, useValue: frames.schedule }],
  });
  const calls = mockBackend(handlers, chat);
  const models = TestBed.inject(ModelsStore);
  await models.refresh();
  const store = TestBed.inject(ChatStore);
  await store.load();
  return { store, models, chat, calls, frames };
}

/** Laisse passer les promesses de l'IPC simulé. */
const settle = () => new Promise((r) => setTimeout(r));

describe('ChatStore', () => {
  afterEach(() => {
    clearMocks();
    localStorage.clear();
  });

  it('nouvelle conversation : brouillon, puis identifiant à « started », réponse ajoutée', async () => {
    const { store, chat, frames } = await setup();
    expect(store.conversations()).toEqual([]);
    chat.manual = true;
    store.draft.set('  Bonjour  ');
    const sending = store.send(store.draft());
    expect(store.draft()).toBe('');
    expect(chat.requests).toEqual([
      { conversationId: null, model: GRAM_MODEL.name, content: 'Bonjour' },
    ]);
    // `started` est déjà passé : la conversation a son identifiant et figure dans la liste.
    await settle();
    expect(store.currentId()).toBe('nouvelle-1');
    expect(store.current()?.title).toBe('Bonjour');
    expect(store.conversations().map((c) => c.id)).toEqual(['nouvelle-1']);
    expect(store.streamingHere()).toBe(true);
    expect(store.messages().map((m) => m.role)).toEqual(['user']);

    // Morceaux réunis : appliqués une fois par image.
    chat.delta('', 'Je réfléchis');
    chat.delta('Bon');
    chat.delta('jour !');
    expect(frames.queue).toHaveLength(1);
    expect(store.streaming()?.content).toBe('');
    frames.run();
    expect(store.streaming()).toEqual({
      conversationId: 'nouvelle-1',
      model: GRAM_MODEL.name,
      content: 'Bonjour !',
      thinking: 'Je réfléchis',
    });

    chat.delta(' Encore');
    chat.finish();
    await sending;
    // « done » applique les derniers morceaux sans attendre l'image.
    expect(store.streaming()).toBeNull();
    const answer = store.messages()[1];
    expect(answer.content).toBe('Bonjour ! Encore');
    expect(answer.thinking).toBe('Je réfléchis');
    expect(answer.model).toBe(GRAM_MODEL.name);
    frames.run();
    expect(store.messages()).toHaveLength(2);
  });

  it('suite d’une conversation, avec le modèle choisi ; vide ou réponse en cours : rien ne part', async () => {
    const { store, chat, models } = await setup(new FakeChat(CONVERSATIONS));
    await store.open(CONVERSATION_TODAY.id);
    models.select(OTHER_MODEL.name);
    expect(await store.send(' \n ')).toBe(false);
    expect(await store.send('Et Q6_K ?')).toBe(true);
    expect(chat.requests.at(-1)).toEqual({
      conversationId: CONVERSATION_TODAY.id,
      model: OTHER_MODEL.name,
      content: 'Et Q6_K ?',
    });
    expect(store.messages()).toHaveLength(4);
    expect(store.messages()[3].model).toBe(OTHER_MODEL.name);
    // La conversation remonte en tête de liste.
    expect(store.conversations()[0].id).toBe(CONVERSATION_TODAY.id);

    chat.manual = true;
    void store.send('Une autre');
    expect(await store.send('Encore une')).toBe(false);
    expect(chat.requests).toHaveLength(2);
  });

  it('arrêter : réponse partielle gardée, marquée interrompue', async () => {
    const { store, chat, calls, frames } = await setup();
    chat.manual = true;
    const sending = store.send('Écris un conte');
    await settle();
    chat.delta('Il était une fois');
    frames.run();
    await store.stop();
    await sending;
    expect(countOf(calls, 'chat_stop')).toBe(1);
    expect(calls.find((c) => c.cmd === 'chat_stop')?.args).toEqual({
      conversationId: 'nouvelle-1',
    });
    const answer = store.messages()[1];
    expect(answer.interrupted).toBe(true);
    expect(answer.content).toBe('Il était une fois');
    expect(store.busy()).toBe(false);
  });

  it('échec en cours de route : dans le message, puis « Réessayer » le remplace', async () => {
    const { store, chat, calls } = await setup();
    chat.manual = true;
    const sending = store.send('Bonjour');
    await settle();
    chat.delta('Bon');
    chat.fail('Ollama ne répond pas sur http://127.0.0.1:11434 : le lancer, puis réessayer');
    await sending;
    expect(store.messages()[1].error).toContain('Ollama ne répond pas');
    expect(store.messages()[1].content).toBe('Bon');
    // L'état d'Ollama est relu.
    expect(countOf(calls, 'list_models')).toBe(2);

    chat.manual = false;
    await store.regenerate();
    expect(chat.requests.at(-1)).toEqual({
      conversationId: 'nouvelle-1',
      model: GRAM_MODEL.name,
      content: null,
    });
    expect(store.messages()).toHaveLength(2);
    expect(store.messages()[1].error).toBeNull();
    expect(chat.conversations.get('nouvelle-1')?.messages).toHaveLength(2);
  });

  it('envoi refusé avant la réponse : brouillon retiré, texte rendu, message affiché', async () => {
    const chat = new FakeChat(CONVERSATIONS);
    chat.refuse = 'Conversation non enregistrée (disque plein) : vérifier la place sur le disque';
    const { store } = await setup(chat);
    expect(await store.send('Bonjour')).toBe(true);
    await settle();
    expect(store.current()).toBeNull();
    expect(store.draft()).toBe('Bonjour');
    expect(store.sendError()).toContain('disque plein');
    expect(store.busy()).toBe(false);

    // Conversation existante : relue telle qu'enregistrée.
    await store.open(CONVERSATION_TODAY.id);
    store.draft.set('');
    await store.send('Suite');
    await settle();
    expect(store.messages()).toHaveLength(2);
    expect(store.draft()).toBe('Suite');
    // Ouvrir une autre conversation efface le message.
    await store.open(CONVERSATION_WEEK.id);
    expect(store.sendError()).toBeNull();
  });

  it('réponse finie pendant qu’une autre conversation est affichée : liste à jour, affichage intact', async () => {
    const { store, chat } = await setup(new FakeChat(CONVERSATIONS));
    chat.manual = true;
    const sending = store.send('Nouvelle question');
    await settle();
    store.newConversation();
    expect(store.streamingHere()).toBe(false);
    expect(store.busy()).toBe(true);
    chat.delta('Réponse');
    chat.finish();
    await sending;
    expect(store.current()).toBeNull();
    expect(store.conversations()[0].id).toBe('nouvelle-1');
    await store.open('nouvelle-1');
    expect(store.messages().map((m) => m.content)).toEqual(['Nouvelle question', 'Réponse']);
  });

  it('ouvrir : messages, modèle de la conversation choisi s’il est dans Ollama', async () => {
    const { store, models } = await setup(new FakeChat(CONVERSATIONS));
    expect(store.conversations().map((c) => c.id)).toEqual(CONVERSATIONS.map((c) => c.id));
    await store.open(CONVERSATION_WEEK.id);
    expect(store.currentId()).toBe(CONVERSATION_WEEK.id);
    expect(models.selected()?.name).toBe(OTHER_MODEL.name);
    expect(store.focusRequest()).toBe(1);

    // Introuvable : message, liste relue.
    await store.open('supprimee');
    expect(store.openError()).toContain('Conversation introuvable');
    expect(store.currentId()).toBe(CONVERSATION_WEEK.id);
  });

  it('renommer et supprimer', async () => {
    const { store, chat } = await setup(new FakeChat(CONVERSATIONS));
    await store.open(CONVERSATION_TODAY.id);
    await store.rename(CONVERSATION_TODAY.id, 'Quantifications');
    expect(store.current()?.title).toBe('Quantifications');
    expect(store.conversations()[0].title).toBe('Quantifications');

    await store.remove(CONVERSATION_TODAY.id);
    expect(store.current()).toBeNull();
    expect(store.conversations().map((c) => c.id)).not.toContain(CONVERSATION_TODAY.id);
    expect(chat.conversations.has(CONVERSATION_TODAY.id)).toBe(false);
  });

  it('liste illisible : message', async () => {
    const { store } = await setup(new FakeChat(), {
      list_conversations: () => {
        throw 'Dossier de données introuvable : les conversations ne peuvent pas être enregistrées';
      },
    });
    expect(store.listError()).toContain('Dossier de données introuvable');
  });
});
