import { Component, DestroyRef, ElementRef, effect, inject, input } from '@angular/core';
import { ToastService } from '@gram/ui/toast/toast';
import { HarnessApi, errorMessage, isWebLink } from '../../core/harness-api.service';
import { enhance, renderMarkdown } from '../../shared/markdown';

/** Durée de « Copié » sur le bouton d'un bloc de code. */
const COPIED_MS = 2000;

/**
 * Réponse d'un modèle rendue en Markdown nettoyé (`shared/markdown.ts`), redessinée à chaque
 * changement du texte (au plus une fois par image pendant une réponse : voir `ChatStore`).
 * Les liens s'ouvrent dans le navigateur du système (http et https seulement), jamais dans la
 * fenêtre ; les blocs de code ont un bouton copier. Styles globaux : `styles/_markdown.scss`.
 */
@Component({
  selector: 'app-markdown',
  template: '',
  host: {
    class: 'md',
    '(click)': 'onClick($event)',
    '(auxclick)': 'onAuxClick($event)',
  },
})
export class MarkdownView {
  readonly text = input.required<string>();

  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
  private readonly api = inject(HarnessApi);
  private readonly toast = inject(ToastService);
  private readonly timers = new Set<ReturnType<typeof setTimeout>>();

  constructor() {
    effect(() => {
      this.host.innerHTML = renderMarkdown(this.text());
      enhance(this.host);
    });
    inject(DestroyRef).onDestroy(() => this.timers.forEach(clearTimeout));
  }

  protected onClick(event: MouseEvent): void {
    const target = event.target instanceof Element ? event.target : null;
    const copy = target?.closest<HTMLButtonElement>('[data-copy-code]');
    if (copy) {
      void this.copyCode(copy);
      return;
    }
    const link = target?.closest('a');
    if (!link) return;
    // Jamais de navigation dans la fenêtre.
    event.preventDefault();
    const href = link.getAttribute('href');
    if (isWebLink(href)) {
      this.api.openLink(href).catch((e: unknown) => {
        this.toast.show(`Lien non ouvert : ${errorMessage(e)}`, { kind: 'error' });
      });
    }
  }

  /** Clic du milieu : pas de nouvelle fenêtre. */
  protected onAuxClick(event: MouseEvent): void {
    if (event.target instanceof Element && event.target.closest('a')) event.preventDefault();
  }

  private async copyCode(button: HTMLButtonElement): Promise<void> {
    const code = button.closest('.md-code')?.querySelector('pre')?.textContent ?? '';
    try {
      await this.api.copyText(code.replace(/\n$/, ''));
      this.toast.show('Code copié', { kind: 'success' });
      button.textContent = 'Copié';
      const timer = setTimeout(() => {
        this.timers.delete(timer);
        button.textContent = 'Copier';
      }, COPIED_MS);
      this.timers.add(timer);
    } catch (e) {
      this.toast.show(`Copie impossible : ${errorMessage(e)}`, { kind: 'error' });
    }
  }
}
