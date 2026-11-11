import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { chooseOption, press, text } from '../../../../testing/dom';
import { ToggleGroup, type ToggleOption } from './toggle-group';

@Component({
  imports: [ToggleGroup],
  template: `<g-toggle-group
    data-testid="ctx"
    label="Contexte"
    [appearance]="appearance()"
    [options]="options"
    [(value)]="value"
  />`,
})
class Host {
  readonly options: ToggleOption<number>[] = [
    { value: 2048, label: '2k' },
    { value: 8192, label: '8k', icon: 'context' },
    { value: 32768, label: '32k', disabled: true },
    { value: 131072, label: '128k' },
  ];
  readonly value = signal<number | null>(8192);
  readonly appearance = signal<'chips' | 'segmented'>('chips');
}

describe('ToggleGroup', () => {
  async function render() {
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    const group = (fixture.nativeElement as HTMLElement).querySelector('g-toggle-group')!;
    const buttons = [...group.querySelectorAll('button')];
    return { fixture, group, buttons, host: fixture.componentInstance };
  }

  it('groupe nommé de boutons à bascule', async () => {
    const { group, buttons } = await render();
    expect(group.getAttribute('role')).toBe('group');
    expect(group.getAttribute('aria-label')).toBe('Contexte');
    expect(buttons.map(text)).toEqual(['2k', '8k', '32k', '128k']);
    expect(buttons.map((b) => b.getAttribute('aria-pressed'))).toEqual([
      'false',
      'true',
      'false',
      'false',
    ]);
    expect(buttons[1].querySelector('g-icon')).not.toBeNull();
    expect(buttons[2].disabled).toBe(true);
  });

  it('un clic choisit la valeur', async () => {
    const { fixture, buttons, host } = await render();
    await chooseOption(fixture, 'ctx', '128k');
    expect(host.value()).toBe(131072);
    expect(buttons[3].getAttribute('aria-pressed')).toBe('true');
    expect(buttons[1].getAttribute('aria-pressed')).toBe('false');
  });

  it('← → passent d’un bouton actif à l’autre, en boucle', async () => {
    const { fixture, buttons } = await render();
    buttons[1].focus();
    await press(fixture, buttons[1], 'ArrowRight');
    expect(document.activeElement).toBe(buttons[3]); // le 32k désactivé est sauté
    await press(fixture, buttons[3], 'ArrowRight');
    expect(document.activeElement).toBe(buttons[0]);
    await press(fixture, buttons[0], 'ArrowLeft');
    expect(document.activeElement).toBe(buttons[3]);
  });

  it('contrôle segmenté', async () => {
    const { fixture, group, host } = await render();
    host.appearance.set('segmented');
    await fixture.whenStable();
    expect(group.getAttribute('data-appearance')).toBe('segmented');
  });
});
