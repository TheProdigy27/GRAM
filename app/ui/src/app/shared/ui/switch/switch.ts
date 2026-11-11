import { Component, booleanAttribute, input, model } from '@angular/core';

/**
 * Interrupteur : `<button role="switch" aria-checked>`, libellé projeté dans le bouton (un clic
 * sur le texte bascule aussi). Sans texte visible, donner `ariaLabel`.
 */
@Component({
  selector: 'g-switch',
  template: `
    <button
      type="button"
      role="switch"
      class="g-switch__button"
      [attr.aria-checked]="checked()"
      [attr.aria-label]="ariaLabel() || null"
      [disabled]="disabled()"
      (click)="toggle()"
    >
      <span class="g-switch__track" aria-hidden="true"><span class="g-switch__thumb"></span></span>
      <span class="g-switch__label"><ng-content /></span>
    </button>
  `,
  host: { class: 'g-switch' },
  styles: `
    :host {
      display: inline-flex;
    }
    .g-switch__button {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      min-height: 32px;
      padding: 0;
      border: 0;
      background: none;
      color: var(--g-fg-2);
      font: inherit;
      font-size: 13px;
      text-align: left;
      cursor: pointer;
    }
    .g-switch__button:focus-visible {
      outline: none;
    }
    .g-switch__button:focus-visible .g-switch__track {
      outline: 2px solid var(--g-accent);
      outline-offset: 2px;
    }
    .g-switch__track {
      position: relative;
      flex: none;
      width: 34px;
      height: 20px;
      border-radius: 10px;
      background: var(--g-control);
      transition: background-color 0.15s;
    }
    .g-switch__thumb {
      position: absolute;
      top: 2px;
      left: 2px;
      width: 16px;
      height: 16px;
      border-radius: 50%;
      background: #ffffff;
      box-shadow: 0 1px 2px rgba(16, 24, 40, 0.2);
      transition: transform 0.15s;
    }
    .g-switch__button[aria-checked='true'] .g-switch__track {
      background: var(--g-accent);
    }
    .g-switch__button[aria-checked='true'] .g-switch__thumb {
      transform: translateX(14px);
    }
    .g-switch__button:disabled {
      color: var(--g-disabled-fg);
      cursor: not-allowed;
    }
    .g-switch__button:disabled .g-switch__track {
      opacity: 0.5;
    }
    .g-switch__label:empty {
      display: none;
    }
    @media (prefers-reduced-motion: reduce) {
      .g-switch__track,
      .g-switch__thumb {
        transition: none;
      }
    }
  `,
})
export class Switch {
  readonly checked = model(false);
  readonly ariaLabel = input('');
  readonly disabled = input(false, { transform: booleanAttribute });

  protected toggle(): void {
    this.checked.update((c) => !c);
  }
}
