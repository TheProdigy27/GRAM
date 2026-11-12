import { Injectable, InjectionToken, computed, inject, signal } from '@angular/core';

/** Préférences d'affichage, gardées d'un lancement à l'autre (stockage local de la WebView). */
export interface DisplayPrefs {
  /** Variantes regroupées sous leur modèle officiel. */
  groupVariants: boolean;
  /** Bandeau « Trois conseils » replié sur son titre. */
  picksCollapsed: boolean;
  /** Quantifications de la fiche : graphique des vitesses ou tableau. */
  quantView: QuantView;
}

export type QuantView = 'chart' | 'table';

const DEFAULTS: DisplayPrefs = { groupVariants: true, picksCollapsed: false, quantView: 'chart' };

/** Clé du stockage local. */
export const DISPLAY_PREFS_KEY = 'gram.display';

/** Stockage des préférences ; `null` : aucun (elles valent pour la session). */
export const DISPLAY_STORAGE = new InjectionToken<Storage | null>('DISPLAY_STORAGE', {
  factory: () => {
    try {
      return globalThis.localStorage ?? null;
    } catch {
      return null;
    }
  },
});

/**
 * Préférences d'affichage de l'interface (pas des paramètres de calcul : rien n'est envoyé au
 * backend). Un stockage illisible ou plein n'empêche rien : les préférences valent alors pour la
 * session.
 */
@Injectable({ providedIn: 'root' })
export class DisplayStore {
  private readonly storage = inject(DISPLAY_STORAGE);
  private readonly prefs = signal<DisplayPrefs>(this.read());

  readonly groupVariants = computed(() => this.prefs().groupVariants);
  readonly picksCollapsed = computed(() => this.prefs().picksCollapsed);
  readonly quantView = computed(() => this.prefs().quantView);

  setGroupVariants(groupVariants: boolean): void {
    this.update({ groupVariants });
  }

  setPicksCollapsed(picksCollapsed: boolean): void {
    this.update({ picksCollapsed });
  }

  setQuantView(quantView: QuantView): void {
    this.update({ quantView });
  }

  private update(patch: Partial<DisplayPrefs>): void {
    this.prefs.update((p) => ({ ...p, ...patch }));
    this.write(this.prefs());
  }

  private read(): DisplayPrefs {
    try {
      const raw = this.storage?.getItem(DISPLAY_PREFS_KEY);
      const saved = raw ? (JSON.parse(raw) as Partial<DisplayPrefs>) : {};
      return {
        groupVariants:
          typeof saved.groupVariants === 'boolean' ? saved.groupVariants : DEFAULTS.groupVariants,
        picksCollapsed:
          typeof saved.picksCollapsed === 'boolean'
            ? saved.picksCollapsed
            : DEFAULTS.picksCollapsed,
        quantView:
          saved.quantView === 'chart' || saved.quantView === 'table'
            ? saved.quantView
            : DEFAULTS.quantView,
      };
    } catch {
      return { ...DEFAULTS };
    }
  }

  private write(prefs: DisplayPrefs): void {
    try {
      this.storage?.setItem(DISPLAY_PREFS_KEY, JSON.stringify(prefs));
    } catch {
      // Stockage indisponible : les préférences valent pour la session.
    }
  }
}
