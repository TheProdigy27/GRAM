import {
  Component,
  Directive,
  ViewEncapsulation,
  ElementRef,
  booleanAttribute,
  computed,
  contentChild,
  inject,
  input,
  linkedSignal,
  output,
} from '@angular/core';
import { Icon } from '../icon/icon';

let nextId = 0;

/**
 * Champ de saisie : libellé, préfixe (`[gPrefix]`), suffixe (`suffix="Go"` ou `[gSuffix]`),
 * bouton effacer, aide et erreur, autour d'un `<input gInput>` natif.
 *
 * ```html
 * <g-field label="Taille maximale" suffix="Go" hint="Vide : aucune limite" clearable>
 *   <input gInput inputmode="decimal" [value]="max()" (input)="setMax($event)" />
 * </g-field>
 * ```
 */
@Component({
  selector: 'g-field',
  imports: [Icon],
  template: `
    @if (label()) {
      <label class="g-field__label" [attr.for]="control()?.id()">{{ label() }}</label>
    }
    <div
      class="g-field__box"
      [class.g-field__box--invalid]="!!error()"
      [class.g-field__box--disabled]="control()?.disabled()"
      (click)="focusControl($event)"
    >
      <ng-content select="[gPrefix]" />
      <ng-content />
      @if (suffix()) {
        <span class="g-field__suffix">{{ suffix() }}</span>
      }
      <ng-content select="[gSuffix]" />
      @if (clearable() && control()?.hasValue() && !control()?.disabled()) {
        <button
          type="button"
          class="g-field__clear"
          data-testid="field-clear"
          [attr.aria-label]="clearLabel()"
          (click)="clear()"
        >
          <g-icon name="close" [size]="14" />
        </button>
      }
    </div>
    @if (error()) {
      <div class="g-field__error" [id]="errorId">
        <g-icon name="error" [size]="14" />{{ error() }}
      </div>
    } @else if (hint()) {
      <div class="g-field__hint" [id]="hintId">{{ hint() }}</div>
    }
  `,
  host: { class: 'g-field' },
  // Sans encapsulation : l'<input> projeté appartient au gabarit parent. Tout est préfixé.
  encapsulation: ViewEncapsulation.None,
  styles: `
    .g-field {
      display: flex;
      flex-direction: column;
      gap: 6px;
      min-width: 0;
    }
    .g-field__label {
      font-size: 12.5px;
      font-weight: 500;
      color: var(--g-fg-2);
    }
    .g-field__box {
      display: flex;
      align-items: center;
      gap: 8px;
      height: 36px;
      padding: 0 10px;
      border-radius: var(--g-radius);
      border: 1px solid var(--g-input);
      background: var(--g-surface);
      color: var(--g-muted);
      cursor: text;
    }
    .g-field__box:focus-within {
      border-color: var(--g-accent);
      box-shadow: 0 0 0 1px var(--g-accent);
    }
    .g-field__box--invalid,
    .g-field__box--invalid:focus-within {
      border-color: var(--g-error);
      box-shadow: 0 0 0 1px var(--g-error);
    }
    .g-field__box--disabled {
      background: var(--g-surface-2);
      cursor: not-allowed;
    }
    .g-field .g-input {
      flex: 1;
      min-width: 0;
      height: 100%;
      padding: 0;
      border: 0;
      outline: none;
      background: transparent;
      color: var(--g-fg);
      font: inherit;
      font-size: 13px;
    }
    .g-field .g-input::placeholder {
      color: var(--g-muted);
      opacity: 1;
    }
    .g-field .g-input:disabled {
      cursor: not-allowed;
      color: var(--g-muted);
    }
    .g-field__suffix {
      font-size: 12.5px;
      white-space: nowrap;
    }
    .g-field__clear {
      display: inline-grid;
      place-items: center;
      flex: none;
      width: 32px;
      height: 32px;
      margin-right: -8px;
      padding: 0;
      border: 0;
      border-radius: 6px;
      background: transparent;
      color: var(--g-muted);
      cursor: pointer;
    }
    .g-field__clear:hover {
      background: var(--g-hover);
      color: var(--g-fg);
    }
    .g-field__clear:focus-visible {
      outline: 2px solid var(--g-accent);
      outline-offset: -2px;
    }
    .g-field__hint,
    .g-field__error {
      display: flex;
      align-items: center;
      gap: 6px;
      font-size: 11.5px;
      line-height: 1.4;
      color: var(--g-muted);
    }
    .g-field__error {
      color: var(--g-error);
    }
  `,
})
export class Field {
  readonly label = input<string>('');
  readonly hint = input<string>('');
  /** Message d'erreur : le champ passe en `aria-invalid`. */
  readonly error = input<string | null | undefined>(null);
  readonly suffix = input<string>('');
  /** Bouton « Effacer » quand le champ n'est pas vide. */
  readonly clearable = input(false, { transform: booleanAttribute });
  readonly clearLabel = input('Effacer');
  readonly cleared = output<void>();

  protected readonly control = contentChild(FieldControl);
  private readonly baseId = `g-field-${nextId++}`;
  protected readonly hintId = `${this.baseId}-hint`;
  protected readonly errorId = `${this.baseId}-error`;

  /** Identifiant de l'aide ou de l'erreur affichée, pour `aria-describedby`. */
  readonly describedBy = computed(() =>
    this.error() ? this.errorId : this.hint() ? this.hintId : null,
  );

  protected clear(): void {
    this.control()?.clear();
    this.cleared.emit();
  }

  protected focusControl(event: MouseEvent): void {
    const target = event.target as HTMLElement;
    if (!target.closest('input, textarea, button')) this.control()?.focus();
  }
}

/** Le `<input>` (ou `<textarea>`) natif d'un `g-field`. */
@Directive({
  selector: 'input[gInput], textarea[gInput]',
  host: {
    class: 'g-input',
    '[id]': 'id()',
    '[value]': 'value() ?? ""',
    '[disabled]': 'disabled()',
    '[attr.aria-describedby]': 'field?.describedBy() ?? null',
    '[attr.aria-invalid]': 'field?.error() ? "true" : null',
    '(input)': 'current.set(element.value)',
  },
})
export class FieldControl {
  readonly id = input(`g-input-${nextId++}`);
  readonly value = input<string | number | null | undefined>(undefined);
  readonly disabled = input(false, { transform: booleanAttribute });

  protected readonly field = inject(Field, { optional: true });
  protected readonly element = inject<ElementRef<HTMLInputElement>>(ElementRef).nativeElement;
  /** Valeur affichée : celle liée par `[value]`, puis ce que l'utilisateur tape. */
  protected readonly current = linkedSignal(() => String(this.value() ?? ''));
  readonly hasValue = computed(() => this.current() !== '');

  focus(): void {
    this.element.focus();
  }

  /** Vide le champ et prévient les écouteurs `(input)`, comme une saisie. */
  clear(): void {
    this.element.value = '';
    this.element.dispatchEvent(new Event('input', { bubbles: true }));
    this.focus();
  }
}

export const FIELD = [Field, FieldControl] as const;
