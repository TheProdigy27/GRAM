import { TestBed } from '@angular/core/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import {
  allByTestId,
  byTestId,
  chooseOption,
  click,
  press,
  queryTestId,
  text,
  toggleSwitch,
  typeInto,
} from '@gram/testing/dom';
import { countOf, mockBackend } from '../testing/backend';
import { FakeChat } from '../testing/chat';
import {
  CONVERSATIONS,
  CONVERSATION_TODAY,
  CONVERSATION_VISION,
  CONVERSATION_WEEK,
  GRAM_MODEL,
  PNG_URL,
  VISION_IMAGE,
  VISION_MODEL,
} from '../testing/fixtures';
import { App } from './app';
import { NEXT_FRAME } from './state/chat.store';
import { ModelsStore } from './state/models.store';
import { PrefsStore } from './state/prefs.store';

async function render(chat = new FakeChat()) {
  TestBed.configureTestingModule({
    providers: [{ provide: NEXT_FRAME, useValue: (run: () => void) => run() }],
  });
  const calls = mockBackend({}, chat);
  const fixture = TestBed.createComponent(App);
  await settle(fixture);
  const el = fixture.nativeElement as HTMLElement;
  return { fixture, el, calls, chat };
}

type Fixture = Awaited<ReturnType<typeof render>>['fixture'];

/** Laisse passer les promesses de l'IPC simulé (et la lecture des fichiers), puis le rendu. */
async function settle(fixture: { whenStable(): Promise<unknown> }): Promise<void> {
  for (let i = 0; i < 3; i++) await new Promise((r) => setTimeout(r));
  await fixture.whenStable();
}

const input = (el: HTMLElement) => byTestId<HTMLTextAreaElement>('composer-input', el);

function image(name: string, type = 'image/png'): File {
  return new File([new Uint8Array([65, 65, 65])], name, { type });
}

async function selectModel(fixture: Fixture, name: string): Promise<void> {
  TestBed.inject(ModelsStore).select(name);
  await settle(fixture);
}

/** Choisit des fichiers avec le trombone (sélecteur de fichiers). */
async function pickFiles(fixture: Fixture, el: HTMLElement, files: File[]): Promise<void> {
  const picker = byTestId<HTMLInputElement>('composer-file', el);
  Object.defineProperty(picker, 'files', { value: files, configurable: true });
  picker.dispatchEvent(new Event('change'));
  await settle(fixture);
}

/** Événement de glisser-déposer ou de collage, avec ses fichiers. */
function transfer(type: string, files: File[], prop = 'dataTransfer', text = ''): Event {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperty(event, prop, {
    value: { files, types: ['Files'], dropEffect: 'none', getData: () => text },
  });
  return event;
}

const conversation = (el: HTMLElement, id: string) =>
  el.querySelector<HTMLButtonElement>(`[data-testid="conversation"][data-id="${id}"]`)!;

describe('GRAM Harness : images, réflexion, contexte, vitesse, instructions, paramètres', () => {
  afterEach(() => {
    clearMocks();
    localStorage.clear();
    document.querySelectorAll('.cdk-overlay-container').forEach((c) => (c.innerHTML = ''));
  });

  it('images : trombone pour un modèle avec la vision, vignettes, retrait, envoi puis affichage', async () => {
    const { fixture, el, chat, calls } = await render();
    // Qwen3 8B n'a pas la vision : pas de trombone.
    expect(queryTestId('composer-attach', el)).toBeNull();
    await selectModel(fixture, VISION_MODEL.name);
    expect(byTestId('composer-attach', el).getAttribute('aria-label')).toBe('Joindre des images');
    // Gemma ne réfléchit pas : pas de bascule.
    expect(queryTestId('composer-think', el)).toBeNull();

    await pickFiles(fixture, el, [image('photo.png'), image('carte.jpg', 'image/jpeg')]);
    const thumbs = allByTestId('attachment', el);
    expect(thumbs.map((t) => t.querySelector('img')?.alt)).toEqual(['photo.png', 'carte.jpg']);
    expect(byTestId('attachment-remove', thumbs[1]).getAttribute('aria-label')).toBe(
      'Retirer carte.jpg',
    );
    await click(fixture, byTestId('attachment-remove', thumbs[1]));
    expect(allByTestId('attachment', el)).toHaveLength(1);
    // Une image seule suffit pour envoyer.
    expect(byTestId<HTMLButtonElement>('composer-send', el).disabled).toBe(false);

    await typeInto(fixture, input(el), 'Décris cette image.');
    await press(fixture, input(el), 'Enter');
    await settle(fixture);
    expect(chat.requests[0].images).toEqual(['QUFB']);
    expect(queryTestId('attachments', el)).toBeNull();
    const shown = byTestId('message-image', el).querySelector('img')!;
    expect(shown.getAttribute('src')).toBe('data:image/png;base64,QUFB');
    expect(shown.alt).toBe('Image jointe 1');
    expect(text(byTestId('message-user', el))).toBe('Décris cette image.');
    expect(countOf(calls, 'get_image')).toBe(0);
  });

  it('images enregistrées : lues depuis la conversation ; modèle sans vision : non envoyées, image collée refusée', async () => {
    const chat = new FakeChat([CONVERSATION_VISION]);
    chat.images.set(`${CONVERSATION_VISION.id}/${VISION_IMAGE}`, PNG_URL);
    const { fixture, el, calls } = await render(chat);
    await click(fixture, conversation(el, CONVERSATION_VISION.id));
    await settle(fixture);
    expect(byTestId('message-image', el).querySelector('img')?.getAttribute('src')).toBe(PNG_URL);
    expect(calls.find((c) => c.cmd === 'get_image')?.args).toEqual({
      conversationId: CONVERSATION_VISION.id,
      name: VISION_IMAGE,
    });

    // Un modèle sans vision : les images de l'historique ne partent pas, on le dit.
    await selectModel(fixture, GRAM_MODEL.name);
    expect(text(byTestId('composer-hint', el))).toBe(
      'Les images de la conversation ne sont pas envoyées à Qwen3 8B Q4_K_M (pas de vision)',
    );
    const paste = transfer('paste', [image('capture.png')], 'clipboardData');
    input(el).dispatchEvent(paste);
    await settle(fixture);
    expect(paste.defaultPrevented).toBe(true);
    expect(document.body.textContent).toContain(
      'Qwen3 8B Q4_K_M n’accepte pas d’images : choisir un modèle avec la vision',
    );
    expect(queryTestId('attachments', el)).toBeNull();
  });

  it('coller : une image se joint, du texte brut l’emporte ; image introuvable signalée', async () => {
    const chat = new FakeChat([CONVERSATION_VISION]);
    const { fixture, el } = await render(chat);
    await click(fixture, conversation(el, CONVERSATION_VISION.id));
    await settle(fixture);
    expect(text(byTestId('message-image-missing', el))).toBe('Image introuvable');

    // Cellules d'un tableur : texte et image de rendu.
    const cells = transfer('paste', [image('cellules.png')], 'clipboardData', 'A1\tB1');
    input(el).dispatchEvent(cells);
    await settle(fixture);
    expect(cells.defaultPrevented).toBe(false);
    expect(queryTestId('attachments', el)).toBeNull();

    const shot = transfer('paste', [image('capture.png')], 'clipboardData');
    input(el).dispatchEvent(shot);
    await settle(fixture);
    expect(shot.defaultPrevented).toBe(true);
    expect(allByTestId('attachment', el)).toHaveLength(1);
  });

  it('glisser-déposer : zone de dépôt, puis images jointes ; refus en toast', async () => {
    const { fixture, el } = await render();
    await selectModel(fixture, VISION_MODEL.name);
    const view = el.querySelector('app-chat-view')!;
    view.dispatchEvent(transfer('dragenter', []));
    const over = transfer('dragover', []);
    view.dispatchEvent(over);
    await settle(fixture);
    expect(over.defaultPrevented).toBe(true);
    expect(text(byTestId('drop-zone', el))).toBe('Déposer les images pour les joindre au message');

    view.dispatchEvent(
      transfer('drop', [image('photo.webp', 'image/webp'), image('a.gif', 'image/gif')]),
    );
    await settle(fixture);
    expect(queryTestId('drop-zone', el)).toBeNull();
    expect(allByTestId('attachment', el)).toHaveLength(1);
    expect(document.body.textContent).toContain('a.gif : format non pris en charge');

    // Modèle sans vision : la zone le dit.
    await selectModel(fixture, GRAM_MODEL.name);
    view.dispatchEvent(transfer('dragenter', []));
    await settle(fixture);
    expect(text(byTestId('drop-zone', el))).toBe('Qwen3 8B Q4_K_M n’accepte pas d’images');
    view.dispatchEvent(transfer('dragleave', []));
    await settle(fixture);
    expect(queryTestId('drop-zone', el)).toBeNull();
    // L'envoi est bloqué tant que les images restent.
    expect(text(byTestId('composer-hint', el))).toBe(
      'Qwen3 8B Q4_K_M n’accepte pas d’images : retirer les images, ou choisir un modèle avec la vision',
    );
  });

  it('réflexion : bascule pour un modèle qui réfléchit, envoyée et retenue par la conversation', async () => {
    const { fixture, el, chat, calls } = await render(new FakeChat(CONVERSATIONS));
    const think = byTestId('composer-think', el);
    expect(text(think)).toBe('Réflexion');
    expect(await toggleSwitch(fixture, think)).toBe(false);
    await typeInto(fixture, input(el), 'Vite');
    await press(fixture, input(el), 'Enter');
    await settle(fixture);
    expect(chat.requests[0].think).toBe(false);

    await click(fixture, conversation(el, CONVERSATION_TODAY.id));
    await settle(fixture);
    expect(await toggleSwitch(fixture, byTestId('composer-think', el))).toBe(true);
    await settle(fixture);
    expect(calls.find((c) => c.cmd === 'set_think')?.args).toEqual({
      id: CONVERSATION_TODAY.id,
      think: true,
    });
    await click(fixture, conversation(el, 'nouvelle-1'));
    await settle(fixture);
    expect(
      byTestId('composer-think', el).querySelector('[role="switch"]')?.getAttribute('aria-checked'),
    ).toBe('false');
  });

  it('jauge de contexte dans l’en-tête ; encadré à 80 % avec « Nouvelle conversation »', async () => {
    const { fixture, el } = await render(new FakeChat([CONVERSATION_TODAY, CONVERSATION_VISION]));
    expect(queryTestId('context-gauge', el)).toBeNull();
    await click(fixture, conversation(el, CONVERSATION_TODAY.id));
    await settle(fixture);
    const gauge = byTestId('context-gauge', el);
    expect(text(byTestId('context-value', gauge))).toBe('230 / 16k');
    expect(gauge.getAttribute('data-level')).toBe('ok');
    expect(gauge.getAttribute('aria-valuetext')).toBe(
      'Contexte occupé à la fin de la dernière réponse : 230 tokens sur 16 384',
    );
    expect(queryTestId('context-warning', el)).toBeNull();

    await click(fixture, conversation(el, CONVERSATION_VISION.id));
    await settle(fixture);
    expect(text(byTestId('context-value', el))).toBe('26k / 32k');
    expect(byTestId('context-gauge', el).getAttribute('data-level')).toBe('warn');
    expect(text(byTestId('context-warning', el))).toContain(
      'Les premiers messages vont bientôt sortir du contexte : nouvelle conversation conseillée.',
    );
    await click(fixture, byTestId('context-new', el));
    await settle(fixture);
    expect(queryTestId('model-summary', el)).not.toBeNull();
    expect(queryTestId('context-gauge', el)).toBeNull();
  });

  it('vitesse sous la réponse, avec l’estimation de GRAM ; rien pour une réponse interrompue', async () => {
    const { fixture, el } = await render(new FakeChat(CONVERSATIONS));
    await click(fixture, conversation(el, CONVERSATION_TODAY.id));
    await settle(fixture);
    const speed = byTestId('message-speed', el);
    expect(text(speed)).toBe(
      '212 tokens · 45 tok/s · chargement 2,1 s (GRAM estimait 45 tok/s (mesuré 43))',
    );
    expect(speed.getAttribute('tabindex')).toBe('0');
    // Réflexion enregistrée, repliée, avec sa durée.
    expect(text(byTestId('thinking-toggle', el))).toBe('Réflexion · 3 s');

    await click(fixture, conversation(el, CONVERSATION_WEEK.id));
    await settle(fixture);
    expect(queryTestId('message-speed', el)).toBeNull();
  });

  it('instructions : dialogue ; parties avec le premier message, puis enregistrées', async () => {
    const { fixture, el, chat, calls } = await render();
    const button = () => byTestId('composer-instructions', el);
    expect(text(button())).toBe('Instructions');

    await click(fixture, button());
    await settle(fixture);
    const field = document.querySelector<HTMLTextAreaElement>(
      '[data-testid="instructions-input"]',
    )!;
    expect(field.value).toBe('');
    expect(document.activeElement).toBe(field);
    await typeInto(fixture, field, 'Réponds en une phrase.');
    await click(fixture, document.querySelector<HTMLElement>('[data-testid="instructions-save"]')!);
    await settle(fixture);
    expect(text(button())).toBe('Instructions (définies)');
    expect(countOf(calls, 'set_instructions')).toBe(0);

    await typeInto(fixture, input(el), 'Bonjour');
    await press(fixture, input(el), 'Enter');
    await settle(fixture);
    expect(chat.requests[0].instructions).toBe('Réponds en une phrase.');

    // Conversation enregistrée : effacer les retire tout de suite.
    await click(fixture, button());
    await settle(fixture);
    const again = document.querySelector<HTMLTextAreaElement>(
      '[data-testid="instructions-input"]',
    )!;
    expect(again.value).toBe('Réponds en une phrase.');
    await typeInto(fixture, again, '');
    await click(fixture, document.querySelector<HTMLElement>('[data-testid="instructions-save"]')!);
    await settle(fixture);
    expect(calls.find((c) => c.cmd === 'set_instructions')?.args).toEqual({
      id: 'nouvelle-1',
      instructions: null,
    });
    expect(text(button())).toBe('Instructions');
    expect(document.body.textContent).toContain('Instructions retirées');
  });

  it('paramètres (Ctrl ,) : thème, mémoire du modèle, instructions des nouvelles conversations', async () => {
    const { fixture, el, chat } = await render();
    const prefs = TestBed.inject(PrefsStore);
    await press(fixture, document.body, ',', { ctrlKey: true });
    await settle(fixture);
    const dialog = () => document.querySelector<HTMLElement>('app-settings-dialog')!;
    expect(dialog()).not.toBeNull();

    await chooseOption(fixture, byTestId('settings-theme', dialog()), 'Sombre');
    expect(prefs.theme()).toBe('dark');
    await chooseOption(fixture, byTestId('settings-keep-alive', dialog()), '30 min');
    expect(prefs.keepAlive()).toBe('halfHour');
    await typeInto(fixture, byTestId('settings-instructions', dialog()), 'Tutoie-moi.');
    expect(prefs.instructions()).toBe('Tutoie-moi.');
    await click(fixture, byTestId('settings-close', dialog()));
    await settle(fixture);
    expect(dialog()).toBeNull();

    // La nouvelle conversation les reprend.
    expect(text(byTestId('composer-instructions', el))).toBe('Instructions (définies)');
    await typeInto(fixture, input(el), 'Salut');
    await press(fixture, input(el), 'Enter');
    await settle(fixture);
    expect(chat.requests[0]).toMatchObject({ keepAlive: 'halfHour', instructions: 'Tutoie-moi.' });

    // Le bouton de l'en-tête ouvre aussi les paramètres.
    await click(fixture, byTestId('settings', el));
    await settle(fixture);
    expect(dialog()).not.toBeNull();
  });
});
