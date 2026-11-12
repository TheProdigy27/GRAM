import { DialogRef } from '@angular/cdk/dialog';
import { TestBed } from '@angular/core/testing';
import { allByTestId, text } from '../../../testing/dom';
import { ShortcutsHelp, shortcutGroups } from './shortcuts-help';

describe('ShortcutsHelp', () => {
  it('table des raccourcis, avec ⌘ sur Mac', () => {
    const groups = shortcutGroups((k) => `⌘${k}`);
    expect(groups.map((g) => g.title)).toEqual(['Partout', 'Classement', 'Palette', 'Fiche']);
    expect(groups[0].rows[0]).toEqual({
      keys: ['⌘K'],
      label: 'Chercher un modèle ou une commande',
    });
    expect(groups[2].rows.at(-1)!.keys).toEqual(['⌘Entrée']);
  });

  it('affiche chaque groupe et ses touches', async () => {
    TestBed.configureTestingModule({
      imports: [ShortcutsHelp],
      providers: [{ provide: DialogRef, useValue: { config: {}, close: () => undefined } }],
    });
    const fixture = TestBed.createComponent(ShortcutsHelp);
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;

    expect(text(el.querySelector('h2'))).toBe('Raccourcis clavier');
    expect([...el.querySelectorAll('h3')].map(text)).toEqual([
      'Partout',
      'Classement',
      'Palette',
      'Fiche',
    ]);
    const rows = allByTestId('shortcut-row', el);
    expect(text(rows[0].querySelector('dt'))).toBe('Ctrl K');
    expect(text(rows[0].querySelector('dd'))).toBe('Chercher un modèle ou une commande');
    const usage = rows.find((r) => text(r).includes('Usage'))!;
    expect([...usage.querySelectorAll('kbd')].map(text)).toEqual(['1', '5']);
    expect(text(usage.querySelector('dt'))).toBe('1 à 5');
  });
});
