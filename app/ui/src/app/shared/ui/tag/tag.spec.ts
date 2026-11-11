import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { text } from '../../../../testing/dom';
import { Tag } from './tag';

@Component({
  imports: [Tag],
  template: `
    <g-tag icon="eye">Vision</g-tag>
    <g-tag tone="ok">Installé</g-tag>
    <g-tag icon="lock" size="sm">Protégé</g-tag>
    <g-tag tone="warn">Simulation · RTX 4090</g-tag>
    <g-tag tone="accent" icon="star">meilleure quantification</g-tag>
  `,
})
class Host {}

describe('Tag', () => {
  it('ton, taille et icône optionnelle', async () => {
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    const tags = [...(fixture.nativeElement as HTMLElement).querySelectorAll<HTMLElement>('g-tag')];
    expect(tags.map(text)).toEqual([
      'Vision',
      'Installé',
      'Protégé',
      'Simulation · RTX 4090',
      'meilleure quantification',
    ]);
    expect(tags.map((t) => t.dataset['tone'])).toEqual([
      'neutral',
      'ok',
      'neutral',
      'warn',
      'accent',
    ]);
    expect(tags[0].querySelector('g-icon')?.getAttribute('data-icon')).toBe('eye');
    expect(tags[1].querySelector('g-icon')).toBeNull();
    expect(tags[2].dataset['size']).toBe('sm');
    expect(tags[4].querySelector('g-icon')?.getAttribute('data-icon')).toBe('star');
  });
});
