import { TestBed } from '@angular/core/testing';
import { emit } from '@tauri-apps/api/event';
import { clearMocks } from '@tauri-apps/api/mocks';
import { App } from './app';
import { argsOf, mockBackend } from '../testing/backend';
import {
  byTestId,
  chooseMenuItem,
  chooseOption,
  click,
  menuItems,
  press,
  queryTestId,
  toggleSwitch,
  typeInto,
} from '../testing/dom';
import {
  GIB,
  HARDWARE,
  RANKING,
  SYNC_SUMMARY,
  makeItem,
  makePick,
  makeResponse,
} from '../testing/fixtures';
import type { RankRequest } from './core/generated/RankRequest';
import { FILTER_DEBOUNCE_MS } from './state/ranking.store';
import { DetailStore } from './state/detail.store';
import { UPDATE_CHECK_DELAY_MS } from './state/update.store';

async function start(updateCheckDelayMs = 60_000) {
  await TestBed.configureTestingModule({
    imports: [App],
    providers: [
      { provide: FILTER_DEBOUNCE_MS, useValue: 0 },
      { provide: UPDATE_CHECK_DELAY_MS, useValue: updateCheckDelayMs },
    ],
  }).compileComponents();
  const fixture = TestBed.createComponent(App);
  await fixture.whenStable();
  return fixture;
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

/** Toast affiché (région ajoutée au `<body>` par `ToastService`). */
const toast = () => document.querySelector<HTMLElement>('g-toast-region .g-toast');
/** Dialogues ouverts, le dernier en haut. */
const dialogs = () => [...document.querySelectorAll<HTMLElement>('[role="dialog"]')];
const dialogTitle = (d: HTMLElement | undefined) => text(d?.querySelector('h2'));

/** Lignes d'une liste du classement (`table-items`, `table-slow`, `table-too-big`). */
const rowsOf = (el: HTMLElement, table = 'table-items') => [
  ...el.querySelectorAll<HTMLElement>(`[data-testid="${table}"] [data-testid="rank-row"]`),
];
const namesOf = (el: HTMLElement, table = 'table-items') =>
  rowsOf(el, table).map((r) => text(r.querySelector('[data-testid="model-name"]')));
const lastRequest = (calls: ReturnType<typeof mockBackend>) =>
  argsOf(calls, 'rank').at(-1)!['request'] as RankRequest;

/** Classement avec des modèles lents et trop gros. */
const SECTIONS = makeResponse(RANKING.items, {
  totalItems: 40,
  slow: [
    makeItem('Llama-70B', {
      rank: 1,
      score: 60,
      tokS: 2.1,
      mode: { kind: 'hybrid', gpuLayers: 20, layers: 80 },
      size: 40 * GIB,
    }),
  ],
  totalSlow: 1,
  tooBigCount: 2,
});

describe('App', () => {
  afterEach(() => {
    clearMocks();
    localStorage.clear();
  });

  it('au lancement : matériel, classement et source des données, puis synchronisation sans forcer', async () => {
    const calls = mockBackend();
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;

    expect(text(el.querySelector('.brand__name'))).toBe('GRAM');
    expect(text(byTestId('hw-usable', el))).toBe(
      '61,6 Go utilisables par un modèle, à deux vitesses',
    );
    const titles = [...byTestId('memory-card', el).querySelectorAll('.zone__title')].map(text);
    expect(titles).toEqual(['VRAM · RTX 5080', 'RAM · 64 Go DDR5-6000, 2 canaux']);

    const names = [
      ...el.querySelectorAll('[data-testid="table-items"] [data-testid="model-name"]'),
    ];
    expect(names.map(text)).toEqual(['Qwen3-8B', 'Gemma-3-12B', 'Mistral-Small-24B']);
    expect(text(el.querySelector('[data-testid="data-source"]'))).toContain('Cache local');

    expect(argsOf(calls, 'sync_catalog')).toEqual([{ force: false }]);
    expect(argsOf(calls, 'get_settings')).toHaveLength(1);
  });

  it('les filtres relancent le classement ; le runtime choisi est enregistré', async () => {
    const calls = mockBackend();
    const fixture = await start();

    await chooseOption(fixture, 'filter-use-case', 'Code');
    expect(lastRequest(calls).useCase).toBe('code');

    expect(await toggleSwitch(fixture, 'filter-too-big')).toBe(true);
    expect(lastRequest(calls).showTooBig).toBe(true);

    await chooseOption(fixture, 'filter-runtime', 'llama.cpp');
    expect(lastRequest(calls).runtime).toBe('llamaCpp');
    const saved = argsOf(calls, 'save_settings').at(-1)!['settings'] as { runtime: string };
    expect(saved.runtime).toBe('llamaCpp');
  });

  it('touches 1 à 5 : usage, y compris en AZERTY sans Maj', async () => {
    const calls = mockBackend();
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;

    await press(fixture, document.body, '2');
    expect(lastRequest(calls).useCase).toBe('code');
    await press(fixture, document.body, '"', { code: 'Digit3' });
    expect(lastRequest(calls).useCase).toBe('reasoning');
    const pressed = byTestId('filter-use-case', el).querySelector('[aria-pressed="true"]');
    expect(text(pressed?.querySelector('.opt__label'))).toBe('Raisonnement');

    // Pas dans un champ de saisie.
    await press(fixture, byTestId('filter-search', el), '5');
    expect(lastRequest(calls).useCase).toBe('reasoning');
  });

  it('recherche un modèle depuis l’en-tête, avec Ctrl K, en gardant le rang', async () => {
    const calls = mockBackend({
      rank: (a) => {
        const search = (a['request'] as RankRequest).search;
        if (search === undefined) return RANKING;
        if (search === 'mistral') return makeResponse([RANKING.items[2]], { search });
        return makeResponse([], { search });
      },
    });
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;
    const input = byTestId<HTMLInputElement>('filter-search', el);
    expect(input.closest('header')).not.toBeNull();
    expect(input.getAttribute('aria-keyshortcuts')).toBe('Control+K');

    // Ctrl K place le curseur dans la recherche (Ctrl F aussi, l'ancien raccourci).
    await press(fixture, document.body, 'k', { ctrlKey: true });
    expect(document.activeElement).toBe(input);
    input.blur();
    await press(fixture, document.body, 'f', { ctrlKey: true });
    expect(document.activeElement).toBe(input);

    await typeInto(fixture, input, 'mistral');
    expect(lastRequest(calls).search).toBe('mistral');
    expect(namesOf(el)).toEqual(['Mistral-Small-24B']);
    expect(text(rowsOf(el)[0].querySelector('[data-col="rank"]'))).toBe('3');

    // Rien trouvé : message, et le bouton efface la recherche.
    await typeInto(fixture, input, 'introuvable');
    expect(text(byTestId('search-empty', el))).toContain(
      'Aucun modèle ne correspond à « introuvable ».',
    );
    await click(fixture, byTestId('empty-clearSearch', el));
    expect(input.value).toBe('');
    expect(argsOf(calls, 'rank').at(-1)!['request']).not.toHaveProperty('search');
    expect(rowsOf(el)).toHaveLength(3);

    // Le bouton du champ et Échap effacent aussi.
    await typeInto(fixture, input, 'mistral');
    await click(fixture, byTestId('field-clear', el));
    expect(input.value).toBe('');
    expect(argsOf(calls, 'rank').at(-1)!['request']).not.toHaveProperty('search');
    await typeInto(fixture, input, 'mistral');
    await press(fixture, input, 'Escape');
    expect(input.value).toBe('');
    expect(argsOf(calls, 'rank').at(-1)!['request']).not.toHaveProperty('search');
  });

  it('ouvre la fiche détail au clic sur une ligne', async () => {
    const calls = mockBackend();
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;

    await click(fixture, rowsOf(el)[0]);

    expect(argsOf(calls, 'get_model_detail')[0]['request']).toEqual({
      repoId: 'Qwen/Qwen3-8B-GGUF',
      useCase: 'general',
    });
    expect(text(el.querySelector('[data-testid="detail-title"]'))).toBe('Qwen3-8B');
  });

  it('fiche ancrée : classement resserré, bande machine et filtres repliés, Échap', async () => {
    mockBackend();
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;
    expect(queryTestId('detail-pane', el)).toBeNull();
    expect(queryTestId('filter-bar', el)).toBeNull();

    await click(fixture, rowsOf(el)[0]);
    expect(text(byTestId('detail-title', el))).toBe('Qwen3-8B');
    // Le classement reste affiché et cliquable, sans les conseils ni les colonnes secondaires.
    expect(queryTestId('picks', el)).toBeNull();
    expect(queryTestId('list-summary', el)).toBeNull();
    expect(byTestId('table-items', el).hasAttribute('data-docked')).toBe(true);
    expect(rowsOf(el)[0].getAttribute('aria-selected')).toBe('true');
    // Bande machine en pastilles, filtres en un bouton suivi des filtres actifs.
    expect(queryTestId('memory-card', el)).toBeNull();
    expect(queryTestId('machine-pills', el)).not.toBeNull();
    const pills = () => [...byTestId('filter-pills', el).querySelectorAll('li')].map(text);
    expect(pills()).toEqual(['Usage Polyvalent', 'Contexte 8k', 'Runtime Ollama']);
    expect(text(el.querySelector('app-status-bar .hints'))).toContain('Échap fermer la fiche');

    // Les touches 1 à 5 marchent toujours, filtres repliés.
    await press(fixture, document.body, '2', { code: 'Digit2' });
    expect(pills()[0]).toBe('Usage Code');

    // « Filtres » les ouvre par-dessus le classement ; Échap les referme d'abord.
    const toggle = byTestId('filters-toggle', el);
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    await click(fixture, toggle);
    const filters = el.querySelector('#filters-panel')!;
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(filters.classList).toContain('filters--open');
    expect(filters.contains(document.activeElement)).toBe(true);
    await press(fixture, document.activeElement!, 'Escape');
    expect(filters.classList).not.toContain('filters--open');
    expect(document.activeElement).toBe(toggle);
    expect(queryTestId('detail-pane', el)).not.toBeNull();

    // Un clic ailleurs les referme aussi.
    await click(fixture, toggle);
    document.body.dispatchEvent(new Event('pointerdown', { bubbles: true }));
    await fixture.whenStable();
    expect(filters.classList).not.toContain('filters--open');

    // Échap dans la fiche la ferme ; le focus revient sur la ligne du modèle.
    byTestId('close-detail', el).focus();
    await press(fixture, document.activeElement!, 'Escape');
    expect(queryTestId('detail-pane', el)).toBeNull();
    expect(document.activeElement).toBe(rowsOf(el)[0]);
    expect(queryTestId('memory-card', el)).not.toBeNull();
    expect(queryTestId('filter-bar', el)).toBeNull();

    // Le bouton ✕ aussi.
    await press(fixture, rowsOf(el)[1], 'Enter');
    expect(queryTestId('detail-pane', el)).not.toBeNull();
    await click(fixture, 'close-detail');
    expect(queryTestId('detail-pane', el)).toBeNull();
    expect(document.activeElement).toBe(rowsOf(el)[1]);
  });

  it('affiche la progression de la synchronisation et recalcule le classement à la fin', async () => {
    let finish: (v: unknown) => void = () => undefined;
    const calls = mockBackend({
      sync_catalog: () => new Promise((resolve) => (finish = resolve)),
    });
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;
    expect(argsOf(calls, 'rank')).toHaveLength(1);

    await emit('sync-progress', { phase: 'details', done: 45, total: 150 });
    await emit('rate-limit', { remaining: 412, resetS: 240 });
    fixture.detectChanges();
    const sync = byTestId('sync-progress', el);
    expect(text(sync)).toContain('Tailles des fichiers 45/150');
    const bar = sync.querySelector('[role="progressbar"]')!;
    expect(bar.getAttribute('aria-valuenow')).toBe('45');
    expect(bar.getAttribute('aria-valuemax')).toBe('150');
    expect(el.textContent).toContain('Quota Hugging Face : 412 requêtes');

    finish({ ...SYNC_SUMMARY, source: { kind: 'online', fetchedAt: new Date().toISOString() } });
    await new Promise((r) => setTimeout(r)); // la promesse de sync_catalog n'est pas suivie par Angular
    await fixture.whenStable();
    expect(el.querySelector('[data-testid="sync-progress"]')).toBeNull();
    expect(argsOf(calls, 'rank')).toHaveLength(2);
  });

  it('exporte le classement affiché depuis le menu', async () => {
    const calls = mockBackend();
    const fixture = await start();

    await chooseMenuItem(fixture, 'export', 'CSV');

    // Sans chemin : le backend ouvre le dialogue « Enregistrer sous ».
    expect(argsOf(calls, 'export_ranking')).toEqual([{ format: 'csv' }]);
    expect(toast()?.dataset['kind']).toBe('success');
    expect(text(toast()?.querySelector('.g-toast__message'))).toBe(
      'Classement exporté : C:\\Users\\x\\Documents\\classement-gram.csv',
    );
  });

  it('un export annulé ne montre rien', async () => {
    const calls = mockBackend({ export_ranking: () => null });
    const fixture = await start();
    await chooseMenuItem(fixture, 'export', 'Markdown');
    expect(argsOf(calls, 'export_ranking')).toEqual([{ format: 'md' }]);
    expect(toast()).toBeNull();
  });

  it('simulation en cours : signalée dans le bandeau, arrêtée en gardant les corrections', async () => {
    const simulated = {
      ...HARDWARE,
      overrides: {
        ...HARDWARE.overrides,
        ramChannels: 4,
        simulatedGpu: 'NVIDIA GeForce RTX 4090 24 Go',
      },
    };
    const calls = mockBackend({ get_hardware: () => simulated });
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;
    expect(text(el.querySelector('[data-testid="simulation"]'))).toContain('Simulation');
    const ranks = argsOf(calls, 'rank').length;

    (el.querySelector('[data-testid="stop-simulation"]') as HTMLButtonElement).click();
    await fixture.whenStable();
    expect(argsOf(calls, 'set_hardware_overrides')).toEqual([
      { overrides: { ...simulated.overrides, simulatedGpu: null, simulatedRamTotal: null } },
    ]);
    expect(el.querySelector('[data-testid="simulation"]')).toBeNull();
    expect(argsOf(calls, 'rank').length).toBe(ranks + 1);
  });

  it('signale une mise à jour trouvée au lancement et ouvre son dialogue', async () => {
    const calls = mockBackend({
      check_update: () => ({
        currentVersion: '0.1.0',
        update: { version: '0.2.0', notes: null, date: null },
      }),
    });
    const fixture = await start(0);
    await new Promise((r) => setTimeout(r, 10)); // vérification différée, hors d'Angular
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    expect(argsOf(calls, 'check_update')).toHaveLength(1);

    expect(text(toast()?.querySelector('.g-toast__message'))).toBe('GRAM 0.2.0 est disponible.');
    expect(text(el.querySelector('[data-testid="open-update"]'))).toBe('Mise à jour 0.2.0');

    await click(fixture, byTestId('toast-action', toast()!));
    expect(toast()).toBeNull();
    const dialog = dialogs().at(-1);
    expect(dialogTitle(dialog)).toBe('Mise à jour de GRAM');
    expect(dialog?.getAttribute('aria-modal')).toBe('true');
    expect(text(dialog)).toContain('GRAM 0.2.0 est disponible');
    // Déjà connue : pas de nouvelle vérification.
    expect(argsOf(calls, 'check_update')).toHaveLength(1);
  });

  it('rien n’est signalé si GRAM est à jour ; le bouton lance une vérification', async () => {
    const calls = mockBackend();
    const fixture = await start(0);
    await new Promise((r) => setTimeout(r, 10));
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    expect(argsOf(calls, 'check_update')).toHaveLength(1);
    expect(toast()).toBeNull();

    (el.querySelector('[data-testid="open-update"]') as HTMLButtonElement).click();
    await fixture.whenStable();
    await new Promise((r) => setTimeout(r));
    await fixture.whenStable();
    expect(text(dialogs().at(-1))).toContain('GRAM 0.1.0 est à jour.');
    expect(argsOf(calls, 'check_update')).toHaveLength(2);
  });

  it('ouvre les paramètres et le dialogue « Corriger… »', async () => {
    const calls = mockBackend();
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;

    const settings = byTestId('open-settings', el);
    settings.focus();
    await click(fixture, settings);
    let dialog = dialogs().at(-1)!;
    expect(dialogTitle(dialog)).toBe('Paramètres');
    // Focus sur le titre, relié au dialogue ; il revient au bouton à la fermeture.
    expect(dialog.getAttribute('aria-labelledby')).toBe(dialog.querySelector('h2')!.id);
    expect(document.activeElement).toBe(dialog.querySelector('h2'));
    expect(argsOf(calls, 'has_hf_token')).toHaveLength(1);
    await click(fixture, byTestId('dialog-close', dialog));
    expect(dialogs()).toHaveLength(0);
    expect(document.activeElement).toBe(settings);

    await click(fixture, byTestId('open-corrections', el));
    dialog = dialogs().at(-1)!;
    expect(dialogTitle(dialog)).toBe('Corriger le matériel');
    expect(argsOf(calls, 'hardware_options')).toHaveLength(1);
    // Échap ferme le dialogue.
    await press(fixture, document.body, 'Escape');
    expect(dialogs()).toHaveLength(0);
  });

  it('touche E : ouvre le menu Exporter, M exporte en Markdown', async () => {
    const calls = mockBackend();
    const fixture = await start();
    await press(fixture, document.body, 'e');
    const items = menuItems();
    expect(items.map(text)).toEqual(['Markdown (.md) M', 'CSV pour Excel (.csv) C']);
    expect(document.activeElement).toBe(items[0]);

    await press(fixture, items[0], 'm');
    expect(argsOf(calls, 'export_ranking')).toEqual([{ format: 'md' }]);
    expect(menuItems()).toEqual([]);
  });

  it('Échap ferme la fiche, mais vide d’abord la recherche', async () => {
    mockBackend();
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;
    const detail = TestBed.inject(DetailStore);
    detail.open('Qwen/Qwen3-8B-GGUF');
    await fixture.whenStable();

    const input = byTestId<HTMLInputElement>('filter-search', el);
    await typeInto(fixture, input, 'qwen');
    await press(fixture, input, 'Escape');
    expect(input.value).toBe('');
    expect(detail.isOpen()).toBe(true);

    await press(fixture, input, 'Escape');
    expect(detail.isOpen()).toBe(false);
  });

  it('« Actualiser les modèles » force la synchronisation ; R ré-analyse, sauf sous un dialogue', async () => {
    const calls = mockBackend();
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;
    await click(fixture, 'refresh-catalog');
    expect(argsOf(calls, 'sync_catalog')).toEqual([{ force: false }, { force: true }]);

    await press(fixture, document.body, 'r');
    expect(argsOf(calls, 'rescan_hardware')).toHaveLength(1);

    await click(fixture, byTestId('open-settings', el));
    await press(fixture, document.body, 'r');
    expect(argsOf(calls, 'rescan_hardware')).toHaveLength(1);
    expect(queryTestId('rescan', el)).not.toBeNull();
  });

  it('signale une erreur de classement et permet de réessayer', async () => {
    let fail = true;
    const calls = mockBackend({
      rank: () => {
        if (fail) throw 'aucun catalogue disponible';
        return RANKING;
      },
    });
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;
    expect(text(byTestId('ranking-error', el))).toContain(
      'Classement impossible : aucun catalogue disponible',
    );

    fail = false;
    await click(fixture, byTestId('retry-ranking', el));
    expect(argsOf(calls, 'rank').length).toBeGreaterThanOrEqual(2);
    expect(queryTestId('ranking-error', el)).toBeNull();
    expect(rowsOf(el)).toHaveLength(3);
  });

  it('onglets Exécutables, Lents et Trop gros avec leurs compteurs ; « Afficher 25 de plus »', async () => {
    const calls = mockBackend({
      rank: (a) => {
        const request = a['request'] as RankRequest;
        return request.showTooBig
          ? {
              ...SECTIONS,
              tooBig: [makeItem('DeepSeek-671B', { rank: 1, mode: { kind: 'tooBig' } })],
            }
          : SECTIONS;
      },
    });
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;
    const tabs = () => [
      ...byTestId('count-items', el).querySelectorAll<HTMLElement>('[role="tab"]'),
    ];

    expect(tabs().map(text)).toEqual(['Exécutables 40', 'Lents 1']);
    expect(tabs()[0].getAttribute('aria-selected')).toBe('true');
    expect(text(byTestId('list-summary', el))).toBe(
      'Polyvalent · contexte 8k · Ollama · variantes regroupées',
    );
    const panel = el.querySelector('[role="tabpanel"]')!;
    expect(panel.getAttribute('aria-labelledby')).toBe(tabs()[0].id);
    expect(text(byTestId('shown-count', el))).toBe('3 sur 40');

    await click(fixture, byTestId('show-more', el));
    expect(text(byTestId('show-more', el))).toBe('Afficher 25 de plus');
    expect(lastRequest(calls).limit).toBe(50);

    // ← → passent d'un onglet à l'autre.
    tabs()[0].focus();
    await press(fixture, tabs()[0], 'ArrowRight');
    expect(namesOf(el, 'table-slow')).toEqual(['Llama-70B']);
    expect(text(byTestId('list-summary', el))).toContain('sous 5 tok/s');
    expect(queryTestId('table-items', el)).toBeNull();

    // Trop gros : une ligne « 2 trop gros · Afficher », qui ajoute l'onglet et l'ouvre.
    expect(text(byTestId('too-big-hint', el))).toBe('2 trop gros · Afficher');
    await click(fixture, byTestId('show-too-big', el));
    expect(lastRequest(calls).showTooBig).toBe(true);
    expect(tabs().map(text)).toEqual(['Exécutables 40', 'Lents 1', 'Trop gros 2']);
    expect(namesOf(el, 'table-too-big')).toEqual(['DeepSeek-671B']);
    expect(queryTestId('too-big-hint', el)).toBeNull();

    // Sans l'interrupteur, l'onglet disparaît et on revient aux exécutables.
    await toggleSwitch(fixture, 'filter-too-big');
    expect(tabs()).toHaveLength(2);
    expect(namesOf(el)).toHaveLength(3);
  });

  it('↑ ↓ depuis la page : la ligne active alimente la carte mémoire ; Entrée ouvre la fiche', async () => {
    const calls = mockBackend();
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;
    const selection = () => text(byTestId('memory-selection', el));
    expect(selection()).toContain('En tête du classement : Qwen3-8B · Q8_0');

    await press(fixture, document.body, 'ArrowDown');
    expect(document.activeElement).toBe(rowsOf(el)[0]);
    expect(selection()).toContain('Sélection : Qwen3-8B · Q8_0');

    await press(fixture, rowsOf(el)[0], 'ArrowDown');
    await press(fixture, rowsOf(el)[1], 'ArrowDown');
    expect(document.activeElement).toBe(rowsOf(el)[2]);
    expect(selection()).toBe(
      'Sélection : Mistral-Small-24B · Q4_K_M — 13,9 Go en VRAM, 1,7 Go en RAM',
    );

    // Dans la liste des usages, ↑ ↓ restent dans la liste.
    const usage = byTestId('filter-use-case', el);
    const code = usage.querySelector<HTMLElement>('[data-value="code"]')!;
    code.focus();
    await press(fixture, code, 'ArrowUp');
    expect(document.activeElement?.getAttribute('data-value')).toBe('general');

    // Ailleurs, ↑ reprend avant la ligne active.
    (document.activeElement as HTMLElement).blur();
    await press(fixture, document.body, 'ArrowUp');
    expect(document.activeElement).toBe(rowsOf(el)[1]);

    await press(fixture, rowsOf(el)[1], 'Enter');
    expect(argsOf(calls, 'get_model_detail')[0]['request']).toMatchObject({
      repoId: 'org/Gemma-3-12B-GGUF',
    });
  });

  it('trie depuis le menu « Trier » comme depuis les en-têtes', async () => {
    mockBackend();
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;
    expect(text(byTestId('sort-menu', el))).toBe('Trier : classement');

    await chooseMenuItem(fixture, 'sort-menu', 'Vitesse');
    expect(text(byTestId('sort-menu', el))).toBe('Trier : vitesse');
    expect(namesOf(el)).toEqual(['Qwen3-8B', 'Gemma-3-12B', 'Mistral-Small-24B']);
    await click(fixture, byTestId('sort-tokS', el)); // sens inverse
    expect(namesOf(el)).toEqual(['Mistral-Small-24B', 'Gemma-3-12B', 'Qwen3-8B']);

    await chooseMenuItem(fixture, 'sort-menu', 'Ordre du classement');
    expect(text(byTestId('sort-menu', el))).toBe('Trier : classement');
    expect(namesOf(el)).toEqual(['Qwen3-8B', 'Gemma-3-12B', 'Mistral-Small-24B']);
  });

  it('remarques du runtime : repliables, masquables jusqu’à ce qu’elles changent', async () => {
    let notes = ['Première remarque.', 'Deuxième.', 'Troisième.'];
    mockBackend({ rank: () => makeResponse(RANKING.items, { notes }) });
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;
    const box = () => queryTestId('ranking-notes', el);

    expect(text(box())).toContain('Première remarque.');
    expect(text(box())).not.toContain('Deuxième.');
    const toggle = byTestId('notes-toggle', el);
    expect(text(toggle)).toBe('+2 remarques');
    await click(fixture, toggle);
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(text(box())).toContain('Troisième.');

    await click(fixture, byTestId('notes-hide', el));
    expect(box()).toBeNull();

    notes = ['Nouvelle remarque.'];
    await chooseOption(fixture, 'filter-runtime', 'LM Studio');
    expect(text(box())).toContain('Nouvelle remarque.');
    expect(queryTestId('notes-toggle', el)).toBeNull();
  });

  it('filtres trop stricts : état vide avec les actions utiles', async () => {
    const calls = mockBackend({
      rank: (a) => {
        const request = a['request'] as RankRequest;
        return request.ctx === undefined && request.maxSize === undefined
          ? SECTIONS
          : makeResponse([], { slow: SECTIONS.slow, totalSlow: 1 });
      },
    });
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;

    await chooseOption(fixture, 'filter-ctx', '128k');
    expect(lastRequest(calls).ctx).toBe(131072);
    await typeInto(fixture, 'filter-max-size', '4');
    const empty = byTestId('filters-empty', el);
    expect(text(empty)).toContain('Aucun modèle ne tient en mémoire assez vite avec ces filtres.');
    const actions = [...empty.querySelectorAll('button')].map(text);
    expect(actions).toEqual(['Voir les lents', 'Contexte par défaut', 'Retirer la taille max']);

    await click(fixture, byTestId('empty-clearMaxSize', el));
    expect(lastRequest(calls)).not.toHaveProperty('maxSize');
    expect(byTestId<HTMLInputElement>('filter-max-size', el).value).toBe('');
    await click(fixture, byTestId('empty-showSlow', el));
    expect(namesOf(el, 'table-slow')).toEqual(['Llama-70B']);
  });

  it('variantes regroupées sous leur famille : ← → depuis la page, interrupteur ; conseils en tête', async () => {
    const qwen = RANKING.items[0];
    const variant = makeItem('Qwen3-8B-abliterated', {
      rank: 4,
      score: 70,
      family: qwen.repoId,
    });
    const calls = mockBackend({
      rank: () => makeResponse([...RANKING.items, variant], { picks: [makePick('best', qwen)] }),
    });
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;

    // Bandeau des conseils entre la bande machine et le classement.
    const picks = byTestId('picks', el);
    expect(text(picks.querySelector('.pick__label'))).toBe('Meilleur choix');
    expect(text(byTestId('pick-name', picks))).toBe('Qwen3-8B');

    expect(namesOf(el)).toEqual(['Qwen3-8B', 'Gemma-3-12B', 'Mistral-Small-24B']);
    expect(text(byTestId('variant-count', rowsOf(el)[0]))).toBe('1 variante');

    // ← → hors du tableau : sur la ligne active, sinon la première.
    await press(fixture, document.body, 'ArrowRight');
    expect(namesOf(el)).toEqual([
      'Qwen3-8B',
      'Qwen3-8B-abliterated',
      'Gemma-3-12B',
      'Mistral-Small-24B',
    ]);
    expect(document.activeElement).toBe(rowsOf(el)[0]);
    (document.activeElement as HTMLElement).blur();
    await press(fixture, document.body, 'ArrowLeft');
    expect(namesOf(el)).toHaveLength(3);

    // Le regroupement se fait à l'affichage : l'interrupteur ne relance pas le calcul.
    const requests = argsOf(calls, 'rank').length;
    await toggleSwitch(fixture, 'filter-group');
    expect(namesOf(el)).toEqual([
      'Qwen3-8B',
      'Gemma-3-12B',
      'Mistral-Small-24B',
      'Qwen3-8B-abliterated',
    ]);
    expect(queryTestId('variant-count', el)).toBeNull();
    expect(argsOf(calls, 'rank')).toHaveLength(requests);
  });
});
