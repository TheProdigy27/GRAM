import { TestBed } from '@angular/core/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import { countOf, mockBackend, type Handler } from '../../testing/backend';
import { FakeChat } from '../../testing/chat';
import {
  CONVERSATIONS,
  CONVERSATION_TODAY,
  CONVERSATION_VISION,
  CONVERSATION_WEEK,
  GRAM_MODEL,
  OTHER_MODEL,
  VISION_MODEL,
} from '../../testing/fixtures';
import { ChatStore, NEXT_FRAME } from './chat.store';
import { ImagesStore } from './images.store';
import { ModelsStore } from './models.store';
import { PrefsStore } from './prefs.store';

/** Fichier image de `size` octets. */
function image(name: string, type = 'image/png', size = 3): File {
  return new File([new Uint8Array(size).fill(65)], name, { type });
}

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
    // Réflexion (le modèle sait réfléchir), mémoire et instructions des paramètres.
    expect(chat.requests).toEqual([
      {
        conversationId: null,
        model: GRAM_MODEL.name,
        content: 'Bonjour',
        images: [],
        think: true,
        keepAlive: 'ollama',
        instructions: null,
      },
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
    expect(store.streaming()).toMatchObject({
      conversationId: 'nouvelle-1',
      model: GRAM_MODEL.name,
      content: 'Bonjour !',
      thinking: 'Je réfléchis',
    });
    // Réflexion puis réponse arrivées dans la même image : même instant.
    const { thinkingFrom, thinkingTo } = store.streaming()!;
    expect(thinkingFrom).not.toBeNull();
    expect(thinkingTo).toBe(thinkingFrom);

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
    expect(chat.requests.at(-1)).toMatchObject({
      conversationId: CONVERSATION_TODAY.id,
      model: OTHER_MODEL.name,
      content: 'Et Q6_K ?',
      // Conversation existante : ses instructions sont déjà enregistrées.
      instructions: null,
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
    expect(chat.requests.at(-1)).toMatchObject({
      conversationId: 'nouvelle-1',
      model: GRAM_MODEL.name,
      content: null,
      images: [],
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

  it('images jointes : formats, taille, nombre, vision ; envoyées en base64, enregistrées sous leur nom', async () => {
    const { store, chat, models, calls } = await setup();
    // Modèle sans vision : refusées.
    expect(await store.attach([image('a.png')])).toBe(
      'Qwen3 8B Q4_K_M n’accepte pas d’images : choisir un modèle avec la vision',
    );
    expect(store.attachments()).toEqual([]);

    models.select(VISION_MODEL.name);
    const problems = await store.attach([
      image('photo.png'),
      image('anim.gif', 'image/gif'),
      image('scan.jpg', 'image/jpeg', 11 * 2 ** 20),
    ]);
    expect(problems).toBe(
      'anim.gif : format non pris en charge (PNG, JPEG ou WebP) · scan.jpg : 11 Mo, 10 Mo au plus',
    );
    expect(store.attachments().map((a) => a.name)).toEqual(['photo.png']);
    expect(store.attachments()[0].url).toBe('data:image/png;base64,QUFB');
    // 4 au plus.
    expect(await store.attach([1, 2, 3, 4].map((n) => image(`${n}.webp`, 'image/webp')))).toBe(
      '4 images au plus par message',
    );
    expect(store.attachments()).toHaveLength(4);
    store.detach(store.attachments()[1].id);
    expect(store.attachments().map((a) => a.name)).toEqual(['photo.png', '2.webp', '3.webp']);

    // Envoi sans texte : images seules, en base64 ; pas de réflexion (Gemma ne réfléchit pas).
    expect(await store.send('  ')).toBe(true);
    expect(chat.requests[0]).toMatchObject({ content: '', think: null });
    expect(chat.requests[0].images).toEqual(['QUFB', 'QUFB', 'QUFB']);
    expect(store.attachments()).toEqual([]);
    // Le message affiché est celui enregistré, avec les noms des images, déjà en cache.
    const user = store.messages()[0];
    expect(user.images).toHaveLength(3);
    expect(user.images.every((n) => n.endsWith('.png'))).toBe(true);
    const images = TestBed.inject(ImagesStore);
    expect(images.url('nouvelle-1', user.images[0])).toBe('data:image/png;base64,QUFB');
    expect(countOf(calls, 'get_image')).toBe(0);
    expect(store.hasImages()).toBe(true);

    // Plus de vision : l'envoi d'images ne part pas.
    await store.attach([image('b.png')]);
    models.select(GRAM_MODEL.name);
    expect(await store.send('Et ça ?')).toBe(false);
    expect(chat.requests).toHaveLength(1);
  });

  it('envoi refusé : texte et images rendus', async () => {
    const chat = new FakeChat();
    chat.refuse = 'Image 1 illisible : la joindre de nouveau';
    const { store, models } = await setup(chat);
    models.select(VISION_MODEL.name);
    await store.attach([image('photo.png')]);
    await store.send('Décris');
    await settle();
    expect(store.draft()).toBe('Décris');
    expect(store.attachments().map((a) => a.name)).toEqual(['photo.png']);
    expect(store.sendError()).toContain('illisible');
  });

  it('images enregistrées : lues une fois, introuvables signalées, oubliées avec la conversation', async () => {
    const chat = new FakeChat([CONVERSATION_VISION]);
    chat.images.set(`${CONVERSATION_VISION.id}/a.png`, 'data:image/png;base64,QUFB');
    const { store, calls } = await setup(chat);
    const images = TestBed.inject(ImagesStore);
    expect(images.url(CONVERSATION_VISION.id, 'a.png')).toBeUndefined();
    expect(images.url(CONVERSATION_VISION.id, 'a.png')).toBeUndefined();
    await settle();
    expect(images.url(CONVERSATION_VISION.id, 'a.png')).toBe('data:image/png;base64,QUFB');
    expect(images.url(CONVERSATION_VISION.id, 'absente.png')).toBeUndefined();
    await settle();
    expect(images.url(CONVERSATION_VISION.id, 'absente.png')).toBeNull();
    expect(countOf(calls, 'get_image')).toBe(2);
    await store.remove(CONVERSATION_VISION.id);
    expect(images.url(CONVERSATION_VISION.id, 'a.png')).toBeUndefined();
  });

  it('réflexion : retenue par la conversation, dernier choix pour les nouvelles', async () => {
    const { store, chat, calls } = await setup(new FakeChat(CONVERSATIONS));
    const prefs = TestBed.inject(PrefsStore);
    expect(store.think()).toBe(true);
    // Nouvelle conversation : partira avec le premier message.
    await store.setThink(false);
    expect(prefs.think()).toBe(false);
    expect(countOf(calls, 'set_think')).toBe(0);
    await store.send('Vite');
    expect(chat.requests[0].think).toBe(false);
    expect(chat.conversations.get('nouvelle-1')?.think).toBe(false);

    // Conversation sans choix : le dernier ; puis enregistré tout de suite.
    await store.open(CONVERSATION_TODAY.id);
    expect(store.think()).toBe(false);
    await store.setThink(true);
    expect(calls.find((c) => c.cmd === 'set_think')?.args).toEqual({
      id: CONVERSATION_TODAY.id,
      think: true,
    });
    expect(store.current()?.think).toBe(true);
    // La conversation précédente garde le sien.
    await store.open('nouvelle-1');
    expect(store.think()).toBe(false);
    // Nouvelle conversation : le dernier choix.
    store.newConversation();
    expect(store.think()).toBe(true);
  });

  it('instructions et mémoire : paramètres pour une nouvelle conversation, puis enregistrées', async () => {
    const { store, chat, calls } = await setup(new FakeChat(CONVERSATIONS));
    const prefs = TestBed.inject(PrefsStore);
    prefs.setInstructions('Réponds en français.');
    prefs.setKeepAlive('forever');
    expect(store.instructions()).toBe('Réponds en français.');
    await store.send('Bonjour');
    expect(chat.requests[0]).toMatchObject({
      instructions: 'Réponds en français.',
      keepAlive: 'forever',
    });

    // Changées avant le premier message.
    store.newConversation();
    await store.setInstructions('  Tutoie-moi.  ');
    expect(store.instructions()).toBe('Tutoie-moi.');
    expect(countOf(calls, 'set_instructions')).toBe(0);
    await store.send('Salut');
    expect(chat.requests[1].instructions).toBe('Tutoie-moi.');
    // Nouvelle conversation suivante : de nouveau celles des paramètres.
    store.newConversation();
    expect(store.instructions()).toBe('Réponds en français.');

    // Conversation existante : enregistrées tout de suite ; vides, aucune.
    await store.open(CONVERSATION_TODAY.id);
    expect(store.instructions()).toBe('');
    await store.setInstructions('Sois bref.');
    expect(calls.filter((c) => c.cmd === 'set_instructions').map((c) => c.args)).toEqual([
      { id: CONVERSATION_TODAY.id, instructions: 'Sois bref.' },
    ]);
    expect(store.instructions()).toBe('Sois bref.');
    await store.setInstructions(' ');
    expect(store.current()?.instructions).toBeNull();
    expect(chat.conversations.get(CONVERSATION_TODAY.id)?.instructions).toBeNull();
  });

  it('jauge : contexte occupé par la dernière réponse', async () => {
    const { store } = await setup(new FakeChat([CONVERSATION_TODAY, CONVERSATION_VISION]));
    expect(store.context()).toBeNull();
    await store.open(CONVERSATION_TODAY.id);
    expect(store.context()).toMatchObject({ used: 230, max: 16384, level: 'ok' });
    await store.open(CONVERSATION_VISION.id);
    expect(store.context()).toMatchObject({ level: 'warn', label: '26k / 32k' });
  });
});
