import { TestBed } from '@angular/core/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import { argsOf, mockBackend } from '../../testing/backend';
import { DISPLAY_PREFS_KEY, DisplayStore } from '../state/display.store';
import { DARK_SCHEME_QUERY, ThemeService } from './theme.service';

/** `matchMedia('(prefers-color-scheme: dark)')` simulé ; `change()` imite le système qui bascule. */
function fakeQuery(matches: boolean) {
  const query = Object.assign(new EventTarget(), { matches });
  return Object.assign(query, {
    change(dark: boolean) {
      query.matches = dark;
      query.dispatchEvent(Object.assign(new Event('change'), { matches: dark }));
    },
  });
}

function setup(systemDark: boolean) {
  const query = fakeQuery(systemDark);
  TestBed.configureTestingModule({
    providers: [{ provide: DARK_SCHEME_QUERY, useValue: query }],
  });
  return { query, theme: TestBed.inject(ThemeService) };
}

const attr = () => document.documentElement.dataset['theme'];
const windowThemes = (calls: ReturnType<typeof mockBackend>) =>
  argsOf(calls, 'plugin:window|set_theme').map((a) => a['value']);

describe('ThemeService', () => {
  afterEach(() => {
    clearMocks();
    localStorage.clear();
    delete document.documentElement.dataset['theme'];
  });

  it('par défaut : le thème du système, suivi en direct, sans rien forcer', () => {
    const calls = mockBackend();
    const { query, theme } = setup(true);
    expect(theme.preference()).toBe('system');
    expect(theme.theme()).toBe('dark');
    expect(attr()).toBeUndefined();

    query.change(false);
    expect(theme.theme()).toBe('light');
    expect(windowThemes(calls)).toEqual([]);
  });

  it('bascule : force le thème opposé sur la page et la fenêtre, et le garde', () => {
    const calls = mockBackend();
    const { query, theme } = setup(true);

    theme.toggle();
    expect(theme.theme()).toBe('light');
    expect(attr()).toBe('light');
    // Le système qui change ne défait pas le choix.
    query.change(false);
    query.change(true);
    expect(theme.theme()).toBe('light');

    theme.toggle();
    expect(theme.theme()).toBe('dark');
    expect(attr()).toBe('dark');
    expect(windowThemes(calls)).toEqual(['light', 'dark']);
    expect(JSON.parse(localStorage.getItem(DISPLAY_PREFS_KEY)!).theme).toBe('dark');

    theme.set('system');
    expect(attr()).toBeUndefined();
    expect(theme.theme()).toBe('dark');
    expect(windowThemes(calls)).toEqual(['light', 'dark', null]);
  });

  it('au lancement : le thème choisi la fois précédente est appliqué tout de suite', () => {
    const calls = mockBackend();
    TestBed.inject(DisplayStore).setTheme('light');
    TestBed.resetTestingModule();

    const { theme } = setup(true);
    expect(theme.theme()).toBe('light');
    expect(attr()).toBe('light');
    expect(windowThemes(calls)).toEqual(['light']);
  });

  it('hors de Tauri, ou sans matchMedia : la page change quand même de thème', async () => {
    mockBackend({
      'plugin:window|set_theme': () => {
        throw 'permission refusée';
      },
    });
    TestBed.configureTestingModule({ providers: [{ provide: DARK_SCHEME_QUERY, useValue: null }] });
    const theme = TestBed.inject(ThemeService);
    expect(theme.theme()).toBe('light');

    theme.toggle();
    await Promise.resolve();
    expect(theme.theme()).toBe('dark');
    expect(attr()).toBe('dark');
  });
});
