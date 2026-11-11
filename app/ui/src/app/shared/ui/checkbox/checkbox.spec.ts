import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { setChecked, text } from '../../../../testing/dom';
import { Checkbox } from './checkbox';

@Component({
  imports: [Checkbox],
  template: `<g-checkbox data-testid="close-all" [disabled]="disabled()" [(checked)]="checked">
    Je ferme tout avant de lancer
  </g-checkbox>`,
})
class Host {
  readonly checked = signal(false);
  readonly disabled = signal(false);
}

describe('Checkbox', () => {
  async function render() {
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    return { fixture, el, input: el.querySelector('input')!, host: fixture.componentInstance };
  }

  it('case native dans son libellé, cochée au clic', async () => {
    const { fixture, el, input, host } = await render();
    expect(input.type).toBe('checkbox');
    expect(input.closest('label')).not.toBeNull();
    expect(text(el.querySelector('label'))).toBe('Je ferme tout avant de lancer');
    await setChecked(fixture, 'close-all', true);
    expect(host.checked()).toBe(true);
    expect(input.checked).toBe(true);
    await setChecked(fixture, 'close-all', false);
    expect(host.checked()).toBe(false);
  });

  it('suit la valeur du parent ; désactivée', async () => {
    const { fixture, input, host } = await render();
    host.checked.set(true);
    host.disabled.set(true);
    await fixture.whenStable();
    expect(input.checked).toBe(true);
    expect(input.disabled).toBe(true);
  });
});
