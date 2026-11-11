import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { text } from '../../../../testing/dom';
import { EmptyState } from './empty-state';

@Component({
  imports: [EmptyState],
  template: `<g-empty-state
    heading="Aucun modèle ne correspond à « llama 405b »"
    description="2 modèles trop gros correspondent."
  >
    <button type="button">Voir les trop gros</button>
  </g-empty-state>`,
})
class Host {}

describe('EmptyState', () => {
  it('icône, titre, explication et action', async () => {
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    const el = (fixture.nativeElement as HTMLElement).querySelector('g-empty-state')!;
    expect(el.querySelector('g-icon')?.getAttribute('data-icon')).toBe('search');
    expect(text(el.querySelector('.g-empty-state__heading'))).toBe(
      'Aucun modèle ne correspond à « llama 405b »',
    );
    expect(text(el.querySelector('.g-empty-state__description'))).toBe(
      '2 modèles trop gros correspondent.',
    );
    expect(text(el.querySelector('.g-empty-state__actions button'))).toBe('Voir les trop gros');
  });
});
