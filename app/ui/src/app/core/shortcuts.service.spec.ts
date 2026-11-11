import { Dialog } from '@angular/cdk/dialog';
import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { ShortcutsService } from './shortcuts.service';

@Component({ template: '<p>dialogue</p>' })
class SomeDialog {}

function key(target: EventTarget, k: string, init: KeyboardEventInit = {}): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(event);
  return event;
}

describe('ShortcutsService', () => {
  let service: ShortcutsService;
  let log: string[];
  let input: HTMLInputElement;
  let off: () => void;

  beforeEach(() => {
    service = TestBed.inject(ShortcutsService);
    log = [];
    off = service.register([
      { key: 'k', mod: true, inInputs: true, run: () => void log.push('search') },
      { key: 'r', run: () => void log.push('rescan') },
      { key: 'Escape', inInputs: true, run: () => void log.push('escape') },
      { key: 'e', run: () => false },
      { key: '1', code: 'Digit1', run: () => void log.push('usage 1') },
    ]);
    input = document.createElement('input');
    document.body.appendChild(input);
  });

  afterEach(() => {
    off();
    input.remove();
  });

  it('touche seule hors des champs, majuscule comprise ; empêche l’action du navigateur', () => {
    const event = key(document.body, 'r');
    expect(event.defaultPrevented).toBe(true);
    key(document.body, 'R', { shiftKey: true });
    expect(log).toEqual(['rescan', 'rescan']);
  });

  it('chiffre : la touche physique suffit (AZERTY sans Maj)', () => {
    key(document.body, '&', { code: 'Digit1' });
    key(document.body, '1', { code: 'Numpad1' });
    key(document.body, '&', { code: 'KeyQ' });
    expect(log).toEqual(['usage 1', 'usage 1']);
  });

  it('ignorée dans un champ de saisie, sauf Ctrl K et Échap', () => {
    key(input, 'r');
    key(input, 'k', { ctrlKey: true });
    key(input, 'Escape');
    expect(log).toEqual(['search', 'escape']);
  });

  it('les modificateurs doivent correspondre', () => {
    key(document.body, 'r', { ctrlKey: true });
    key(document.body, 'k');
    key(document.body, 'k', { ctrlKey: true, altKey: true });
    expect(log).toEqual([]);
    expect(service.modShortcut('K')).toBe('Ctrl K');
  });

  it('une touche déjà traitée, répétée ou refusée par l’action est laissée', () => {
    const handled = new KeyboardEvent('keydown', { key: 'r', bubbles: true, cancelable: true });
    handled.preventDefault();
    document.body.dispatchEvent(handled);
    key(document.body, 'r', { repeat: true });
    const refused = key(document.body, 'e');
    expect(log).toEqual([]);
    expect(refused.defaultPrevented).toBe(false);
  });

  it('ignorés pendant un dialogue ; retirés par la fonction renvoyée', () => {
    const ref = TestBed.inject(Dialog).open(SomeDialog);
    key(document.body, 'r');
    expect(log).toEqual([]);
    ref.close();
    key(document.body, 'r');
    expect(log).toEqual(['rescan']);
    off();
    key(document.body, 'r');
    expect(log).toEqual(['rescan']);
  });
});
