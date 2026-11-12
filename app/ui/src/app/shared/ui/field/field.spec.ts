import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { clearField, queryTestId, typeInto } from '../../../../testing/dom';
import { decimal, parseNumber, sameNumber } from '../../format';
import { Icon } from '../icon/icon';
import { FIELD } from './field';

@Component({
  imports: [FIELD, Icon],
  template: `<g-field
    data-testid="search"
    label="Recherche"
    hint="Nom du modèle"
    suffix="Go"
    clearable
    [error]="error()"
    (cleared)="clears = clears + 1"
  >
    <g-icon gPrefix name="search" />
    <input gInput [value]="value()" [disabled]="disabled()" (input)="onInput($event)" />
  </g-field>`,
})
class Host {
  readonly value = signal('');
  readonly error = signal<string | null>(null);
  readonly disabled = signal(false);
  clears = 0;
  onInput(event: Event): void {
    this.value.set((event.target as HTMLInputElement).value);
  }
}

describe('Field', () => {
  async function render() {
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    return { fixture, el, host: fixture.componentInstance, input: el.querySelector('input')! };
  }

  it('relie le libellé et l’aide au champ natif', async () => {
    const { el, input } = await render();
    const label = el.querySelector('label')!;
    expect(label.textContent?.trim()).toBe('Recherche');
    expect(label.htmlFor).toBe(input.id);
    expect(input.id).toMatch(/^g-input-/);
    const hint = el.querySelector('.g-field__hint')!;
    expect(hint.textContent?.trim()).toBe('Nom du modèle');
    expect(input.getAttribute('aria-describedby')).toBe(hint.id);
    expect(input.hasAttribute('aria-invalid')).toBe(false);
    expect(el.querySelector('.g-field__suffix')?.textContent?.trim()).toBe('Go');
    expect(el.querySelector('[gPrefix]')).not.toBeNull();
  });

  it('erreur : aria-invalid, message relié à la place de l’aide', async () => {
    const { fixture, el, host, input } = await render();
    host.error.set('Nombre attendu');
    await fixture.whenStable();
    const error = el.querySelector('.g-field__error')!;
    expect(error.textContent?.trim()).toBe('Nombre attendu');
    expect(input.getAttribute('aria-invalid')).toBe('true');
    expect(input.getAttribute('aria-describedby')).toBe(error.id);
    expect(el.querySelector('.g-field__hint')).toBeNull();
  });

  it('bouton Effacer : visible quand le champ a une valeur, vide le champ comme une saisie', async () => {
    const { fixture, el, host, input } = await render();
    expect(queryTestId('field-clear', el)).toBeNull();
    await typeInto(fixture, 'search', 'qwen3 8b');
    expect(host.value()).toBe('qwen3 8b');
    const clear = queryTestId('field-clear', el)!;
    expect(clear.getAttribute('aria-label')).toBe('Effacer');
    await clearField(fixture, 'search');
    expect(host.value()).toBe('');
    expect(input.value).toBe('');
    expect(host.clears).toBe(1);
    expect(document.activeElement).toBe(input);
    expect(queryTestId('field-clear', el)).toBeNull();
  });

  it('suit une valeur liée par le parent ; pas d’effacement si désactivé', async () => {
    const { fixture, el, host, input } = await render();
    host.value.set('llama');
    await fixture.whenStable();
    expect(input.value).toBe('llama');
    expect(queryTestId('field-clear', el)).not.toBeNull();
    host.disabled.set(true);
    await fixture.whenStable();
    expect(input.disabled).toBe(true);
    expect(queryTestId('field-clear', el)).toBeNull();
  });

  it('un clic dans la boîte donne le focus au champ', async () => {
    const { fixture, el, input } = await render();
    (el.querySelector('.g-field__suffix') as HTMLElement).click();
    await fixture.whenStable();
    expect(document.activeElement).toBe(input);
  });

  it('champ numérique : la saisie en cours est gardée, remise en forme à la validation', async () => {
    const fixture = TestBed.createComponent(NumberHost);
    await fixture.whenStable();
    const input = (fixture.nativeElement as HTMLElement).querySelector('input')!;
    const host = fixture.componentInstance;
    expect(input.value).toBe('0,81');

    // « 0, » vaut 0 : la virgule reste pendant la frappe.
    input.value = '0,';
    input.dispatchEvent(new Event('input'));
    await fixture.whenStable();
    expect(host.value()).toBe(0);
    expect(input.value).toBe('0,');
    input.value = '0,70';
    input.dispatchEvent(new Event('input'));
    await fixture.whenStable();
    expect(host.value()).toBe(0.7);
    expect(input.value).toBe('0,70');

    // Validation : la forme de la valeur liée revient ; un texte illisible est remplacé.
    input.dispatchEvent(new Event('change'));
    await fixture.whenStable();
    expect(input.value).toBe('0,7');
    input.value = 'abc';
    input.dispatchEvent(new Event('input'));
    input.dispatchEvent(new Event('change'));
    await fixture.whenStable();
    expect(input.value).toBe('0,7');

    // Une valeur différente venue du parent remplace la saisie.
    input.value = '0,5';
    input.dispatchEvent(new Event('input'));
    await fixture.whenStable();
    host.value.set(0.81);
    await fixture.whenStable();
    expect(input.value).toBe('0,81');
  });
});

@Component({
  imports: [FIELD],
  template: `<g-field>
    <input gInput [value]="text()" [sameValue]="sameNumber" (input)="onInput($event)" />
  </g-field>`,
})
class NumberHost {
  readonly value = signal(0.81);
  readonly sameNumber = sameNumber;
  text(): string {
    return decimal(this.value());
  }
  onInput(event: Event): void {
    const x = parseNumber((event.target as HTMLInputElement).value);
    if (x !== null) this.value.set(x);
  }
}
