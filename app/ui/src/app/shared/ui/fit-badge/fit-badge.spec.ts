import { TestBed } from '@angular/core/testing';
import { text } from '../../../../testing/dom';
import type { FitLevel } from '../../../core/generated/FitLevel';
import { FitBadge } from './fit-badge';

describe('FitBadge', () => {
  it.each<[FitLevel, string, string]>([
    ['perfect', 'Parfait', 'check'],
    ['good', 'Bon', 'circle'],
    ['marginal', 'Limite', 'warning'],
    ['tooTight', 'Trop juste', 'minus-circle'],
  ])('%s : icône et texte, jamais la couleur seule', async (fit, label, icon) => {
    const fixture = TestBed.createComponent(FitBadge);
    fixture.componentRef.setInput('fit', fit);
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    expect(text(el)).toBe(label);
    expect(el.dataset['fit']).toBe(fit);
    expect(el.querySelector('g-icon')?.getAttribute('data-icon')).toBe(icon);
  });
});
