import { DOCUMENT } from '@angular/common';
import { DestroyRef, Injectable, InjectionToken, inject } from '@angular/core';

/** Délai après le dernier défilement avant que le pouce ne s'efface (en fondu, voir styles.scss). */
export const SCROLLBAR_HIDE_DELAY_MS = new InjectionToken<number>('SCROLLBAR_HIDE_DELAY_MS', {
  factory: () => 900,
});

/**
 * Barres de défilement visibles pendant le défilement seulement. Un seul écouteur, en phase de
 * capture (l'événement `scroll` ne remonte pas), pose `g-scrollbar` et `g-scrolling` sur
 * l'élément qui défile, puis retire `g-scrolling` peu après le dernier défilement. Les styles
 * sont dans `styles.scss`. Créé au démarrage (app.config).
 */
@Injectable({ providedIn: 'root' })
export class ScrollbarsService {
  constructor() {
    const doc = inject(DOCUMENT);
    const delay = inject(SCROLLBAR_HIDE_DELAY_MS);
    const timers = new Map<Element, ReturnType<typeof setTimeout>>();

    const onScroll = (event: Event) => {
      // La page elle-même : l'événement vise le document, la barre est celle de <html>.
      const el = event.target === doc ? doc.documentElement : (event.target as Element | null);
      if (!el?.classList) return;
      el.classList.add('g-scrollbar', 'g-scrolling');
      clearTimeout(timers.get(el));
      timers.set(
        el,
        setTimeout(() => {
          timers.delete(el);
          el.classList.remove('g-scrolling');
        }, delay),
      );
    };

    doc.addEventListener('scroll', onScroll, { capture: true, passive: true });
    inject(DestroyRef).onDestroy(() => {
      doc.removeEventListener('scroll', onScroll, { capture: true });
      timers.forEach((t) => clearTimeout(t));
    });
  }
}
