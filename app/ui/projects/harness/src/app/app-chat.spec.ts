import { TestBed } from '@angular/core/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import {
  allByTestId,
  byTestId,
  click,
  openMenu,
  press,
  queryTestId,
  text,
  typeInto,
} from '@gram/testing/dom';
import { countOf, mockBackend, type Handler } from '../testing/backend';
import { FakeChat } from '../testing/chat';
import {
  CONVERSATIONS,
  CONVERSATION_OLD,
  CONVERSATION_TODAY,
  CONVERSATION_WEEK,
  OLLAMA_STOPPED,
  OTHER_MODEL,
} from '../testing/fixtures';
import { App } from './app';
import { NEXT_FRAME } from './state/chat.store';

async function render(chat = new FakeChat(), handlers: Record<string, Handler> = {}) {
  TestBed.configureTestingModule({
    providers: [{ provide: NEXT_FRAME, useValue: (run: () => void) => run() }],
  });
  const calls = mockBackend(handlers, chat);
  const fixture = TestBed.createComponent(App);
  await settle(fixture);
  const el = fixture.nativeElement as HTMLElement;
  return { fixture, el, calls, chat };
}

type Fixture = Awaited<ReturnType<typeof render>>['fixture'];

/** Laisse passer les promesses de l'IPC simulé, puis le rendu. */
async function settle(fixture: { whenStable(): Promise<unknown> }): Promise<void> {
  await new Promise((r) => setTimeout(r));
  await fixture.whenStable();
}

const input = (el: HTMLElement) => byTestId<HTMLTextAreaElement>('composer-input', el);

async function sendText(fixture: Fixture, el: HTMLElement, value: string): Promise<void> {
  await typeInto(fixture, input(el), value);
  await press(fixture, input(el), 'Enter');
  await settle(fixture);
}

/** Conversation de la barre latérale. */
const conversation = (el: HTMLElement, id: string) =>
  el.querySelector<HTMLButtonElement>(`[data-testid="conversation"][data-id="${id}"]`)!;

describe('Discussion (GRAM Harness)', () => {
  afterEach(() => {
    clearMocks();
    localStorage.clear();
    document.querySelectorAll('.cdk-overlay-container').forEach((c) => (c.innerHTML = ''));
  });

  it('Entrée envoie, Maj + Entrée et une composition en cours non ; réponse en Markdown', async () => {
    const { fixture, el, chat } = await render();
    expect(queryTestId('model-summary', el)).not.toBeNull();
    expect(input(el).placeholder).toBe('Écrire à Qwen3 8B Q4_K_M…');
    expect(text(byTestId('composer-hint', el))).toBe(
      'Entrée pour envoyer · Maj + Entrée pour aller à la ligne',
    );
    expect(byTestId<HTMLButtonElement>('composer-send', el).disabled).toBe(true);

    await typeInto(fixture, input(el), 'Bonjour');
    expect(byTestId<HTMLButtonElement>('composer-send', el).disabled).toBe(false);
    const shift = await press(fixture, input(el), 'Enter', { shiftKey: true });
    expect(shift.defaultPrevented).toBe(false);
    await press(fixture, input(el), 'Enter', { isComposing: true });
    expect(chat.requests).toEqual([]);

    const enter = await press(fixture, input(el), 'Enter');
    await settle(fixture);
    expect(enter.defaultPrevented).toBe(true);
    expect(chat.requests).toHaveLength(1);
    expect(input(el).value).toBe('');
    expect(queryTestId('model-summary', el)).toBeNull();

    expect(text(byTestId('message-user', el))).toBe('Bonjour');
    const answer = byTestId('message-assistant', el);
    expect(text(byTestId('message-model', answer))).toBe('Qwen3 8B Q4_K_M');
    expect(text(answer.querySelector('.md strong'))).toBe('Comment');
    expect(queryTestId('message-copy', answer)).not.toBeNull();
    expect(queryTestId('message-regenerate', answer)).not.toBeNull();
    // La nouvelle conversation est dans la barre latérale, active.
    expect(conversation(el, 'nouvelle-1').getAttribute('aria-current')).toBe('page');
    expect(text(byTestId('group-today', el))).toBe('Aujourd’hui');
  });

  it('pendant la réponse : « Arrêter », attente, réflexion ; Échap arrête depuis la saisie', async () => {
    const chat = new FakeChat();
    chat.manual = true;
    const { fixture, el, calls } = await render(chat);
    await sendText(fixture, el, 'Écris un conte');
    expect(queryTestId('composer-send', el)).toBeNull();
    // Annoncé par l'infobulle, comme dans GRAM.
    expect(byTestId('composer-stop', el).getAttribute('aria-keyshortcuts')).toBe('Échap');
    expect(text(byTestId('composer-hint', el))).toBe('Réponse en cours · Échap pour l’arrêter');
    expect(text(byTestId('message-waiting', el))).toBe('Le modèle prépare sa réponse…');
    expect(conversation(el, 'nouvelle-1').querySelector('g-spinner')).not.toBeNull();

    chat.delta('', 'Un conte, donc.');
    await settle(fixture);
    expect(text(byTestId('message-waiting', el))).toBe('Le modèle réfléchit…');
    // La réflexion s'écrit en direct, ouverte, minutée.
    const toggle = byTestId('thinking-toggle', el);
    expect(text(toggle)).toBe('Réflexion… < 1 s');
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(text(byTestId('thinking-text', el))).toBe('Un conte, donc.');
    expect(byTestId('message-assistant', el).getAttribute('aria-busy')).toBe('true');

    chat.delta('Il était **une fois**');
    await settle(fixture);
    // Repliée dès que la réponse commence ; on peut la rouvrir.
    expect(byTestId('thinking-toggle', el).getAttribute('aria-expanded')).toBe('false');
    expect(queryTestId('thinking-text', el)).toBeNull();
    await click(fixture, byTestId('thinking-toggle', el));
    expect(text(byTestId('thinking-text', el))).toBe('Un conte, donc.');
    expect(queryTestId('message-waiting', el)).toBeNull();
    expect(text(byTestId('message-assistant', el).querySelector('.md strong'))).toBe('une fois');
    // Pas d'actions sur une réponse en cours.
    expect(queryTestId('message-copy', el)).toBeNull();

    // Entrée ne renvoie pas pendant la réponse.
    await typeInto(fixture, input(el), 'Suite');
    await press(fixture, input(el), 'Enter');
    expect(chat.requests).toHaveLength(1);

    await press(fixture, input(el), 'Escape');
    await settle(fixture);
    expect(countOf(calls, 'chat_stop')).toBe(1);
    expect(text(byTestId('message-interrupted', el))).toBe('Interrompue');
    // Durée enregistrée par le backend (1,2 s dans le faux backend).
    expect(text(byTestId('thinking-toggle', el))).toBe('Réflexion · 1 s');
    expect(queryTestId('composer-send', el)).not.toBeNull();
    // Échap sans réponse en cours : sans effet.
    await press(fixture, input(el), 'Escape');
    expect(countOf(calls, 'chat_stop')).toBe(1);
  });

  it('le bouton « Arrêter » arrête aussi', async () => {
    const chat = new FakeChat();
    chat.manual = true;
    const { fixture, el, calls } = await render(chat);
    await sendText(fixture, el, 'Bonjour');
    chat.delta('Bon');
    await click(fixture, byTestId('composer-stop', el));
    await settle(fixture);
    expect(countOf(calls, 'chat_stop')).toBe(1);
    expect(queryTestId('message-interrupted', el)).not.toBeNull();
  });

  it('échec : message dans le fil et « Réessayer » ; « Régénérer » sur la dernière réponse', async () => {
    const { fixture, el, chat } = await render(new FakeChat(CONVERSATIONS));
    await click(fixture, conversation(el, CONVERSATION_OLD.id));
    await settle(fixture);
    const error = byTestId('message-error', el);
    expect(error.getAttribute('role')).toBe('alert');
    expect(text(error)).toContain('Ollama ne répond pas');
    expect(queryTestId('message-regenerate', el)).toBeNull();
    await click(fixture, byTestId('message-retry', el));
    await settle(fixture);
    expect(chat.requests.at(-1)).toMatchObject({
      conversationId: CONVERSATION_OLD.id,
      model: 'gram/qwen3-8b:q4_k_m',
      content: null,
    });
    expect(queryTestId('message-error', el)).toBeNull();
    expect(allByTestId('message-assistant', el)).toHaveLength(1);

    await click(fixture, byTestId('message-regenerate', el));
    await settle(fixture);
    expect(chat.requests).toHaveLength(2);
    expect(allByTestId('message-assistant', el)).toHaveLength(1);
  });

  it('copier une réponse, un bloc de code ; un lien s’ouvre dans le navigateur, pas dans la fenêtre', async () => {
    const { fixture, el, calls } = await render(new FakeChat(CONVERSATIONS));
    await click(fixture, conversation(el, CONVERSATION_TODAY.id));
    await settle(fixture);
    const writes = () =>
      calls.filter((c) => c.cmd === 'plugin:clipboard-manager|write_text').map((c) => c.args);

    await click(fixture, byTestId('message-copy', el));
    await settle(fixture);
    expect(writes()[0]).toMatchObject({ text: CONVERSATION_TODAY.messages[1].content });
    expect(document.body.textContent).toContain('Réponse copiée');

    const copyCode = byTestId<HTMLButtonElement>('copy-code', el);
    await click(fixture, copyCode);
    await settle(fixture);
    expect((writes()[1] as { text: string }).text).toBe(
      'def taille(params: float, bits: float) -> float:\n    """Taille approximative en Go."""\n    return params * bits / 8',
    );
    expect(copyCode.textContent).toBe('Copié');

    const link = el.querySelector<HTMLAnchorElement>('.md a')!;
    expect(link.title).toBe('https://github.com/ggml-org/llama.cpp');
    const event = new MouseEvent('click', { bubbles: true, cancelable: true });
    link.dispatchEvent(event);
    await settle(fixture);
    expect(event.defaultPrevented).toBe(true);
    const opened = calls.filter((c) => c.cmd === 'plugin:opener|open_url');
    expect(opened.map((c) => c.args['url'])).toEqual(['https://github.com/ggml-org/llama.cpp']);
  });

  it('barre latérale : groupes par date, ouvrir une conversation choisit son modèle', async () => {
    const { fixture, el } = await render(new FakeChat(CONVERSATIONS));
    const groups = [...el.querySelectorAll('.group')].map((g) => ({
      title: text(g.querySelector('h3')),
      items: [...g.querySelectorAll('[data-testid="conversation"]')].map(text),
    }));
    expect(groups).toEqual([
      { title: 'Aujourd’hui', items: ['Différence entre Q4_K_M et Q8_0'] },
      { title: '7 derniers jours', items: ['Écrire un conte'] },
      { title: 'Plus ancien', items: ['Bonjour'] },
    ]);
    expect(queryTestId('conversations-empty', el)).toBeNull();

    await click(fixture, conversation(el, CONVERSATION_WEEK.id));
    await settle(fixture);
    expect(conversation(el, CONVERSATION_WEEK.id).getAttribute('aria-current')).toBe('page');
    expect(conversation(el, CONVERSATION_TODAY.id).getAttribute('aria-current')).toBeNull();
    expect(text(byTestId('message-user', el))).toBe('Écris un long conte.');
    expect(text(byTestId('model-select', el))).toBe('qwen3:8b');
    expect(text(byTestId('message-model', el))).toBe(OTHER_MODEL.name);
    expect(document.activeElement).toBe(input(el));
  });

  it('renommer et supprimer par le menu', async () => {
    const { fixture, el, chat } = await render(new FakeChat(CONVERSATIONS));
    await click(fixture, conversation(el, CONVERSATION_TODAY.id));
    await settle(fixture);
    const menuButton = () =>
      el.querySelector<HTMLElement>(
        `li[data-id="${CONVERSATION_TODAY.id}"] [data-testid="conversation-menu"]`,
      )!;
    expect(menuButton().getAttribute('aria-label')).toBe(
      'Actions : Différence entre Q4_K_M et Q8_0',
    );

    let items = await openMenu(fixture, menuButton());
    expect(items.map(text)).toEqual(['Renommer', 'Supprimer']);
    await click(fixture, items[0]);
    await settle(fixture);
    const field = document.querySelector<HTMLInputElement>('[data-testid="rename-input"]')!;
    expect(field.value).toBe('Différence entre Q4_K_M et Q8_0');
    await typeInto(fixture, field, '  Quantifications ');
    await click(fixture, document.querySelector<HTMLElement>('[data-testid="rename-confirm"]')!);
    await settle(fixture);
    expect(text(conversation(el, CONVERSATION_TODAY.id))).toBe('Quantifications');
    expect(chat.conversations.get(CONVERSATION_TODAY.id)?.title).toBe('Quantifications');

    items = await openMenu(fixture, menuButton());
    await click(fixture, items[1]);
    await settle(fixture);
    const dialog = document.querySelector<HTMLElement>('app-delete-dialog')!;
    expect(text(dialog)).toContain('« Quantifications » et tous ses messages seront supprimés');
    await click(fixture, document.querySelector<HTMLElement>('[data-testid="delete-confirm"]')!);
    await settle(fixture);
    expect(conversation(el, CONVERSATION_TODAY.id)).toBeNull();
    expect(chat.conversations.has(CONVERSATION_TODAY.id)).toBe(false);
    // La conversation affichée a disparu : nouvelle conversation.
    expect(queryTestId('model-summary', el)).not.toBeNull();
    expect(document.body.textContent).toContain('Conversation supprimée');
  });

  it('Ctrl N : nouvelle conversation, zone de saisie prête ; ? : aide des raccourcis', async () => {
    const { fixture, el } = await render(new FakeChat(CONVERSATIONS));
    await click(fixture, conversation(el, CONVERSATION_TODAY.id));
    await settle(fixture);
    expect(queryTestId('model-summary', el)).toBeNull();
    const event = await press(fixture, input(el), 'n', { ctrlKey: true });
    await settle(fixture);
    expect(event.defaultPrevented).toBe(true);
    expect(queryTestId('model-summary', el)).not.toBeNull();
    expect(el.querySelector('[aria-current="page"]')).toBeNull();
    expect(document.activeElement).toBe(input(el));

    // Le bouton de la barre latérale fait de même.
    await click(fixture, conversation(el, CONVERSATION_WEEK.id));
    await settle(fixture);
    await click(fixture, byTestId('new-conversation', el));
    expect(queryTestId('model-summary', el)).not.toBeNull();

    (document.activeElement as HTMLElement | null)?.blur();
    await press(fixture, document.body, '?');
    await settle(fixture);
    const rows = [...document.querySelectorAll('[data-testid="shortcut-row"]')];
    expect(rows.map((r) => text(r.querySelector('dd')))).toEqual([
      'Nouvelle conversation',
      'Paramètres',
      'Arrêter la réponse ; fermer le dialogue ou le menu',
      'Afficher cette aide',
      'Actualiser la liste des modèles',
      'Envoyer le message',
      'Aller à la ligne',
    ]);
    expect(text(rows[0].querySelector('dt'))).toBe('Ctrl N');
  });

  it('Ollama arrêté : la conversation se lit, l’envoi est désactivé avec la raison', async () => {
    const { fixture, el } = await render(new FakeChat(CONVERSATIONS), {
      list_models: () => OLLAMA_STOPPED,
    });
    // Sans conversation : l'état d'Ollama, sans zone de saisie.
    expect(queryTestId('ollama-stopped', el)).not.toBeNull();
    expect(queryTestId('composer-input', el)).toBeNull();
    await click(fixture, conversation(el, CONVERSATION_TODAY.id));
    await settle(fixture);
    await typeInto(fixture, input(el), 'Suite');
    expect(byTestId<HTMLButtonElement>('composer-send', el).disabled).toBe(true);
    expect(text(byTestId('composer-hint', el))).toBe(
      'Aucun modèle disponible : lancer Ollama ou ajouter un modèle depuis GRAM, puis Actualiser',
    );
    expect(byTestId<HTMLButtonElement>('message-regenerate', el).disabled).toBe(true);
    // Le modèle qui a répondu garde son nom technique (plus dans la liste).
    expect(text(byTestId('message-model', el))).toBe('gram/qwen3-8b:q4_k_m');
  });

  it('réponse en cours ailleurs : envoi désactivé ici, avec la raison', async () => {
    const chat = new FakeChat(CONVERSATIONS);
    chat.manual = true;
    const { fixture, el } = await render(chat);
    await sendText(fixture, el, 'Longue question');
    await click(fixture, conversation(el, CONVERSATION_WEEK.id));
    await settle(fixture);
    await typeInto(fixture, input(el), 'Autre chose');
    expect(byTestId<HTMLButtonElement>('composer-send', el).disabled).toBe(true);
    expect(text(byTestId('composer-hint', el))).toBe(
      'Une réponse est en cours dans une autre conversation',
    );
    expect(queryTestId('composer-stop', el)).toBeNull();
    chat.finish();
    await settle(fixture);
    expect(byTestId<HTMLButtonElement>('composer-send', el).disabled).toBe(false);
  });

  it('envoi refusé : message au-dessus de la saisie, texte rendu', async () => {
    const chat = new FakeChat();
    chat.refuse = 'Conversation non enregistrée (accès refusé) : vérifier la place sur le disque';
    const { fixture, el } = await render(chat);
    await sendText(fixture, el, 'Bonjour');
    expect(text(byTestId('send-error', el))).toContain('accès refusé');
    expect(input(el).value).toBe('Bonjour');
    expect(queryTestId('model-summary', el)).not.toBeNull();
  });
});
