import { TestBed } from '@angular/core/testing';
import { MatDialogRef } from '@angular/material/dialog';
import { emit } from '@tauri-apps/api/event';
import { clearMocks } from '@tauri-apps/api/mocks';
import { type Handler, argsOf, mockBackend } from '../../../testing/backend';
import type { UpdateCheck } from '../../core/generated/UpdateCheck';
import { RELEASES_URL, UpdateStore, noteBlocks } from '../../state/update.store';
import { UpdateDialog, releaseDate } from './update-dialog';

const AVAILABLE: UpdateCheck = {
  currentVersion: '0.1.0',
  update: {
    version: '0.2.0',
    notes: '## Nouveautés\n\n- Mises à jour automatiques\n- Export plus rapide\n\nMerci !',
    date: '2026-11-11T12:00:00Z',
  },
};

async function open(handlers: Record<string, Handler> = {}) {
  const calls = mockBackend(handlers);
  const dialogRef = { disableClose: false, close: () => undefined };
  TestBed.configureTestingModule({
    imports: [UpdateDialog],
    providers: [{ provide: MatDialogRef, useValue: dialogRef }],
  });
  const store = TestBed.inject(UpdateStore);
  const fixture = TestBed.createComponent(UpdateDialog);
  await fixture.whenStable();
  // La vérification lancée à l'ouverture n'est pas suivie par Angular.
  await new Promise((r) => setTimeout(r));
  await fixture.whenStable();
  const el = fixture.nativeElement as HTMLElement;
  const find = (testId: string) => el.querySelector(`[data-testid="${testId}"]`);
  const click = async (testId: string) => {
    (find(testId) as HTMLElement).click();
    await new Promise((r) => setTimeout(r));
    await fixture.whenStable();
  };
  return { calls, dialogRef, store, fixture, el, find, click };
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

describe('UpdateDialog', () => {
  afterEach(() => clearMocks());

  it('notes de version et date', () => {
    expect(noteBlocks(AVAILABLE.update!.notes!)).toEqual([
      { kind: 'heading', text: 'Nouveautés' },
      { kind: 'item', text: 'Mises à jour automatiques' },
      { kind: 'item', text: 'Export plus rapide' },
      { kind: 'text', text: 'Merci !' },
    ]);
    expect(noteBlocks('* <b>pas du HTML</b>')).toEqual([
      { kind: 'item', text: '<b>pas du HTML</b>' },
    ]);
    expect(releaseDate('2026-11-11T12:00:00Z')).toBe('11 novembre 2026');
    expect(releaseDate('pas une date')).toBeNull();
    expect(releaseDate(null)).toBeNull();
  });

  it('vérifie à l’ouverture : GRAM est à jour', async () => {
    const { calls, find } = await open();
    expect(argsOf(calls, 'check_update')).toHaveLength(1);
    expect(text(find('update-uptodate'))).toBe('check_circle GRAM 0.1.0 est à jour.');
    expect(find('update-install')).toBeNull();
  });

  it('mise à jour disponible : notes, installation, progression ; fermeture bloquée', async () => {
    const { calls, dialogRef, fixture, find, click, el } = await open({
      check_update: () => AVAILABLE,
    });
    expect(text(find('update-available')?.querySelector('span'))).toBe(
      'GRAM 0.2.0 est disponible (version actuelle : 0.1.0)',
    );
    expect(el.textContent).toContain('Publiée le 11 novembre 2026');
    const notes = find('update-notes')!;
    expect(text(notes.querySelector('h3'))).toBe('Nouveautés');
    expect([...notes.querySelectorAll('.item')].map(text)).toEqual([
      'Mises à jour automatiques',
      'Export plus rapide',
    ]);

    await click('update-install');
    expect(argsOf(calls, 'install_update')).toHaveLength(1);
    expect(dialogRef.disableClose).toBe(true);
    expect(text(find('update-progress'))).toBe('Téléchargement de GRAM 0.2.0');

    await emit('update-progress', { downloaded: 3 * 2 ** 20, total: 9 * 2 ** 20 });
    fixture.detectChanges();
    expect(text(find('update-progress'))).toBe('Téléchargement de GRAM 0.2.0 : 3 Mo sur 9 Mo');
    await emit('update-progress', { downloaded: 9 * 2 ** 20, total: 9 * 2 ** 20 });
    fixture.detectChanges();
    expect(text(find('update-progress'))).toBe('Installation… GRAM va redémarrer.');
  });

  it('une mise à jour déjà trouvée n’est pas recherchée de nouveau', async () => {
    const calls = mockBackend({ check_update: () => AVAILABLE });
    TestBed.configureTestingModule({
      imports: [UpdateDialog],
      providers: [{ provide: MatDialogRef, useValue: { disableClose: false } }],
    });
    await TestBed.inject(UpdateStore).check(true);
    const fixture = TestBed.createComponent(UpdateDialog);
    await fixture.whenStable();
    expect(argsOf(calls, 'check_update')).toHaveLength(1);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('GRAM 0.2.0');
  });

  it('échec de l’installation : message, nouvel essai, page des releases', async () => {
    let attempts = 0;
    const { calls, dialogRef, find, click } = await open({
      check_update: () => AVAILABLE,
      install_update: () => {
        attempts++;
        throw new Error('téléchargement impossible : signature invalide, paquet refusé');
      },
    });
    await click('update-install');
    expect(text(find('update-error'))).toBe(
      'téléchargement impossible : signature invalide, paquet refusé',
    );
    expect(dialogRef.disableClose).toBe(false);

    await click('update-retry');
    expect(attempts).toBe(2);
    expect(argsOf(calls, 'check_update')).toHaveLength(1);

    await click('update-releases');
    expect(argsOf(calls, 'plugin:opener|open_url')[0]['url']).toBe(RELEASES_URL);
  });

  it('échec de la vérification : « Réessayer » relance la recherche', async () => {
    let fail = true;
    const { calls, find, click } = await open({
      check_update: () => {
        if (fail) throw new Error('vérification impossible : aucune release trouvée sur GitHub');
        return AVAILABLE;
      },
    });
    expect(text(find('update-error'))).toBe(
      'vérification impossible : aucune release trouvée sur GitHub',
    );
    fail = false;
    await click('update-retry');
    expect(argsOf(calls, 'check_update')).toHaveLength(2);
    expect(find('update-install')).not.toBeNull();
  });
});

describe('UpdateStore', () => {
  afterEach(() => clearMocks());

  it('une vérification automatique qui échoue ne montre rien', async () => {
    mockBackend({
      check_update: () => {
        throw new Error('vérification impossible : GitHub injoignable');
      },
    });
    const store = TestBed.inject(UpdateStore);
    expect(await store.check(true)).toBeNull();
    expect(store.status()).toBe('idle');
    expect(store.error()).toBeNull();
  });

  it('pourcentage téléchargé', async () => {
    mockBackend();
    const store = TestBed.inject(UpdateStore);
    expect(store.percent()).toBeNull();
    store.progress.set({ downloaded: 512, total: null });
    expect(store.percent()).toBeNull();
    expect(store.downloaded()).toBe(false);
    store.progress.set({ downloaded: 256, total: 1024 });
    expect(store.percent()).toBe(25);
    store.progress.set({ downloaded: 1024, total: 1024 });
    expect(store.downloaded()).toBe(true);
  });
});
