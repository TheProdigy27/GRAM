import { TestBed } from '@angular/core/testing';
import { emit } from '@tauri-apps/api/event';
import { clearMocks } from '@tauri-apps/api/mocks';
import { byTestId, click, queryTestId, text, typeInto } from '@gram/testing/dom';
import type { ModelList } from '@gram/types/ModelList';
import { countOf, mockBackend, type Handler } from '../testing/backend';
import { FakeChat } from '../testing/chat';
import {
  CONVERSATION_TODAY,
  GRAM_MODEL,
  MODELS,
  OLLAMA_STOPPED,
  OTHER_MODEL,
} from '../testing/fixtures';
import { App } from './app';
import { OPEN_MODEL_EVENT } from './core/harness-api.service';
import { NEXT_FRAME } from './state/chat.store';
import { PREFS_KEY } from './state/prefs.store';

/**
 * Application avec un lien « Ouvrir dans GRAM Harness » en attente côté backend : `link(nom)`
 * le met de côté (comme le backend à la réception du lien), `take_opened_model` le donne une
 * fois.
 */
async function render(
  options: { chat?: FakeChat; pending?: string; handlers?: Record<string, Handler> } = {},
) {
  TestBed.configureTestingModule({
    providers: [{ provide: NEXT_FRAME, useValue: (run: () => void) => run() }],
  });
  let pending: string | null = options.pending ?? null;
  const calls = mockBackend(
    {
      take_opened_model: () => {
        const name = pending;
        pending = null;
        return name;
      },
      ...options.handlers,
    },
    options.chat,
  );
  const fixture = TestBed.createComponent(App);
  await settle(fixture);
  const el = fixture.nativeElement as HTMLElement;
  /** Lien reçu pendant que l'application tourne. */
  const link = async (name: string) => {
    pending = name;
    await emit(OPEN_MODEL_EVENT);
    await settle(fixture);
  };
  return { fixture, el, calls, link };
}

/** Laisse passer les promesses de l'IPC simulé, puis le rendu. */
async function settle(fixture: { whenStable(): Promise<unknown> }): Promise<void> {
  for (let i = 0; i < 3; i++) await new Promise((r) => setTimeout(r));
  await fixture.whenStable();
}

const savedModel = () => JSON.parse(localStorage.getItem(PREFS_KEY) ?? '{}').model;
const input = (el: HTMLElement) => byTestId<HTMLTextAreaElement>('composer-input', el);
const conversation = (el: HTMLElement, id: string) =>
  el.querySelector<HTMLButtonElement>(`[data-testid="conversation"][data-id="${id}"]`)!;

describe('Ouvrir dans GRAM Harness (lien profond)', () => {
  afterEach(() => {
    clearMocks();
    localStorage.clear();
    document.querySelectorAll('.cdk-overlay-container').forEach((c) => (c.innerHTML = ''));
  });

  it('au lancement : le modèle du lien est choisi, dans une nouvelle conversation', async () => {
    const { el, calls } = await render({ pending: OTHER_MODEL.name });
    expect(countOf(calls, 'take_opened_model')).toBe(1);
    expect(text(byTestId('model-select', el))).toBe('qwen3:8b');
    expect(savedModel()).toBe(OTHER_MODEL.name);
    expect(queryTestId('model-summary', el)).not.toBeNull();
    expect(document.activeElement).toBe(input(el));
  });

  it('sans lien : rien ne change', async () => {
    const { el, calls } = await render();
    expect(countOf(calls, 'take_opened_model')).toBe(1);
    expect(countOf(calls, 'list_models')).toBe(1);
    expect(text(byTestId('model-select', el))).toBe('Qwen3 8B Q4_K_M');
    expect(savedModel()).toBeUndefined();
  });

  it('pendant que l’application tourne : nouvelle conversation avec ce modèle, liste relue', async () => {
    const chat = new FakeChat([CONVERSATION_TODAY]);
    const { fixture, el, calls, link } = await render({ chat });
    await click(fixture, conversation(el, CONVERSATION_TODAY.id));
    await settle(fixture);
    expect(queryTestId('message-user', el)).not.toBeNull();

    const before = countOf(calls, 'list_models');
    // Ollama ne tient pas compte de la casse.
    await link('QWEN3:8B');
    expect(countOf(calls, 'list_models')).toBe(before + 1);
    expect(queryTestId('message-user', el)).toBeNull();
    expect(queryTestId('model-summary', el)).not.toBeNull();
    expect(text(byTestId('model-select', el))).toBe('qwen3:8b');
    expect(savedModel()).toBe(OTHER_MODEL.name);
    expect(conversation(el, CONVERSATION_TODAY.id).getAttribute('aria-current')).toBeNull();
  });

  it('nouvelle conversation vide affichée : gardée, avec le texte en cours de saisie', async () => {
    const { fixture, el, link } = await render();
    await typeInto(fixture, input(el), 'Bonjour');
    await link(OTHER_MODEL.name);
    expect(text(byTestId('model-select', el))).toBe('qwen3:8b');
    expect(input(el).value).toBe('Bonjour');
    expect(document.activeElement).toBe(input(el));
  });

  it('modèle absent d’Ollama : toast, rien ne change', async () => {
    const chat = new FakeChat([CONVERSATION_TODAY]);
    const { fixture, el, link } = await render({ chat });
    await click(fixture, conversation(el, CONVERSATION_TODAY.id));
    await settle(fixture);

    await link('gram/absent:q4_k_m');
    expect(document.body.textContent).toContain(
      'Ce modèle (gram/absent:q4_k_m) n’est pas dans Ollama : l’ajouter depuis GRAM.',
    );
    expect(queryTestId('message-user', el)).not.toBeNull();
    expect(text(byTestId('model-select', el))).toBe('Qwen3 8B Q4_K_M');
    expect(savedModel()).toBe(GRAM_MODEL.name);
  });

  it('Ollama arrêté : le modèle est retenu, puis choisi quand Ollama répond', async () => {
    let list: ModelList = OLLAMA_STOPPED;
    const { fixture, el, link } = await render({ handlers: { list_models: () => list } });
    await link(OTHER_MODEL.name);
    expect(queryTestId('ollama-stopped', el)).not.toBeNull();
    expect(document.body.textContent).not.toContain('n’est pas dans Ollama');
    expect(savedModel()).toBe(OTHER_MODEL.name);

    list = MODELS;
    await click(fixture, byTestId('state-refresh', el));
    await settle(fixture);
    expect(text(byTestId('model-select', el))).toBe('qwen3:8b');
  });
});
