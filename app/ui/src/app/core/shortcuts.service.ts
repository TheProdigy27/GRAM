import { Dialog } from '@angular/cdk/dialog';
import { DOCUMENT } from '@angular/common';
import { DestroyRef, Injectable, inject } from '@angular/core';

/** Un raccourci clavier (table « Raccourcis » de plan-refonte-ui.md). */
export interface Shortcut {
  /** `KeyboardEvent.key` : « r », « Escape »… (insensible à la casse pour les lettres). */
  key: string;
  /** Avec Ctrl (⌘ sur Mac). Sans : la touche seule, sans aucun modificateur. */
  mod?: boolean;
  /** Actif aussi dans un champ de saisie (Ctrl K, Échap). */
  inInputs?: boolean;
  /** Action ; renvoyer `false` laisse passer la touche (rien à faire dans cet état). */
  run: (event: KeyboardEvent) => boolean | void;
}

/** Mac : ⌘ au lieu de Ctrl. */
function detectMac(doc: Document): boolean {
  const nav = doc.defaultView?.navigator;
  return /Mac|iPhone|iPad/.test(nav?.platform || nav?.userAgent || '');
}

/** Cible où une touche seule s'écrit : champ, zone de texte, liste, contenu éditable. */
function isEditable(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  if (target.closest('textarea, select, [contenteditable=""], [contenteditable="true"]')) {
    return true;
  }
  const input = target.closest('input');
  return !!input && !['checkbox', 'radio', 'button', 'submit', 'reset'].includes(input.type);
}

/**
 * Raccourcis globaux de la fenêtre, enregistrés par les écrans qui les portent :
 *
 * ```ts
 * inject(DestroyRef).onDestroy(inject(ShortcutsService).register([{ key: 'r', run: … }]));
 * ```
 *
 * Ignorés quand un dialogue est ouvert (il gère ses touches) ou que la touche part d'une
 * superposition (menu, liste) ; les touches seules sont ignorées dans un champ de saisie. Une
 * touche déjà traitée (`preventDefault`, par exemple Échap qui vide la recherche) est ignorée.
 */
@Injectable({ providedIn: 'root' })
export class ShortcutsService {
  private readonly document = inject(DOCUMENT);
  private readonly dialog = inject(Dialog);
  private readonly shortcuts: Shortcut[] = [];

  readonly isMac = detectMac(this.document);
  /** « Ctrl » ou « ⌘ ». */
  readonly modLabel = this.isMac ? '⌘' : 'Ctrl';

  constructor() {
    const listener = (event: KeyboardEvent) => this.handle(event);
    this.document.addEventListener('keydown', listener);
    inject(DestroyRef).onDestroy(() => this.document.removeEventListener('keydown', listener));
  }

  /** Libellé d'un raccourci avec Ctrl : « Ctrl K », « ⌘K ». */
  modShortcut(key: string): string {
    return this.isMac ? `⌘${key}` : `Ctrl ${key}`;
  }

  /** Enregistre des raccourcis ; renvoie la fonction qui les retire. */
  register(shortcuts: Shortcut[]): () => void {
    this.shortcuts.push(...shortcuts);
    return () => {
      for (const s of shortcuts) {
        const i = this.shortcuts.indexOf(s);
        if (i >= 0) this.shortcuts.splice(i, 1);
      }
    };
  }

  private handle(event: KeyboardEvent): void {
    if (event.defaultPrevented || event.isComposing) return;
    if (this.dialog.openDialogs.length > 0) return;
    const target = event.target;
    if (target instanceof Element && target.closest('.cdk-overlay-container')) return;

    const mod = this.isMac ? event.metaKey : event.ctrlKey;
    const other = this.isMac ? event.ctrlKey : event.metaKey;
    if (other || event.altKey) return;
    const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
    const editable = isEditable(target);

    // Les derniers enregistrés passent d'abord (un écran ouvert après la coque).
    for (let i = this.shortcuts.length - 1; i >= 0; i--) {
      const s = this.shortcuts[i];
      const sKey = s.key.length === 1 ? s.key.toLowerCase() : s.key;
      // Maj n'est pas regardée : « R » vaut « r », et les chiffres de l'AZERTY la demandent.
      if (sKey !== key || !!s.mod !== mod) continue;
      if (editable && !s.inInputs) continue;
      if (!s.mod && event.repeat) {
        event.preventDefault();
        return;
      }
      if (s.run(event) === false) continue;
      event.preventDefault();
      return;
    }
  }
}
