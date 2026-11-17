import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { press } from '../../../../testing/dom';
import { TOOLTIP_DELAY_MS } from '../tooltip/tooltip';
import { InfoTip } from './info-tip';

@Component({
  imports: [InfoTip],
  template: `<h2>Contexte</h2>
    <g-info [entry]="entry" />`,
})
class Host {
  readonly entry = { term: 'Contexte', text: 'Longueur de conversation gardée en tête.' };
}

const bubble = () => document.querySelector<HTMLElement>('g-tooltip-panel');

describe('InfoTip', () => {
  async function render() {
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    const button = (fixture.nativeElement as HTMLElement).querySelector('button')!;
    return { fixture, button };
  }

  afterEach(() => vi.useRealTimers());

  it('bouton nommé « Aide : terme », explication reliée par aria-describedby', async () => {
    const { button } = await render();
    expect(button.type).toBe('button');
    expect(button.getAttribute('aria-label')).toBe('Aide : Contexte');
    const id = button.getAttribute('aria-describedby')!;
    expect(document.getElementById(id)?.textContent).toBe(
      'Longueur de conversation gardée en tête.',
    );
  });

  it('au survol après le délai ; disparaît en sortant', async () => {
    const { button } = await render();
    vi.useFakeTimers();
    button.dispatchEvent(new MouseEvent('mouseenter'));
    expect(bubble()).toBeNull();
    vi.advanceTimersByTime(400);
    expect(bubble()?.textContent).toContain('Longueur de conversation gardée en tête.');
    button.dispatchEvent(new MouseEvent('mouseleave'));
    expect(bubble()).toBeNull();
  });

  it('un clic l’affiche tout de suite ; Échap la ferme', async () => {
    const { fixture, button } = await render();
    button.dispatchEvent(new Event('pointerdown'));
    button.click();
    expect(bubble()?.textContent).toContain('Longueur de conversation gardée en tête.');
    await press(fixture, document.body, 'Escape');
    expect(bubble()).toBeNull();
  });

  it('au focus clavier', async () => {
    TestBed.configureTestingModule({ providers: [{ provide: TOOLTIP_DELAY_MS, useValue: 0 }] });
    const { fixture, button } = await render();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', keyCode: 9 }));
    button.focus();
    await fixture.whenStable();
    expect(bubble()).not.toBeNull();
    button.blur();
    expect(bubble()).toBeNull();
  });
});
