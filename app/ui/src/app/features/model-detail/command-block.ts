import { Component, DestroyRef, inject, input, signal } from '@angular/core';
import { TauriService, errorMessage } from '../../core/tauri.service';
import { Icon } from '../../shared/ui/icon/icon';
import { ToastService } from '../../shared/ui/toast/toast';
import { Tooltip } from '../../shared/ui/tooltip/tooltip';

/** Durée du ✓ après une copie. */
const COPIED_MS = 2000;

/**
 * Commande à copier, en police à chasse fixe sur fond sombre (dans les deux thèmes). Le bouton
 * copier passe à ✓ pendant deux secondes, et un toast confirme la copie.
 */
@Component({
  selector: 'app-command-block',
  imports: [Icon, Tooltip],
  template: `
    <span class="prompt" aria-hidden="true">›</span>
    <code class="code" tabindex="0" [attr.data-testid]="testId()">{{ command() }}</code>
    <button
      type="button"
      class="copy"
      data-testid="copy-command"
      [class.copy--done]="copied()"
      [attr.aria-label]="'Copier : ' + command()"
      [gTooltip]="copied() ? 'Copiée' : 'Copier'"
      (click)="copy()"
    >
      <g-icon [name]="copied() ? 'check' : 'copy'" [size]="15" />
    </button>
  `,
  styles: `
    :host {
      display: flex;
      align-items: center;
      gap: 8px;
      min-width: 0;
      padding: 4px 4px 4px 12px;
      border: 1px solid var(--g-code-line);
      border-radius: var(--g-radius);
      background: var(--g-code-bg);
      color: var(--g-code-fg);
    }
    .prompt {
      flex: none;
      font-family: var(--g-font-mono);
      color: var(--g-code-muted);
    }
    // Une commande longue défile plutôt que d'être coupée.
    .code {
      flex: 1;
      min-width: 0;
      padding: 6px 0;
      overflow-x: auto;
      font-size: 12.5px;
      white-space: nowrap;
      scrollbar-width: thin;
      scrollbar-color: var(--g-code-muted) transparent;
      border-radius: 4px;
    }
    .code:focus-visible {
      outline-offset: 0;
    }
    .copy {
      flex: none;
      display: grid;
      place-items: center;
      width: 32px;
      height: 32px;
      border: 0;
      border-radius: 6px;
      background: transparent;
      color: var(--g-code-muted);
      cursor: pointer;
    }
    .copy:hover {
      background: var(--g-code-hover);
      color: var(--g-code-fg);
    }
    .copy--done {
      color: var(--g-code-fg);
    }
  `,
})
export class CommandBlock {
  readonly command = input.required<string>();
  /** `data-testid` du texte de la commande. */
  readonly testId = input('command');

  private readonly tauri = inject(TauriService);
  private readonly toast = inject(ToastService);
  protected readonly copied = signal(false);
  private timer: ReturnType<typeof setTimeout> | null = null;

  constructor() {
    inject(DestroyRef).onDestroy(() => this.clearTimer());
  }

  protected async copy(): Promise<void> {
    try {
      await this.tauri.copyText(this.command());
      this.toast.show('Commande copiée', { kind: 'success' });
      this.clearTimer();
      this.copied.set(true);
      this.timer = setTimeout(() => this.copied.set(false), COPIED_MS);
    } catch (e) {
      this.toast.show(`Copie impossible : ${errorMessage(e)}`, { kind: 'error' });
    }
  }

  private clearTimer(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
}
