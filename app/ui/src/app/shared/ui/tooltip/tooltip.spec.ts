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

@Component({
  imports: [Tooltip],
  template: `<span data-testid="score" gTooltip="Score 86 sur 100" [gTooltipTemplate]="detail"
      >86</span
    >
    <ng-template #detail><strong class="rich">Qualité 82</strong></ng-template>
    <span data-testid="name" gTooltipIfTruncated [gTooltip]="name">{{ name }}</span>`,
})
class RichHost {
  readonly name = 'Qwen3.8-27B-TURBO-Fable-Cold-Fusion';
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

  it('contenu riche : le gabarit dans une bulle claire, le texte reste la description', async () => {
    noDelay();
    const fixture = TestBed.createComponent(RichHost);
    await fixture.whenStable();
    const score = (fixture.nativeElement as HTMLElement).querySelector('[data-testid="score"]')!;
    const id = score.getAttribute('aria-describedby')!;
    expect(document.getElementById(id)?.textContent).toBe('Score 86 sur 100');
    score.dispatchEvent(new MouseEvent('mouseenter'));
    const panel = bubble()!;
    expect(panel.classList).toContain('g-tooltip--rich');
    expect(panel.querySelector('.rich')?.textContent).toBe('Qualité 82');
    expect(panel.textContent).not.toContain('Score 86');
    score.dispatchEvent(new MouseEvent('mouseleave'));
  });

  it('gTooltipIfTruncated : seulement si le texte est coupé, sans aria-describedby', async () => {
    noDelay();
    const fixture = TestBed.createComponent(RichHost);
    await fixture.whenStable();
    const name = (fixture.nativeElement as HTMLElement).querySelector<HTMLElement>(
      '[data-testid="name"]',
    )!;
    expect(name.hasAttribute('aria-describedby')).toBe(false);
    name.dispatchEvent(new MouseEvent('mouseenter'));
    expect(bubble()).toBeNull();
    name.dispatchEvent(new MouseEvent('mouseleave'));

    Object.defineProperty(name, 'scrollWidth', { configurable: true, value: 320 });
    Object.defineProperty(name, 'clientWidth', { configurable: true, value: 180 });
    name.dispatchEvent(new MouseEvent('mouseenter'));
    expect(bubble()?.textContent).toContain('Qwen3.8-27B-TURBO-Fable-Cold-Fusion');
    name.dispatchEvent(new MouseEvent('mouseleave'));
  });
});
