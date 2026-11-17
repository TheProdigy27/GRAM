import { Injectable, InjectionToken, computed, inject, signal } from '@angular/core';
import { type ThemePref, type ThemePrefStore, isThemePref } from '@gram/theme';
import type { KeepAlive } from '@gram/types/KeepAlive';

/** Préférences de GRAM Harness, gardées d'un lancement à l'autre (stockage local de la WebView). */
export interface HarnessPrefs {
  /** Thème de l'interface, appliqué par `ThemeService`. */
  theme: ThemePref;
  /** Dernier modèle choisi (nom dans Ollama), repris au lancement s'il est toujours là. */
  model: string | null;
  /** Instructions des nouvelles conversations (paramètres) ; vides : aucune. */
  instructions: string;
  /** Garder le modèle en mémoire après chaque réponse (paramètres). */
  keepAlive: KeepAlive;
  /** Dernière position de la bascule « Réflexion », reprise par les nouvelles conversations. */
  think: boolean;
}

const DEFAULTS: HarnessPrefs = {
  theme: 'system',
  model: null,
  instructions: '',
  keepAlive: 'ollama',
  think: true,
};

const KEEP_ALIVES: readonly KeepAlive[] = ['ollama', 'halfHour', 'forever'];

/** Clé du stockage local. */
export const PREFS_KEY = 'gram-harness.prefs';

/** Stockage des préférences ; `null` : aucun (elles valent pour la session). */
export const PREFS_STORAGE = new InjectionToken<Storage | null>('PREFS_STORAGE', {
  factory: () => {
    try {
      return globalThis.localStorage ?? null;
    } catch {
      return null;
    }
  },
});

/**
 * Préférences de l'interface. Un stockage illisible ou plein n'empêche rien : elles valent
 * alors pour la session.
 */
@Injectable({ providedIn: 'root' })
export class PrefsStore implements ThemePrefStore {
  private readonly storage = inject(PREFS_STORAGE);
  private readonly prefs = signal<HarnessPrefs>(this.read());

  readonly theme = computed(() => this.prefs().theme);
  readonly model = computed(() => this.prefs().model);
  readonly instructions = computed(() => this.prefs().instructions);
  readonly keepAlive = computed(() => this.prefs().keepAlive);
  readonly think = computed(() => this.prefs().think);

  setTheme(theme: ThemePref): void {
    this.update({ theme });
  }

  setModel(model: string): void {
    this.update({ model });
  }

  setInstructions(instructions: string): void {
    this.update({ instructions });
  }

  setKeepAlive(keepAlive: KeepAlive): void {
    this.update({ keepAlive });
  }

  setThink(think: boolean): void {
    this.update({ think });
  }

  private update(patch: Partial<HarnessPrefs>): void {
    this.prefs.update((p) => ({ ...p, ...patch }));
    try {
      this.storage?.setItem(PREFS_KEY, JSON.stringify(this.prefs()));
    } catch {
      // Stockage indisponible : les préférences valent pour la session.
    }
  }

  private read(): HarnessPrefs {
    try {
      const raw = this.storage?.getItem(PREFS_KEY);
      const saved = raw ? (JSON.parse(raw) as Partial<HarnessPrefs>) : {};
      return {
        theme: isThemePref(saved.theme) ? saved.theme : DEFAULTS.theme,
        model: typeof saved.model === 'string' && saved.model ? saved.model : DEFAULTS.model,
        instructions:
          typeof saved.instructions === 'string' ? saved.instructions : DEFAULTS.instructions,
        keepAlive: KEEP_ALIVES.includes(saved.keepAlive as KeepAlive)
          ? (saved.keepAlive as KeepAlive)
          : DEFAULTS.keepAlive,
        think: typeof saved.think === 'boolean' ? saved.think : DEFAULTS.think,
      };
    } catch {
      return { ...DEFAULTS };
    }
  }
}
