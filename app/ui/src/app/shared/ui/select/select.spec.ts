import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { chooseOption, click, optionLabels, press, text } from '../../../../testing/dom';
import { Select, type SelectOption } from './select';

type Runtime = 'ollama' | 'lmStudio' | 'llamaCpp';

@Component({
  imports: [Select],
  template: `<g-select
    data-testid="runtime"
    label="Runtime"
    [options]="options"
    [(value)]="value"
  />`,
})
class Host {
  readonly options: SelectOption<Runtime>[] = [
    { value: 'ollama', label: 'Ollama' },
    { value: 'lmStudio', label: 'LM Studio', hint: 'non installé' },
    { value: 'llamaCpp', label: 'llama.cpp', disabled: true },
  ];
  readonly value = signal<Runtime | null>(null);
}

describe('Select', () => {
  async function render() {
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    const trigger = el.querySelector<HTMLButtonElement>('button[aria-haspopup="listbox"]')!;
    return { fixture, el, trigger, host: fixture.componentInstance };
  }
  const list = (trigger: HTMLElement) =>
    document.getElementById(trigger.getAttribute('aria-controls') ?? '');

  it('déclencheur : libellé, invite, état fermé', async () => {
    const { el, trigger } = await render();
    expect(text(trigger)).toBe('Choisir…');
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    expect(trigger.hasAttribute('aria-controls')).toBe(false);
    const label = el.querySelector('.g-select__label')!;
    expect(trigger.getAttribute('aria-labelledby')).toContain(label.id);
  });

  it('ouvre la liste au clic, choisit une option, se ferme et rend le focus', async () => {
    const { fixture, trigger, host } = await render();
    await click(fixture, trigger);
    expect(trigger.getAttribute('aria-expanded')).toBe('true');
    const listbox = list(trigger)!;
    expect(listbox.getAttribute('role')).toBe('listbox');
    expect(optionLabels(listbox)).toEqual(['Ollama', 'LM Studio', 'llama.cpp']);
    const options = listbox.querySelectorAll('[role="option"]');
    expect(text(options[1])).toContain('non installé');
    expect(options[2].getAttribute('aria-disabled')).toBe('true');

    await chooseOption(fixture, 'runtime', 'LM Studio');
    expect(host.value()).toBe('lmStudio');
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    expect(list(trigger)).toBeNull();
    expect(text(trigger)).toBe('LM Studio');
    expect(document.activeElement).toBe(trigger);
  });

  it('clavier : ↓ ouvre sur l’option choisie, les flèches parcourent, Entrée choisit', async () => {
    const { fixture, trigger, host } = await render();
    host.value.set('ollama');
    await fixture.whenStable();
    await press(fixture, trigger, 'ArrowDown');
    const listbox = list(trigger)!;
    expect(listbox).not.toBeNull();
    const options = listbox.querySelectorAll('[role="option"]');
    expect(options[0].getAttribute('aria-selected')).toBe('true');
    expect(document.activeElement).toBe(options[0]);
    await press(fixture, listbox, 'ArrowDown');
    expect(document.activeElement).toBe(options[1]);
    await press(fixture, listbox, 'Enter');
    await fixture.whenStable();
    expect(host.value()).toBe('lmStudio');
    expect(list(trigger)).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it('Échap ferme sans changer la valeur ; un clic sur l’option déjà choisie ferme aussi', async () => {
    const { fixture, trigger, host } = await render();
    host.value.set('ollama');
    await click(fixture, trigger);
    await press(fixture, document.body, 'Escape');
    expect(list(trigger)).toBeNull();
    expect(host.value()).toBe('ollama');
    expect(document.activeElement).toBe(trigger);

    await chooseOption(fixture, trigger, 'Ollama');
    await fixture.whenStable();
    expect(list(trigger)).toBeNull();
    expect(host.value()).toBe('ollama');
  });
});
