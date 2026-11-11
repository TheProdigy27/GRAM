import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { text } from '../../../../testing/dom';
import { Callout } from './callout';

@Component({
  imports: [Callout],
  template: `
    <g-callout>Réglez num_ctx sur le contexte indiqué.</g-callout>
    <g-callout kind="warn" heading="Attention à fenêtre glissante">KV surestimé.</g-callout>
    <g-callout kind="error" role="alert">
      Hugging Face injoignable.
      <button gCalloutAction type="button">Réessayer</button>
    </g-callout>
  `,
})
class Host {}

describe('Callout', () => {
  it('information, attention, erreur : icône, titre, action', async () => {
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    const [info, warn, error] = [
      ...(fixture.nativeElement as HTMLElement).querySelectorAll<HTMLElement>('g-callout'),
    ];
    expect(info.dataset['kind']).toBe('info');
    expect(info.querySelector('g-icon')?.getAttribute('data-icon')).toBe('info');
    expect(warn.querySelector('g-icon')?.getAttribute('data-icon')).toBe('warning');
    expect(text(warn.querySelector('.g-callout__heading'))).toBe('Attention à fenêtre glissante');
    expect(error.querySelector('g-icon')?.getAttribute('data-icon')).toBe('error');
    expect(error.getAttribute('role')).toBe('alert');
    // L'action est hors du corps du texte, à droite.
    expect(error.querySelector('.g-callout__body button')).toBeNull();
    expect(text(error.querySelector(':scope > button'))).toBe('Réessayer');
  });
});
