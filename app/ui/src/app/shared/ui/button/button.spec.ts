import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { click } from '../../../../testing/dom';
import { Button, type ButtonVariant } from './button';

@Component({
  imports: [Button],
  template: `<button
    gButton
    data-testid="btn"
    [variant]="variant()"
    [busy]="busy()"
    [disabled]="disabled()"
    (click)="clicks = clicks + 1"
  >
    Enregistrer
  </button>`,
})
class Host {
  readonly variant = signal<ButtonVariant>('primary');
  readonly busy = signal(false);
  readonly disabled = signal(false);
  clicks = 0;
}

describe('Button', () => {
  async function render() {
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    const button = (fixture.nativeElement as HTMLElement).querySelector('button')!;
    return { fixture, button, host: fixture.componentInstance };
  }

  it('bouton natif avec sa variante et sa taille', async () => {
    const { fixture, button, host } = await render();
    expect(button.classList).toContain('g-button');
    expect(button.dataset['variant']).toBe('primary');
    expect(button.dataset['size']).toBe('md');
    expect(button.textContent?.trim()).toBe('Enregistrer');
    await click(fixture, button);
    expect(host.clicks).toBe(1);
    host.variant.set('danger');
    await fixture.whenStable();
    expect(button.dataset['variant']).toBe('danger');
  });

  it('occupé : indicateur, aria-busy, plus de clic', async () => {
    const { fixture, button, host } = await render();
    host.busy.set(true);
    await fixture.whenStable();
    expect(button.getAttribute('aria-busy')).toBe('true');
    expect(button.disabled).toBe(true);
    expect(button.querySelector('g-spinner')).not.toBeNull();
    button.click();
    expect(host.clicks).toBe(0);
    host.busy.set(false);
    await fixture.whenStable();
    expect(button.hasAttribute('aria-busy')).toBe(false);
    expect(button.querySelector('g-spinner')).toBeNull();
    expect(button.disabled).toBe(false);
  });

  it('désactivé par [disabled]', async () => {
    const { fixture, button, host } = await render();
    host.disabled.set(true);
    await fixture.whenStable();
    expect(button.disabled).toBe(true);
  });

  it('bouton-icône carré et petite taille', async () => {
    @Component({
      imports: [Button],
      template: `<button gButton icon size="sm" variant="ghost" aria-label="Copier">⧉</button>`,
    })
    class IconHost {}
    const fixture = TestBed.createComponent(IconHost);
    await fixture.whenStable();
    const button = (fixture.nativeElement as HTMLElement).querySelector('button')!;
    expect(button.classList).toContain('g-button--icon');
    expect(button.dataset['size']).toBe('sm');
    expect(button.dataset['variant']).toBe('ghost');
    expect(button.getAttribute('aria-label')).toBe('Copier');
  });
});
