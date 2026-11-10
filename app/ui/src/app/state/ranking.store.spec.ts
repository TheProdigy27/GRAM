import { ApplicationRef } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import { argsOf, mockBackend } from '../../testing/backend';
import { GIB, RANKING, SETTINGS, makeItem, makeResponse } from '../../testing/fixtures';
import type { RankRequest } from '../core/generated/RankRequest';
import { CatalogStore } from './catalog.store';
import { FILTER_DEBOUNCE_MS, RankingStore } from './ranking.store';
import { SettingsStore } from './settings.store';

/** Laisse les effets et les loaders de `resource` aller au bout. */
async function settle(): Promise<void> {
  await TestBed.inject(ApplicationRef).whenStable();
}

function setup(debounceMs = 0) {
  TestBed.configureTestingModule({
    providers: [{ provide: FILTER_DEBOUNCE_MS, useValue: debounceMs }],
  });
  return TestBed.inject(RankingStore);
}

describe('RankingStore', () => {
  afterEach(() => clearMocks());

  it('recalcule après un enregistrement des paramètres, pas au chargement', async () => {
    const calls = mockBackend();
    setup();
    const settings = TestBed.inject(SettingsStore);
    await settings.load();
    await settle();
    expect(argsOf(calls, 'rank')).toHaveLength(1);

    await settings.save({ ...SETTINGS, estimate: { ...SETTINGS.estimate, vramMargin: 0.2 } });
    await settle();
    expect(argsOf(calls, 'rank')).toHaveLength(2);
  });

  it('premier classement immédiat, avec les valeurs par défaut du backend', async () => {
    const calls = mockBackend();
    const store = setup(10_000); // le premier calcul ne passe pas par le debounce
    await settle();

    expect(argsOf(calls, 'rank')).toEqual([
      { request: { useCase: 'general', showTooBig: false, limit: 25 } },
    ]);
    expect(store.response()).toEqual(RANKING);
    expect(store.effectiveCtx()).toBe(8192);
    expect(store.effectiveRuntime()).toBe('ollama');
  });

  it('construit la requête à partir des filtres', async () => {
    const calls = mockBackend();
    const store = setup();
    await settle();

    store.setCtx(32768);
    store.setRuntime('llamaCpp');
    store.setMaxSizeGb(12);
    store.setShowTooBig(true);
    store.showMore();
    await settle();

    const last = argsOf(calls, 'rank').at(-1)!['request'] as RankRequest;
    expect(last).toEqual({
      useCase: 'general',
      ctx: 32768,
      runtime: 'llamaCpp',
      maxSize: 12 * GIB,
      showTooBig: true,
      limit: 50,
    });

    // Changer de cas d'usage reprend son contexte par défaut et revient à la première page.
    store.setUseCase('code');
    store.setMaxSizeGb(null);
    await settle();
    expect(argsOf(calls, 'rank').at(-1)!['request']).toEqual({
      useCase: 'code',
      runtime: 'llamaCpp',
      showTooBig: true,
      limit: 25,
    });
  });

  it('regroupe les changements rapprochés (debounce) et garde l’ancien classement affiché', async () => {
    let answer = RANKING;
    const calls = mockBackend({ rank: () => answer });
    const store = setup(30);
    await settle();
    expect(argsOf(calls, 'rank')).toHaveLength(1);

    answer = makeResponse([makeItem('Autre')], { ctx: 65536 });
    store.setCtx(16384);
    store.setCtx(32768);
    store.setCtx(65536);
    TestBed.tick();
    // Pendant le calcul, le classement précédent reste affiché.
    expect(store.loading()).toBe(true);
    expect(store.response()).toEqual(RANKING);

    await settle();
    const requests = argsOf(calls, 'rank').map((a) => (a['request'] as RankRequest).ctx);
    expect(requests).toEqual([undefined, 65536]);
    expect(store.response()?.items[0].displayName).toBe('Autre');
  });

  it('recalcule quand le catalogue change, et garde le dernier classement après une erreur', async () => {
    let fail = false;
    const calls = mockBackend({
      rank: () => {
        if (fail) throw 'Modèle absent du catalogue.';
        return RANKING;
      },
    });
    const store = setup();
    await settle();

    fail = true;
    TestBed.inject(CatalogStore).version.update((v) => v + 1);
    await settle();

    expect(argsOf(calls, 'rank')).toHaveLength(2);
    expect(store.error()).toBe('Modèle absent du catalogue.');
    expect(store.response()).toEqual(RANKING);
  });

  it('retrouve une ligne dans toutes les sections', async () => {
    const slow = makeItem('Lent', { tokS: 2 });
    mockBackend({ rank: () => makeResponse([makeItem('Rapide')], { slow: [slow], totalSlow: 1 }) });
    const store = setup();
    await settle();
    expect(store.findItem(slow.repoId)?.displayName).toBe('Lent');
    expect(store.findItem('inconnu')).toBeUndefined();
  });
});
