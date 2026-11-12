import { DialogRef } from '@angular/cdk/dialog';
import { ApplicationRef } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import { type Handler, argsOf, mockBackend } from '../../../testing/backend';
import { allByTestId, byTestId, click, press, text, typeInto } from '../../../testing/dom';
import { MODEL, makeDetail } from '../../../testing/fixtures';
import type { DetailRequest } from '../../core/generated/DetailRequest';
import type { RankRequest } from '../../core/generated/RankRequest';
import { DetailStore } from '../../state/detail.store';
import { FILTER_DEBOUNCE_MS, RankingStore } from '../../state/ranking.store';
import { Palette } from './palette';

/** Fiche d'un modèle quelconque : le nom vient du dépôt (« org/Disparu-GGUF » → « Disparu »). */
function detailOf(a: Record<string, unknown>) {
  const { repoId } = a['request'] as DetailRequest;
  const displayName = repoId.split('/')[1].replace(/-GGUF$/, '');
  return { ...makeDetail(), model: { ...MODEL, repoId, displayName } };
}

async function open(
  handlers: Record<string, Handler> = {},
  before?: (s: { ranking: RankingStore; detail: DetailStore }) => Promise<void> | void,
) {
  const calls = mockBackend({ get_model_detail: detailOf, ...handlers });
  let closed = 0;
  TestBed.configureTestingModule({
    imports: [Palette],
    providers: [
      { provide: DialogRef, useValue: { config: {}, close: () => closed++ } },
      { provide: FILTER_DEBOUNCE_MS, useValue: 0 },
    ],
  });
  const ranking = TestBed.inject(RankingStore);
  const detail = TestBed.inject(DetailStore);
  await TestBed.inject(ApplicationRef).whenStable();
  await before?.({ ranking, detail });
  const fixture = TestBed.createComponent(Palette);
  await fixture.whenStable();
  const el = fixture.nativeElement as HTMLElement;
  const input = byTestId<HTMLInputElement>('palette-input', el);
  const options = () => allByTestId('palette-option', el);
  const labels = (kind?: string) =>
    options()
      .filter((o) => !kind || o.dataset['kind'] === kind)
      .map((o) => text(o.querySelector('.opt__label')));
  const groups = () =>
    [...el.querySelectorAll<HTMLElement>('[role="group"]')].map((g) => g.dataset['group']);
  const active = () => document.getElementById(input.getAttribute('aria-activedescendant') ?? '');
  return {
    calls,
    fixture,
    el,
    input,
    options,
    labels,
    groups,
    active,
    ranking,
    detail,
    closed: () => closed,
  };
}

/** Ouvre des fiches (la plus récente en dernier) et attend leur chargement. */
async function openDetails(detail: DetailStore, ...repoIds: string[]) {
  for (const id of repoIds) {
    detail.open(id);
    await TestBed.inject(ApplicationRef).whenStable();
  }
  detail.close();
}

describe('Palette', () => {
  afterEach(() => clearMocks());

  it('sans saisie : fiches récentes avec leur rang actuel, puis les commandes principales', async () => {
    const p = await open({}, ({ detail }) =>
      openDetails(detail, 'Qwen/Qwen3-8B-GGUF', 'org/Disparu-GGUF'),
    );
    expect(p.groups()).toEqual(['Récents', 'Commandes']);
    expect(p.labels('model')).toEqual(['Disparu', 'Qwen3-8B']);

    // Chaque récent est recherché par son dépôt, avec les filtres du classement.
    const searches = argsOf(p.calls, 'search_ranking').map((a) => a['request'] as RankRequest);
    expect(searches.map((r) => r.search).sort()).toEqual([
      'Qwen/Qwen3-8B-GGUF',
      'org/Disparu-GGUF',
    ]);
    expect(searches[0]).toMatchObject({ useCase: 'general', showTooBig: true });

    const [missing, qwen] = p.options();
    expect(text(missing.querySelector('.opt__sub'))).toBe('hors du classement avec ces filtres');
    expect(text(qwen.querySelector('.opt__rank'))).toBe('#1');
    expect(text(qwen.querySelector('.opt__sub'))).toBe('Q8_0 · 8,1 Go');
    expect(text(qwen.querySelector('.opt__tok'))).toBe('~95 tok/s');
    expect(text(qwen.querySelector('.opt__score'))).toBe('86');
    expect(text(qwen.querySelector('.mode-badge'))).toBe('GPU');
    expect(qwen.getAttribute('aria-label')).toBe(
      'Qwen3-8B, rang 1, Q8_0 · 8,1 Go, GPU, ~95 tok/s, score 86',
    );
    expect(text(byTestId('palette-status', p.el))).toBe('Fiches ouvertes pendant la session');

    // Pas d'usages ni de contextes sans saisie ; raccourcis affichés à droite.
    expect(p.labels('command')).toEqual([
      'Exporter le classement en Markdown',
      'Exporter le classement en CSV',
      'Ré-analyser le matériel',
      'Actualiser les modèles',
      'Corriger ou simuler le matériel',
      'Ouvrir les paramètres',
      'Raccourcis clavier',
    ]);
    const kbd = (label: string) =>
      text(
        p
          .options()
          .find((o) => text(o.querySelector('.opt__label')) === label)
          ?.querySelector('kbd'),
      );
    expect(kbd('Exporter le classement en CSV')).toBe('E C');
    expect(kbd('Raccourcis clavier')).toBe('?');

    // Combobox : le focus reste dans le champ, l'option active est désignée.
    expect(document.activeElement).toBe(p.input);
    expect(p.input.getAttribute('role')).toBe('combobox');
    expect(p.input.getAttribute('aria-controls')).toBe(byTestId('palette-list', p.el).id);
    expect(p.active()).toBe(missing);
    expect(missing.getAttribute('aria-selected')).toBe('true');
  });

  it('saisie : modèles du backend avec leur rang, sans toucher au classement exporté ; Ctrl Entrée filtre', async () => {
    const p = await open();
    const ranks = argsOf(p.calls, 'rank').length;

    await typeInto(p.fixture, p.input, ' mistral ');
    expect(argsOf(p.calls, 'search_ranking').at(-1)!['request']).toEqual({
      useCase: 'general',
      showTooBig: false,
      search: 'mistral',
      limit: 6,
    });
    // `rank` retient la dernière requête pour l'export : la palette ne l'appelle pas.
    expect(argsOf(p.calls, 'rank')).toHaveLength(ranks);

    expect(p.groups()).toEqual(['Recherche', 'Modèles']);
    const [filter, model] = p.options();
    expect(text(filter.querySelector('.opt__label'))).toBe('Filtrer le classement sur « mistral »');
    expect(text(filter.querySelector('.opt__hint'))).toBe('1 modèle');
    expect(text(filter.querySelector('kbd'))).toBe('Ctrl Entrée');
    expect(text(model.querySelector('.opt__rank'))).toBe('#3');
    expect(text(model.querySelector('.mode-badge'))).toBe('Hybride');
    expect(text(byTestId('palette-status', p.el))).toBe(
      'Rang dans le classement complet',
    );

    await press(p.fixture, p.input, 'Enter', { ctrlKey: true });
    expect(p.ranking.search()).toBe('mistral');
    expect(p.closed()).toBe(1);
  });

  it('↑ ↓ choisissent l’option active, Entrée ouvre la fiche du modèle', async () => {
    const p = await open();
    await typeInto(p.fixture, p.input, 'qwen3');
    const [filter, qwen] = p.options();
    expect(p.active()).toBe(filter);

    await press(p.fixture, p.input, 'ArrowDown');
    expect(p.active()).toBe(qwen);
    expect(qwen.getAttribute('aria-selected')).toBe('true');
    expect(filter.getAttribute('aria-selected')).toBe('false');
    await press(p.fixture, p.input, 'ArrowDown'); // dernière option : on y reste
    expect(p.active()).toBe(p.options().at(-1));
    await press(p.fixture, p.input, 'ArrowUp');
    await press(p.fixture, p.input, 'ArrowUp');
    expect(p.active()).toBe(filter);
    await press(p.fixture, p.input, 'ArrowDown');

    // Le survol choisit aussi.
    p.options()[0].dispatchEvent(new MouseEvent('mousemove', { bubbles: true }));
    await p.fixture.whenStable();
    expect(p.active()).toBe(filter);
    await press(p.fixture, p.input, 'ArrowDown');

    await press(p.fixture, p.input, 'Enter');
    expect(p.detail.repoId()).toBe('Qwen/Qwen3-8B-GGUF');
    expect(p.closed()).toBe(1);
  });

  it('Entrée sur « Filtrer le classement », rien trouvé compris', async () => {
    const p = await open();
    await typeInto(p.fixture, p.input, 'introuvable');
    expect(p.groups()).toEqual(['Recherche']);
    expect(text(p.options()[0].querySelector('.opt__hint'))).toBe('aucun modèle');
    expect(text(byTestId('palette-status', p.el))).toBe('');
    await press(p.fixture, p.input, 'Enter');
    expect(p.ranking.search()).toBe('introuvable');
    expect(p.closed()).toBe(1);
  });

  it('commandes : usages et contextes à la saisie ; la palette se ferme puis la commande part', async () => {
    const p = await open();
    await typeInto(p.fixture, p.input, 'code');
    const useCode = p.options().find((o) => o.dataset['key'] === 'command:use-code')!;
    expect(text(useCode.querySelector('.opt__label'))).toBe('Usage : Code');
    expect(text(useCode.querySelector('.opt__hint'))).toBe('filtre');
    expect(text(useCode.querySelector('kbd'))).toBe('2');
    await click(p.fixture, useCode);
    expect(p.closed()).toBe(1);
    expect(p.ranking.useCase()).toBe('code');

    await typeInto(p.fixture, p.input, '32k');
    expect(p.labels('command')).toEqual(['Contexte : 32k']);
    await click(p.fixture, p.options().at(-1)!);
    expect(p.ranking.ctx()).toBe(32768);
    await typeInto(p.fixture, p.input, 'contexte defaut');
    expect(p.labels('command')).toEqual(['Contexte par défaut']);

    await typeInto(p.fixture, p.input, 'csv');
    await click(
      p.fixture,
      p.options().find((o) => o.dataset['key'] === 'command:export-csv')!,
    );
    expect(argsOf(p.calls, 'export_ranking')).toEqual([{ format: 'csv' }]);
  });

  it('recherche active : saisie pré-remplie et sélectionnée, commande « Effacer la recherche »', async () => {
    const p = await open({}, ({ ranking }) => ranking.setSearch('mistral'));
    expect(p.input.value).toBe('mistral');
    expect([p.input.selectionStart, p.input.selectionEnd]).toEqual([0, 7]);

    // Ctrl K dans la palette sélectionne de nouveau la saisie.
    p.input.setSelectionRange(7, 7);
    await press(p.fixture, p.input, 'k', { ctrlKey: true });
    expect([p.input.selectionStart, p.input.selectionEnd]).toEqual([0, 7]);

    await typeInto(p.fixture, p.input, '');
    expect(p.labels('command')[0]).toBe('Effacer la recherche « mistral »');
    await click(
      p.fixture,
      p.options().find((o) => o.dataset['key'] === 'command:clear-search')!,
    );
    expect(p.ranking.search()).toBe('');
  });

  it('signale une recherche impossible', async () => {
    const p = await open({
      search_ranking: () => {
        throw 'catalogue indisponible';
      },
    });
    await typeInto(p.fixture, p.input, 'qwen');
    expect(text(byTestId('palette-status', p.el))).toBe(
      'Recherche impossible : réessayer dans un instant',
    );
    expect(p.groups()).toEqual(['Recherche']);
  });
});
