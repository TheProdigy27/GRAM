import { DOCUMENT } from '@angular/common';
import { DestroyRef, Injectable, InjectionToken, computed, inject, signal } from '@angular/core';
import { DisplayStore, type ThemePref } from '../state/display.store';
import { TauriService } from './tauri.service';

export type Theme = 'light' | 'dark';

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

/**
 * Thème de l'interface : celui du système par défaut, ou clair ou sombre au choix (bouton de
 * l'en-tête, palette), gardé d'un lancement à l'autre. Les jetons de `styles.scss` suivent le
 * système, sauf si `data-theme` est posé sur `<html>`. La fenêtre suit aussi le choix : barre de
 * titre native, et `prefers-color-scheme` de la WebView, que Tauri aligne sur la fenêtre. Un
 * échec de ce côté (hors de Tauri) n'empêche rien.
 */
@Injectable({ providedIn: 'root' })
export class ThemeService {
  private readonly display = inject(DisplayStore);
  private readonly tauri = inject(TauriService);
  private readonly root = inject(DOCUMENT).documentElement;
  private readonly query = inject(DARK_SCHEME_QUERY);

  /**
   * Système en sombre. Sous Tauri, la requête suit la fenêtre : elle ne dit le thème du système
   * que lorsque la préférence est `system`, le seul cas où elle sert.
   */
  private readonly systemDark = signal(this.query?.matches ?? false);

  readonly preference = this.display.theme;
  /** Thème affiché. */
  readonly theme = computed<Theme>(() => {
    const pref = this.display.theme();
    if (pref !== 'system') return pref;
    return this.systemDark() ? 'dark' : 'light';
  });

  constructor() {
    const query = this.query;
    if (query) {
      const onChange = (e: MediaQueryListEvent) => this.systemDark.set(e.matches);
      query.addEventListener('change', onChange);
      inject(DestroyRef).onDestroy(() => query.removeEventListener('change', onChange));
    }
    // Avant le premier rendu (le service est créé avec la coque).
    const pref = this.display.theme();
    if (pref !== 'system') this.apply(pref);
  }

  /** Bouton de l'en-tête : passe au thème opposé à celui affiché. */
  toggle(): void {
    this.set(this.theme() === 'dark' ? 'light' : 'dark');
  }

  set(pref: ThemePref): void {
    if (pref === this.display.theme()) return;
    this.display.setTheme(pref);
    this.apply(pref);
  }

  private apply(pref: ThemePref): void {
    if (pref === 'system') delete this.root.dataset['theme'];
    else this.root.dataset['theme'] = pref;
    this.tauri.setWindowTheme(pref === 'system' ? null : pref).catch(() => undefined);
  }
}
