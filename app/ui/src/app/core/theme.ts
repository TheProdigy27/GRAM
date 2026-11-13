import { DOCUMENT } from '@angular/common';
import { DestroyRef, InjectionToken, type Signal, computed, inject, signal } from '@angular/core';

/** Thème affiché. */
export type Theme = 'light' | 'dark';

/** Thème choisi : celui du système, ou clair ou sombre quel que soit le système. */
export type ThemePref = 'system' | 'light' | 'dark';

export function isThemePref(value: unknown): value is ThemePref {
  return value === 'system' || value === 'light' || value === 'dark';
}

/** Requête « le système est en sombre » ; `null` sans `matchMedia` (jsdom). */
export const DARK_SCHEME_QUERY = new InjectionToken<MediaQueryList | null>('DARK_SCHEME_QUERY', {
  factory: () => {
    try {
      return globalThis.matchMedia?.('(prefers-color-scheme: dark)') ?? null;
    } catch {
      return null;
    }
  },
});

/** Où une application garde le thème choisi, d'un lancement à l'autre. */
export interface ThemePrefStore {
  readonly theme: Signal<ThemePref>;
  setTheme(pref: ThemePref): void;
}

/** Règle le thème de la fenêtre ; `null` : celui du système. */
export type SetWindowTheme = (theme: Theme | null) => Promise<void>;

/**
 * Thème de l'interface, partagé par GRAM et GRAM Harness (chacun le dérive avec son stockage et
 * son accès à la fenêtre) : celui du système par défaut, ou clair ou sombre au choix. Les jetons
 * de `styles/_tokens.scss` suivent le système, sauf si `data-theme` est posé sur `<html>`. La
 * fenêtre suit aussi le choix : barre de titre native, et `prefers-color-scheme` de la WebView,
 * que Tauri aligne sur la fenêtre. Un échec de ce côté (hors de Tauri) n'empêche rien.
 */
export abstract class ThemeController {
  private readonly root = inject(DOCUMENT).documentElement;
  private readonly query = inject(DARK_SCHEME_QUERY);

  /**
   * Système en sombre. Sous Tauri, la requête suit la fenêtre : elle ne dit le thème du système
   * que lorsque la préférence est `system`, le seul cas où elle sert.
   */
  private readonly systemDark = signal(this.query?.matches ?? false);

  readonly preference: Signal<ThemePref>;
  /** Thème affiché. */
  readonly theme: Signal<Theme>;

  protected constructor(
    private readonly prefs: ThemePrefStore,
    private readonly setWindowTheme: SetWindowTheme,
  ) {
    this.preference = prefs.theme;
    this.theme = computed<Theme>(() => {
      const pref = prefs.theme();
      if (pref !== 'system') return pref;
      return this.systemDark() ? 'dark' : 'light';
    });
    const query = this.query;
    if (query) {
      const onChange = (e: MediaQueryListEvent) => this.systemDark.set(e.matches);
      query.addEventListener('change', onChange);
      inject(DestroyRef).onDestroy(() => query.removeEventListener('change', onChange));
    }
    // Avant le premier rendu (le service est créé avec la coque).
    const pref = prefs.theme();
    if (pref !== 'system') this.apply(pref);
  }

  /** Bouton de l'en-tête : passe au thème opposé à celui affiché. */
  toggle(): void {
    this.set(this.theme() === 'dark' ? 'light' : 'dark');
  }

  set(pref: ThemePref): void {
    if (pref === this.prefs.theme()) return;
    this.prefs.setTheme(pref);
    this.apply(pref);
  }

  private apply(pref: ThemePref): void {
    if (pref === 'system') delete this.root.dataset['theme'];
    else this.root.dataset['theme'] = pref;
    this.setWindowTheme(pref === 'system' ? null : pref).catch(() => undefined);
  }
}
