import { CdkListbox, CdkOption } from '@angular/cdk/listbox';
import {
  CdkConnectedOverlay,
  CdkOverlayOrigin,
  type ConnectedPosition,
} from '@angular/cdk/overlay';
import {
  Component,
  ElementRef,
  booleanAttribute,
  computed,
  input,
  model,
  signal,
  viewChild,
} from '@angular/core';
import { Icon } from '../icon/icon';

export interface SelectOption<T> {
  value: T;
  label: string;
  /** Précision à droite (« non installé »). */
  hint?: string;
  disabled?: boolean;
}

let nextId = 0;

const POSITIONS: ConnectedPosition[] = [
  { originX: 'start', originY: 'bottom', overlayX: 'start', overlayY: 'top', offsetY: 4 },
  { originX: 'start', originY: 'top', overlayX: 'start', overlayY: 'bottom', offsetY: -4 },
];

/**
 * Liste déroulante pour plus de 5 choix (en dessous, préférer `g-toggle-group`) :
 * bouton déclencheur + liste `CdkListbox` dans une superposition CDK.
 * Clavier : flèches, Entrée, Espace, Début / Fin, recherche par lettre, Échap.
 */
@Component({
  selector: 'g-select',
  imports: [CdkConnectedOverlay, CdkOverlayOrigin, CdkListbox, CdkOption, Icon],
  template: `
    @if (label()) {
      <span class="g-select__label" [id]="labelId">{{ label() }}</span>
    }
    <button
      #trigger
      type="button"
      class="g-select__trigger"
      cdkOverlayOrigin
      #origin="cdkOverlayOrigin"
      aria-haspopup="listbox"
      [attr.aria-expanded]="open()"
      [attr.aria-controls]="open() ? listId : null"
      [attr.aria-labelledby]="label() ? labelId + ' ' + valueId : null"
      [attr.aria-label]="label() ? null : ariaLabel() || null"
      [disabled]="disabled()"
      (click)="toggle()"
      (keydown)="onTriggerKeydown($event)"
    >
      <span class="g-select__value" [class.g-select__value--empty]="!selected()" [id]="valueId">
        {{ selected()?.label ?? placeholder() }}
      </span>
      <g-icon name="chevron-down" />
    </button>
    <ng-template
      cdkConnectedOverlay
      [cdkConnectedOverlayOrigin]="origin"
      [cdkConnectedOverlayOpen]="open()"
      [cdkConnectedOverlayPositions]="positions"
      [cdkConnectedOverlayMinWidth]="panelWidth()"
      (overlayOutsideClick)="onOutsideClick($event)"
      (overlayKeydown)="onOverlayKeydown($event)"
      (detach)="open.set(false)"
      (attach)="focusList()"
    >
      <ul
        #list
        class="g-select__panel"
        cdkListbox
        [id]="listId"
        [attr.aria-labelledby]="label() ? labelId : null"
        [attr.aria-label]="label() ? null : ariaLabel() || null"
        [cdkListboxValue]="listValue()"
        (cdkListboxValueChange)="pick($event.value[0])"
        (keydown)="onListKeydown($event)"
      >
        @for (o of options(); track o.value) {
          <li
            class="g-select__option"
            [cdkOption]="o.value"
            [cdkOptionDisabled]="!!o.disabled"
            [cdkOptionTypeaheadLabel]="o.label"
            (click)="closeSoon()"
          >
            <span class="g-select__option-label">{{ o.label }}</span>
            @if (o.hint) {
              <span class="g-select__hint">{{ o.hint }}</span>
            }
            <g-icon class="g-select__check" name="check" />
          </li>
        }
      </ul>
    </ng-template>
  `,
  host: { class: 'g-select' },
  styles: `
    :host {
      display: inline-flex;
      flex-direction: column;
      gap: 6px;
      min-width: 0;
    }
    .g-select__label {
      font-size: 12.5px;
      font-weight: 500;
      color: var(--g-fg-2);
    }
    .g-select__trigger {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
      width: 100%;
      height: 36px;
      padding: 0 10px;
      border-radius: var(--g-radius);
      border: 1px solid var(--g-input);
      background: var(--g-surface);
      color: var(--g-fg);
      font: inherit;
      font-size: 13px;
      text-align: left;
      cursor: pointer;
    }
    .g-select__trigger g-icon {
      color: var(--g-muted);
    }
    .g-select__trigger[aria-expanded='true'],
    .g-select__trigger:focus-visible {
      border-color: var(--g-accent);
      box-shadow: 0 0 0 1px var(--g-accent);
      outline: none;
    }
    .g-select__trigger:disabled {
      background: var(--g-surface-2);
      color: var(--g-muted);
      cursor: not-allowed;
    }
    .g-select__value {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .g-select__value--empty {
      color: var(--g-muted);
    }
    .g-select__panel {
      margin: 0;
      padding: 4px;
      list-style: none;
      max-height: 320px;
      overflow-y: auto;
      border-radius: 10px;
      border: 1px solid var(--g-line);
      background: var(--g-surface);
      box-shadow: var(--g-shadow-pop);
      font-family: var(--g-font-sans);
      outline: none;
    }
    .g-select__option {
      display: flex;
      align-items: center;
      gap: 8px;
      min-height: 34px;
      padding: 0 10px;
      border-radius: 6px;
      color: var(--g-fg-2);
      font-size: 13px;
      cursor: pointer;
    }
    .g-select__option:hover,
    .g-select__option:focus {
      background: var(--g-hover);
      color: var(--g-fg);
      outline: none;
    }
    .g-select__option:focus-visible {
      outline: 2px solid var(--g-accent);
      outline-offset: -2px;
    }
    .g-select__option[aria-selected='true'] {
      background: var(--g-accent-weak);
      color: var(--g-fg);
      font-weight: 600;
    }
    .g-select__option[aria-disabled='true'] {
      color: var(--g-disabled-fg);
      cursor: not-allowed;
    }
    .g-select__option-label {
      flex: 1;
    }
    .g-select__hint {
      font-size: 11.5px;
      font-weight: 400;
      color: var(--g-muted);
    }
    .g-select__check {
      visibility: hidden;
      color: var(--g-accent);
    }
    .g-select__option[aria-selected='true'] .g-select__check {
      visibility: visible;
    }
  `,
})
export class Select<T> {
  readonly options = input.required<readonly SelectOption<T>[]>();
  readonly value = model<T | null>(null);
  readonly label = input('');
  /** Nom accessible quand il n'y a pas de libellé visible. */
  readonly ariaLabel = input('');
  readonly placeholder = input('Choisir…');
  readonly disabled = input(false, { transform: booleanAttribute });

  readonly open = signal(false);
  protected readonly positions = POSITIONS;
  protected readonly panelWidth = signal(0);
  private readonly baseId = `g-select-${nextId++}`;
  protected readonly labelId = `${this.baseId}-label`;
  protected readonly valueId = `${this.baseId}-value`;
  protected readonly listId = `${this.baseId}-list`;

  private readonly trigger = viewChild.required<ElementRef<HTMLButtonElement>>('trigger');
  private readonly list = viewChild(CdkListbox);

  protected readonly selected = computed(() =>
    this.options().find((o) => Object.is(o.value, this.value())),
  );
  protected readonly listValue = computed(() => (this.selected() ? [this.value()] : []));

  toggle(): void {
    if (this.open()) this.close();
    else this.show();
  }

  show(): void {
    if (this.disabled()) return;
    this.panelWidth.set(this.trigger().nativeElement.getBoundingClientRect().width);
    this.open.set(true);
  }

  /** Ferme la liste ; rend le focus au déclencheur sauf si l'utilisateur est parti ailleurs. */
  close(restoreFocus = true): void {
    if (!this.open()) return;
    this.open.set(false);
    if (restoreFocus) this.trigger().nativeElement.focus();
  }

  protected focusList(): void {
    this.list()?.focus();
  }

  protected pick(value: T | null | undefined): void {
    if (value !== undefined) this.value.set(value);
    this.close();
  }

  /** Clic sur l'option déjà choisie : la liste ne signale pas de changement, on ferme quand même. */
  protected closeSoon(): void {
    queueMicrotask(() => this.close());
  }

  protected onTriggerKeydown(event: KeyboardEvent): void {
    if ((event.key === 'ArrowDown' || event.key === 'ArrowUp') && !this.open()) {
      event.preventDefault();
      this.show();
    }
  }

  protected onListKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter' || event.key === ' ') this.closeSoon();
    else if (event.key === 'Tab') this.close(false);
  }

  protected onOverlayKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      this.close();
    }
  }

  protected onOutsideClick(event: MouseEvent): void {
    // Un clic sur le déclencheur passe déjà par toggle().
    if (!this.trigger().nativeElement.contains(event.target as Node)) this.close(false);
  }
}
