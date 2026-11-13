import { TestBed } from '@angular/core/testing';
import { PREFS_KEY, PREFS_STORAGE, PrefsStore } from './prefs.store';

describe('PrefsStore', () => {
  afterEach(() => localStorage.clear());

  it('valeurs par défaut, puis choix gardés dans le stockage local', () => {
    const prefs = TestBed.inject(PrefsStore);
    expect(prefs.theme()).toBe('system');
    expect(prefs.model()).toBeNull();
    prefs.setTheme('dark');
    prefs.setModel('gram/qwen3-8b:q4_k_m');
    expect(JSON.parse(localStorage.getItem(PREFS_KEY)!)).toEqual({
      theme: 'dark',
      model: 'gram/qwen3-8b:q4_k_m',
    });

    TestBed.resetTestingModule();
    const again = TestBed.inject(PrefsStore);
    expect(again.theme()).toBe('dark');
    expect(again.model()).toBe('gram/qwen3-8b:q4_k_m');
  });

  it('valeurs invalides ou stockage illisible : valeurs par défaut', () => {
    localStorage.setItem(PREFS_KEY, JSON.stringify({ theme: 'rose', model: 42 }));
    const prefs = TestBed.inject(PrefsStore);
    expect(prefs.theme()).toBe('system');
    expect(prefs.model()).toBeNull();
    TestBed.resetTestingModule();

    localStorage.setItem(PREFS_KEY, '{pas du json');
    expect(TestBed.inject(PrefsStore).theme()).toBe('system');
  });

  it('sans stockage : les choix valent pour la session', () => {
    const broken = {
      getItem: () => {
        throw new Error('refusé');
      },
      setItem: () => {
        throw new Error('plein');
      },
    } as unknown as Storage;
    TestBed.configureTestingModule({ providers: [{ provide: PREFS_STORAGE, useValue: broken }] });
    const prefs = TestBed.inject(PrefsStore);
    prefs.setTheme('light');
    expect(prefs.theme()).toBe('light');
  });
});
