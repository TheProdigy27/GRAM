import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { byTestId, text } from '../../../testing/dom';
import { makeScore } from '../../../testing/fixtures';
import type { Score } from '../../core/generated/Score';
import { TOOLTIP_DELAY_MS } from '../../shared/ui/tooltip/tooltip';
import { ScoreCell } from './score-cell';

@Component({
  imports: [ScoreCell],
  template: `<app-score-cell [score]="score()" [compact]="compact()" />`,
})
class Host {
  readonly score = signal<Score>(makeScore(86.4));
  readonly compact = signal(false);
}

describe('ScoreCell', () => {
  async function render() {
    TestBed.configureTestingModule({ providers: [{ provide: TOOLTIP_DELAY_MS, useValue: 0 }] });
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    return { fixture, el: fixture.nativeElement as HTMLElement };
  }

  it('quatre mini-barres (qualité, vitesse, marge, contexte) puis le total arrondi', async () => {
    const { el } = await render();
    const bars = [...el.querySelectorAll<HTMLElement>('.spark__bar')].map((b) => b.style.height);
    // 82, 71, 100, 100 sur 18 px.
    expect(bars).toEqual(['15px', '13px', '18px', '18px']);
    expect(el.querySelector('.spark')?.getAttribute('aria-hidden')).toBe('true');
    expect(text(byTestId('score', el))).toBe('86');
  });

  it('compact : le total seul', async () => {
    const { fixture, el } = await render();
    fixture.componentInstance.compact.set(true);
    await fixture.whenStable();
    expect(el.querySelector('.spark')).toBeNull();
    expect(text(byTestId('score', el))).toBe('86');
  });

  it('infobulle : les axes en barres avec leurs poids, la formule et les facteurs', async () => {
    const { fixture, el } = await render();
    fixture.componentInstance.score.set(makeScore(78, { useCaseFactor: 0.92, confidence: 0.95 }));
    await fixture.whenStable();
    const score = el.querySelector<HTMLElement>('.score')!;
    const id = score.getAttribute('aria-describedby')!;
    expect(document.getElementById(id)?.textContent).toContain('Qualité 82 (poids 0,45)');

    score.dispatchEvent(new MouseEvent('mouseenter'));
    const panel = document.querySelector('g-tooltip-panel')!;
    expect(panel.classList).toContain('g-tooltip--rich');
    const labels = [...panel.querySelectorAll('.axis__label')].map(text);
    expect(labels).toEqual(['Qualité', 'Vitesse', 'Marge mémoire', 'Contexte']);
    const fills = [...panel.querySelectorAll<HTMLElement>('.axis__fill')];
    expect(fills.map((f) => f.dataset['axis'])).toEqual(['quality', 'speed', 'fit', 'context']);
    expect(fills[1].style.width).toBe('71%');
    expect([...panel.querySelectorAll('.axis__weight')].map(text)).toEqual([
      '× 0,45',
      '× 0,3',
      '× 0,15',
      '× 0,1',
    ]);
    expect(text(panel)).toContain('Moyenne pondérée 82,2 × 0,95 + popularité 3,2');
    expect([...panel.querySelectorAll('.detail__line--factor')].map(text)).toEqual([
      '× 0,92 : ne correspond pas au cas d’usage',
      '× 0,95 : estimation approximative',
    ]);
    score.dispatchEvent(new MouseEvent('mouseleave'));
  });
});
