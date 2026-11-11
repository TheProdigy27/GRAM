import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { click, text, toggleSwitch } from '../../../../testing/dom';
import { Switch } from './switch';

@Component({
  imports: [Switch],
  template: `<g-switch data-testid="too-big" [disabled]="disabled()" [(checked)]="checked">
    Trop gros
  </g-switch>`,
})
class Host {
  readonly checked = signal(false);
  readonly disabled = signal(false);
}

describe('Switch', () => {
  async function render() {
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    const button = (fixture.nativeElement as HTMLElement).querySelector('button')!;
    return { fixture, button, host: fixture.componentInstance };
  }

  it('bouton role="switch" nommé par son libellé, bascule au clic', async () => {
    const { fixture, button, host } = await render();
    expect(button.getAttribute('role')).toBe('switch');
    expect(button.getAttribute('aria-checked')).toBe('false');
    expect(text(button)).toBe('Trop gros');
    expect(await toggleSwitch(fixture, 'too-big')).toBe(true);
    expect(host.checked()).toBe(true);
    expect(await toggleSwitch(fixture, 'too-big')).toBe(false);
    expect(host.checked()).toBe(false);
  });

  it('suit la valeur du parent ; désactivé, il ne bascule plus', async () => {
    const { fixture, button, host } = await render();
    host.checked.set(true);
    host.disabled.set(true);
    await fixture.whenStable();
    expect(button.getAttribute('aria-checked')).toBe('true');
    expect(button.disabled).toBe(true);
    await click(fixture, button);
    expect(host.checked()).toBe(true);
  });
});
