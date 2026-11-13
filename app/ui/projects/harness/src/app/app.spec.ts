import { TestBed } from '@angular/core/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import {
  byTestId,
  chooseMenuItem,
  chooseOption,
  click,
  menuItems,
  openMenu,
  optionLabels,
  press,
  queryTestId,
  text,
} from '@gram/testing/dom';
import type { ModelList } from '@gram/types/ModelList';
import { countOf, mockBackend, type Handler } from '../testing/backend';
import {
  GRAM_MODEL,
  MODELS,
  NO_MODELS,
  OLLAMA_ABSENT,
  OLLAMA_STOPPED,
  OTHER_MODEL,
  VISION_MODEL,
} from '../testing/fixtures';
import { App } from './app';
import { PREFS_KEY } from './state/prefs.store';

async function render(handlers: Record<string, Handler> = {}) {
  const calls = mockBackend(handlers);
  const fixture = TestBed.createComponent(App);
  await fixture.whenStable();
  const el = fixture.nativeElement as HTMLElement;
  return { fixture, el, calls };
}

const savedPrefs = () => JSON.parse(localStorage.getItem(PREFS_KEY) ?? '{}');

describe('App (GRAM Harness)', () => {
  afterEach(() => {
    clearMocks();
    localStorage.clear();
    delete document.documentElement.dataset['theme'];
  });

  it('au lancement : liste lue, premier modèle choisi, ses faits et ses capacités', async () => {
    const { el, calls } = await render();
    expect(countOf(calls, 'list_models')).toBe(1);

    expect(text(byTestId('model-select', el))).toBe('Qwen3 8B Q4_K_M');
    expect(text(byTestId('model-summary', el).querySelector('h1'))).toBe('Qwen3 8B');
    expect(text(byTestId('model-name', el))).toBe(GRAM_MODEL.name);
    const facts = [...byTestId('model-facts', el).querySelectorAll('li')].map(text);
    expect(facts).toEqual([
      'Q4_K_M',
      '4,7 Go',
      'contexte 16k',
      '~45 tok/s estimés',
      '~43 tok/s mesurés',
    ]);
    expect(queryTestId('capability-thinking', el)).not.toBeNull();
    expect(queryTestId('capability-tools', el)).not.toBeNull();
    expect(queryTestId('capability-vision', el)).toBeNull();

    // Barre latérale : conversations vides, état d'Ollama.
    expect(byTestId<HTMLButtonElement>('new-conversation', el).disabled).toBe(true);
    expect(text(byTestId('conversations-empty', el))).toBe('Aucune conversation pour l’instant.');
    expect(text(byTestId('ollama-status', el))).toBe('Ollama en cours · 3 modèles');
  });

  it('sélecteur : modèles de GRAM puis autres, capacités en étiquettes ; le choix est retenu', async () => {
    const { fixture, el } = await render();
    await click(fixture, byTestId('model-select', el).querySelector('button')!);
    const list = document.querySelector<HTMLElement>('[role="listbox"]')!;
    const groups = [...list.querySelectorAll('[role="group"]')];
    expect(groups.map((g) => text(g.querySelector('.g-select__group-label')))).toEqual([
      'Ajoutés depuis GRAM',
      'Autres modèles d’Ollama',
    ]);
    expect(optionLabels(groups[0])).toEqual(['Qwen3 8B Q4_K_M', 'Gemma 3 4B Q4_K_M']);
    expect(optionLabels(groups[1])).toEqual(['qwen3:8b']);
    const gemma = groups[0].querySelectorAll('[role="option"]')[1];
    expect([...gemma.querySelectorAll('.g-select__tag')].map(text)).toEqual(['vision']);
    expect(text(gemma.querySelector('.g-select__hint'))).toBe('3,1 Go');

    await chooseOption(fixture, 'model-select', 'Gemma 3 4B Q4_K_M');
    expect(text(byTestId('model-summary', el).querySelector('h1'))).toBe('Gemma 3 4B');
    expect(queryTestId('capability-vision', el)).not.toBeNull();
    expect(queryTestId('capability-thinking', el)).toBeNull();
    expect(savedPrefs().model).toBe(VISION_MODEL.name);
  });

  it('modèle retenu repris au lancement ; disparu, le premier le remplace', async () => {
    localStorage.setItem(PREFS_KEY, JSON.stringify({ model: OTHER_MODEL.name }));
    const { el } = await render();
    expect(text(byTestId('model-select', el))).toBe('qwen3:8b');
    // Nom technique : pas de seconde ligne, contexte non réglé, pas de vitesse de GRAM.
    expect(queryTestId('model-name', el)).toBeNull();
    const facts = [...byTestId('model-facts', el).querySelectorAll('li')].map(text);
    expect(facts).toEqual(['Q4_K_M', '4,9 Go', 'contexte choisi par Ollama']);
    TestBed.resetTestingModule();

    localStorage.setItem(PREFS_KEY, JSON.stringify({ model: 'gram/disparu:q8_0' }));
    const again = await render();
    expect(text(byTestId('model-select', again.el))).toBe('Qwen3 8B Q4_K_M');
  });

  const states: [string, ModelList, string, string][] = [
    ['ollama-absent', OLLAMA_ABSENT, 'Ollama n’est pas installé', 'Ollama introuvable'],
    ['ollama-stopped', OLLAMA_STOPPED, 'Ollama est arrêté', 'Ollama arrêté'],
    ['no-models', NO_MODELS, 'Aucun modèle dans Ollama', 'Ollama en cours · 0 modèle'],
  ];
  for (const [id, list, heading, status] of states) {
    it(`état vide : ${heading}`, async () => {
      const { el } = await render({ list_models: () => list });
      expect(text(byTestId(id, el))).toContain(heading);
      expect(queryTestId('model-summary', el)).toBeNull();
      expect(text(byTestId('ollama-status', el))).toBe(status);
      const select = byTestId('model-select', el).querySelector('button')!;
      expect(select.disabled).toBe(true);
      expect(text(select)).toBe('Aucun modèle');
    });
  }

  it('Ollama absent : « Installer Ollama » ouvre la page de téléchargement', async () => {
    const { fixture, el, calls } = await render({ list_models: () => OLLAMA_ABSENT });
    expect(text(byTestId('ollama-absent', el))).toContain('Installer Ollama, puis Actualiser');
    await click(fixture, byTestId('install-ollama', el));
    expect(countOf(calls, 'open_ollama_download')).toBe(1);
    expect(queryTestId('open-error', el)).toBeNull();
  });

  it('Ollama absent : un navigateur qui ne s’ouvre pas donne l’adresse', async () => {
    const { fixture, el } = await render({
      list_models: () => OLLAMA_ABSENT,
      open_ollama_download: () => {
        throw 'Navigateur non ouvert (x) : aller sur https://ollama.com/download';
      },
    });
    await click(fixture, byTestId('install-ollama', el));
    expect(text(byTestId('open-error', el))).toContain('https://ollama.com/download');
  });

  it('Ollama lancé puis « Actualiser » : les modèles apparaissent', async () => {
    let list = OLLAMA_STOPPED;
    const { fixture, el, calls } = await render({ list_models: () => list });
    list = MODELS;
    await click(fixture, byTestId('state-refresh', el));
    expect(countOf(calls, 'list_models')).toBe(2);
    expect(queryTestId('model-summary', el)).not.toBeNull();
  });

  it('réponse illisible : message et « Réessayer »', async () => {
    let fail = true;
    const { fixture, el } = await render({
      list_models: () => {
        if (fail) throw '/api/tags : expected value at line 1 column 1';
        return MODELS;
      },
    });
    const error = byTestId('models-error', el);
    expect(error.getAttribute('role')).toBe('alert');
    expect(text(error)).toContain('/api/tags : expected value');
    expect(text(byTestId('ollama-status', el))).toBe('Ollama : réponse illisible');
    fail = false;
    await click(fixture, byTestId('error-refresh', el));
    expect(queryTestId('models-error', el)).toBeNull();
    expect(queryTestId('model-summary', el)).not.toBeNull();
  });

  it('R et le bouton « Actualiser » relisent la liste ; le focus de la fenêtre aussi, au plus toutes les 2 s', async () => {
    const { fixture, el, calls } = await render();
    await press(fixture, document.body, 'r');
    expect(countOf(calls, 'list_models')).toBe(2);
    await click(fixture, byTestId('refresh', el));
    expect(countOf(calls, 'list_models')).toBe(3);

    window.dispatchEvent(new Event('focus'));
    await fixture.whenStable();
    expect(countOf(calls, 'list_models')).toBe(4);
    window.dispatchEvent(new Event('focus'));
    await fixture.whenStable();
    expect(countOf(calls, 'list_models')).toBe(4);
  });

  it('une seule lecture à la fois', async () => {
    let resolve: (l: ModelList) => void = () => undefined;
    const { fixture, el, calls } = await render({
      list_models: () => new Promise<ModelList>((r) => (resolve = r)),
    });
    expect(text(byTestId('models-loading', el))).toBe('Recherche des modèles d’Ollama…');
    await click(fixture, byTestId('refresh', el));
    expect(countOf(calls, 'list_models')).toBe(1);
    resolve(MODELS);
    // La réponse traverse plusieurs promesses de l'IPC simulé avant d'arriver au store.
    await new Promise((r) => setTimeout(r));
    await fixture.whenStable();
    expect(queryTestId('models-loading', el)).toBeNull();
  });

  it('thème : menu clair, sombre ou système ; le choix est appliqué et retenu', async () => {
    const { fixture, el, calls } = await render();
    const items = await openMenu(fixture, byTestId('theme-menu', el));
    expect(items.map(text)).toEqual(['Clair', 'Sombre', 'Celui du système (choisi)']);
    await click(fixture, items[1]);
    expect(document.documentElement.dataset['theme']).toBe('dark');
    expect(savedPrefs().theme).toBe('dark');
    const themes = calls.filter((c) => c.cmd === 'plugin:window|set_theme');
    expect(themes.map((c) => c.args['value'])).toEqual(['dark']);

    await chooseMenuItem(fixture, byTestId('theme-menu', el), 'Celui du système');
    expect(document.documentElement.dataset['theme']).toBeUndefined();
    expect(menuItems()).toEqual([]);
  });
});
