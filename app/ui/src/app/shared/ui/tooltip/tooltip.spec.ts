import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { press } from '../../../../testing/dom';
import { TOOLTIP_DELAY_MS, Tooltip } from './tooltip';

@Component({
  imports: [Tooltip],
  template: `<button
    type="button"
    data-testid="rescan"
    [gTooltip]="message()"
    gTooltipShortcut="R"
    [gTooltipDisabled]="disabled()"
  >
    Ré-analyser
  </button>`,
})
class Host {
  readonly message = signal('Détecter de nouveau\nle matériel');
  readonly disabled = signal(false);
}

const bubble = () => document.querySelector<HTMLElement>('g-tooltip-panel');
const noDelay = () =>
  TestBed.configureTestingModule({ providers: [{ provide: TOOLTIP_DELAY_MS, useValue: 0 }] });

describe('Tooltip', () => {
  async function render() {
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    const button = (fixture.nativeElement as HTMLElement).querySelector('button')!;
    return { fixture, button, host: fixture.componentInstance };
  }

  afterEach(() => vi.useRealTimers());

  it('relie le texte par aria-describedby et annonce le raccourci', async () => {
    const { fixture, button, host } = await render();
    const id = button.getAttribute('aria-describedby')!;
    expect(document.getElementById(id)?.textContent).toBe('Détecter de nouveau\nle matériel');
    expect(button.getAttribute('aria-keyshortcuts')).toBe('R');
    host.disabled.set(true);
    await fixture.whenStable();
    expect(button.hasAttribute('aria-describedby')).toBe(false);
  });

  it('apparaît après 400 ms de survol, avec sauts de ligne et raccourci ; disparaît en sortant', async () => {
    const { button } = await render();
    vi.useFakeTimers();
    button.dispatchEvent(new MouseEvent('mouseenter'));
    vi.advanceTimersByTime(399);
    expect(bubble()).toBeNull();
    vi.advanceTimersByTime(1);
    const panel = bubble()!;
    expect(panel.textContent).toContain('Détecter de nouveau\nle matériel');
    expect(panel.querySelector('kbd')?.textContent).toBe('R');
    expect(panel.getAttribute('aria-hidden')).toBe('true');
    button.dispatchEvent(new MouseEvent('mouseleave'));
    expect(bubble()).toBeNull();
  });

  it('sortir avant le délai annule l’apparition ; un clic la ferme', async () => {
    const { button } = await render();
    vi.useFakeTimers();
    button.dispatchEvent(new MouseEvent('mouseenter'));
    vi.advanceTimersByTime(200);
    button.dispatchEvent(new MouseEvent('mouseleave'));
    vi.advanceTimersByTime(400);
    expect(bubble()).toBeNull();
    button.dispatchEvent(new MouseEvent('mouseenter'));
    vi.advanceTimersByTime(400);
    expect(bubble()).not.toBeNull();
    button.dispatchEvent(new Event('pointerdown'));
    expect(bubble()).toBeNull();
  });

  it('au focus clavier (pas à la souris) ; Échap la ferme', async () => {
    noDelay();
    const { fixture, button } = await render();
    button.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, buttons: 1, detail: 1 }));
    button.focus();
    await fixture.whenStable();
    expect(bubble()).toBeNull();
    button.blur();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', keyCode: 9 }));
    button.focus();
    await fixture.whenStable();
    expect(bubble()).not.toBeNull();
    await press(fixture, document.body, 'Escape');
    expect(bubble()).toBeNull();
    button.blur();
  });

  it('suit un changement de texte ; un texte vide la ferme', async () => {
    noDelay();
    const { fixture, button, host } = await render();
    button.dispatchEvent(new MouseEvent('mouseenter'));
    expect(bubble()).not.toBeNull();
    host.message.set('Autre texte');
    await fixture.whenStable();
    expect(bubble()?.textContent).toContain('Autre texte');
    host.message.set('');
    await fixture.whenStable();
    expect(bubble()).toBeNull();
  });
});
