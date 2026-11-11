import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { clearField, queryTestId, typeInto } from '../../../../testing/dom';
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
});
