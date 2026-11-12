import { TestBed } from '@angular/core/testing';
import { DISPLAY_PREFS_KEY, DISPLAY_STORAGE, DisplayStore } from './display.store';

describe('DisplayStore', () => {
  afterEach(() => localStorage.clear());

  it('valeurs par défaut, puis gardées dans le stockage local pour le prochain lancement', () => {
    const store = TestBed.inject(DisplayStore);
    expect(store.groupVariants()).toBe(true);
    expect(store.picksCollapsed()).toBe(false);
    expect(store.quantView()).toBe('chart');

    store.setPicksCollapsed(true);
    store.setGroupVariants(false);
    store.setQuantView('table');
    expect(JSON.parse(localStorage.getItem(DISPLAY_PREFS_KEY)!)).toEqual({
      groupVariants: false,
      picksCollapsed: true,
      quantView: 'table',
    });

    TestBed.resetTestingModule();
    const next = TestBed.inject(DisplayStore);
    expect(next.groupVariants()).toBe(false);
    expect(next.picksCollapsed()).toBe(true);
    expect(next.quantView()).toBe('table');
  });

  it('stockage illisible ou indisponible : valeurs par défaut, pour la session', () => {
    localStorage.setItem(DISPLAY_PREFS_KEY, '{pas du JSON');
    expect(TestBed.inject(DisplayStore).groupVariants()).toBe(true);

    TestBed.resetTestingModule();
    const failing = {
      getItem: () => {
        throw new Error('refusé');
      },
      setItem: () => {
        throw new Error('plein');
      },
    } as unknown as Storage;
    TestBed.configureTestingModule({
      providers: [{ provide: DISPLAY_STORAGE, useValue: failing }],
    });
    const store = TestBed.inject(DisplayStore);
    expect(store.picksCollapsed()).toBe(false);
    store.setPicksCollapsed(true);
    expect(store.picksCollapsed()).toBe(true);
  });
});
