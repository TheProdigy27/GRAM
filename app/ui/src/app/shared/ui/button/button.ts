import { Component, booleanAttribute, input } from '@angular/core';
import { Spinner } from '../spinner/spinner';

export type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger';
export type ButtonSize = 'md' | 'sm';

/**
 * Bouton natif stylé : `<button gButton variant="primary">`.
 * Tailles 34 px (`md`) et 30 px (`sm`) ; `icon` le rend carré (bouton-icône : `aria-label`
 * obligatoire) ; `busy` affiche un indicateur, pose `aria-busy` et bloque les clics.
 */
@Component({
  selector: 'button[gButton]',
  imports: [Spinner],
  template: `@if (busy()) {
      <g-spinner [size]="14" />
    }
    <ng-content />`,
  host: {
    class: 'g-button',
    '[attr.data-variant]': 'variant()',
    '[attr.data-size]': 'size()',
    '[class.g-button--icon]': 'icon()',
    '[attr.aria-busy]': 'busy() || null',
    '[disabled]': 'disabled() || busy()',
  },
  styles: `
    :host {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 8px;
      height: 34px;
      padding: 0 12px;
      border-radius: var(--g-radius);
      border: 1px solid transparent;
      background: transparent;
      color: var(--g-fg-2);
      font: inherit;
      font-size: 13px;
      font-weight: 500;
      line-height: 1;
      white-space: nowrap;
      cursor: pointer;
      user-select: none;
      transition:
        background-color 0.12s,
        border-color 0.12s,
        color 0.12s;
    }
    :host([data-size='sm']) {
      height: 30px;
      padding: 0 10px;
      gap: 6px;
      font-size: 12.5px;
    }
    :host(.g-button--icon) {
      width: 34px;
      padding: 0;
    }
    :host(.g-button--icon[data-size='sm']) {
      width: 30px;
    }
    :host([data-variant='primary']) {
      background: var(--g-accent);
      color: var(--g-on-accent);
      font-weight: 600;
    }
    :host([data-variant='primary']:hover:not(:disabled)) {
      background: var(--g-accent-hover);
    }
    :host([data-variant='secondary']) {
      border-color: var(--g-input);
      background: var(--g-surface);
    }
    :host([data-variant='secondary']:hover:not(:disabled)),
    :host([data-variant='ghost']:hover:not(:disabled)) {
      background: var(--g-hover);
    }
    :host([data-variant='danger']) {
      background: var(--g-error-bg);
      color: var(--g-error);
    }
    :host([data-variant='danger']:hover:not(:disabled)) {
      background-image: linear-gradient(var(--g-hover-overlay), var(--g-hover-overlay));
    }
    :host(:focus-visible) {
      outline: 2px solid var(--g-accent);
      outline-offset: 2px;
    }
    :host(:disabled) {
      cursor: not-allowed;
    }
    :host(:disabled:not([aria-busy])) {
      border-color: transparent;
      background: var(--g-disabled-bg);
      color: var(--g-disabled-fg);
    }
    :host(:disabled:not([aria-busy])[data-variant='ghost']) {
      background: transparent;
    }
    :host([aria-busy]) {
      cursor: progress;
    }
  `,
})
export class Button {
  readonly variant = input<ButtonVariant>('secondary');
  readonly size = input<ButtonSize>('md');
  /** Bouton carré ne contenant qu'une icône. */
  readonly icon = input(false, { transform: booleanAttribute });
  readonly busy = input(false, { transform: booleanAttribute });
  readonly disabled = input(false, { transform: booleanAttribute });
}
