import { Component, booleanAttribute, input, model } from '@angular/core';
import { Icon } from '../icon/icon';

/** Case à cocher native stylée, libellé projeté. */
@Component({
  selector: 'g-checkbox',
  imports: [Icon],
  template: `
    <label class="g-checkbox__label">
      <span class="g-checkbox__box">
        <input
          type="checkbox"
          class="g-checkbox__input"
          [checked]="checked()"
          [disabled]="disabled()"
          [attr.aria-label]="ariaLabel() || null"
          (change)="onChange($event)"
        />
        <g-icon class="g-checkbox__check" name="check" [size]="12" [strokeWidth]="3" />
      </span>
      <span class="g-checkbox__text"><ng-content /></span>
    </label>
  `,
  host: { class: 'g-checkbox' },
  styles: `
    :host {
      display: inline-flex;
    }
    .g-checkbox__label {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      min-height: 32px;
      color: var(--g-fg-2);
      font-size: 13px;
      cursor: pointer;
    }
    .g-checkbox__box {
      position: relative;
      display: inline-grid;
      place-items: center;
      flex: none;
    }
    .g-checkbox__input {
      -webkit-appearance: none;
      appearance: none;
      width: 18px;
      height: 18px;
      margin: 0;
      border-radius: 5px;
      border: 1.5px solid var(--g-control);
      background: var(--g-surface);
      cursor: pointer;
    }
    .g-checkbox__input:checked {
      border-color: var(--g-accent);
      background: var(--g-accent);
    }
    .g-checkbox__input:focus-visible {
      outline: 2px solid var(--g-accent);
      outline-offset: 2px;
    }
    .g-checkbox__input:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
    .g-checkbox__check {
      position: absolute;
      color: var(--g-on-accent);
      pointer-events: none;
      visibility: hidden;
    }
    .g-checkbox__input:checked + .g-checkbox__check {
      visibility: visible;
    }
    .g-checkbox__text:empty {
      display: none;
    }
  `,
})
export class Checkbox {
  readonly checked = model(false);
  readonly ariaLabel = input('');
  readonly disabled = input(false, { transform: booleanAttribute });

  protected onChange(event: Event): void {
    this.checked.set((event.target as HTMLInputElement).checked);
  }
}
