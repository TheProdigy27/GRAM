import { TestBed } from '@angular/core/testing';
import { emit } from '@tauri-apps/api/event';
import { clearMocks } from '@tauri-apps/api/mocks';
import { byTestId, click, queryTestId, text } from '@gram/testing/dom';
import { UPDATE_CHECK_DELAY_MS } from '@gram/updates';
import { countOf, mockBackend, type Handler } from '../testing/backend';
import { UPDATE_AVAILABLE } from '../testing/fixtures';
import { App } from './app';
import { appConfig } from './app.config';
import { NEXT_FRAME } from './state/chat.store';

/** Application réelle (configuration comprise), vérification des mises à jour aussitôt. */
async function render(handlers: Record<string, Handler> = {}) {
  TestBed.configureTestingModule({
    providers: [
      ...appConfig.providers,
      { provide: NEXT_FRAME, useValue: (run: () => void) => run() },
      { provide: UPDATE_CHECK_DELAY_MS, useValue: 0 },
    ],
  });
  const calls = mockBackend(handlers);
  const fixture = TestBed.createComponent(App);
  await settle(fixture);
  return { fixture, el: fixture.nativeElement as HTMLElement, calls };
}

/** Laisse passer la vérification différée et les promesses de l'IPC simulé, puis le rendu. */
async function settle(fixture: { whenStable(): Promise<unknown> }): Promise<void> {
  for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r));
  await fixture.whenStable();
}

const toast = () => document.querySelector<HTMLElement>('g-toast-region .g-toast');
const dialogs = () => [...document.querySelectorAll<HTMLElement>('[role="dialog"]')];

describe('Mises à jour de GRAM Harness', () => {
  afterEach(() => {
    clearMocks();
    localStorage.clear();
    document.querySelectorAll('.cdk-overlay-container').forEach((c) => (c.innerHTML = ''));
    document.querySelectorAll('g-toast-region').forEach((r) => r.remove());
  });

  it('trouvée au lancement : toast, bouton de l’en-tête, dialogue de GRAM aux noms du harness', async () => {
    let finish: (() => void) | undefined;
    const { fixture, el, calls } = await render({
      check_update: () => UPDATE_AVAILABLE,
      // L'installation ne revient pas : l'application redémarre.
      install_update: () => new Promise<void>((resolve) => (finish = resolve)),
    });
    expect(countOf(calls, 'check_update')).toBe(1);
    expect(text(toast()?.querySelector('.g-toast__message'))).toBe(
      'GRAM Harness 0.3.0 est disponible.',
    );
    expect(text(byTestId('open-update', el))).toBe('Mise à jour 0.3.0');

    await click(fixture, byTestId('toast-action', toast()!));
    await settle(fixture);
    const dialog = dialogs().at(-1)!;
    expect(text(dialog.querySelector('h2'))).toBe('Mise à jour de GRAM Harness');
    expect(text(byTestId('update-available', dialog))).toContain(
      'GRAM Harness 0.3.0 est disponible',
    );
    expect(text(byTestId('update-notes', dialog))).toBe(
      'NouveautésGRAM Harness se met à jour tout seul.',
    );
    expect(text(dialog)).toContain('vos conversations et paramètres sont conservés');
    // Déjà connue : pas de nouvelle vérification.
    expect(countOf(calls, 'check_update')).toBe(1);

    await click(fixture, byTestId('update-install', dialog));
    expect(countOf(calls, 'install_update')).toBe(1);
    expect(text(byTestId('update-progress', dialog))).toBe('Téléchargement de GRAM Harness 0.3.0');
    await emit('update-progress', { downloaded: 2 ** 20, total: 2 ** 20 });
    await settle(fixture);
    expect(text(byTestId('update-progress', dialog))).toBe(
      'Installation… GRAM Harness va redémarrer.',
    );
    finish?.();
  });

  it('à jour, ou vérification impossible au lancement : rien n’est signalé', async () => {
    const upToDate = await render();
    expect(countOf(upToDate.calls, 'check_update')).toBe(1);
    expect(toast()).toBeNull();
    expect(queryTestId('open-update', upToDate.el)).toBeNull();
    upToDate.fixture.destroy();
    TestBed.resetTestingModule();

    const offline = await render({
      check_update: () => {
        throw 'vérification impossible : GitHub injoignable (connexion ?)';
      },
    });
    expect(countOf(offline.calls, 'check_update')).toBe(1);
    expect(toast()).toBeNull();
    expect(queryTestId('open-update', offline.el)).toBeNull();
  });

  it('paramètres : version, recherche à la demande, puis « Voir et installer »', async () => {
    let check = (): unknown => {
      throw 'vérification impossible : GitHub injoignable (connexion ?)';
    };
    const { fixture, el, calls } = await render({ check_update: () => check() });
    await click(fixture, byTestId('settings', el));
    await settle(fixture);
    const settings = dialogs().at(-1)!;
    expect(text(byTestId('app-version', settings))).toBe('GRAM Harness 0.2.0');
    // L'échec silencieux du lancement n'est pas affiché.
    expect(text(byTestId('update-state', settings))).toBe('');

    await click(fixture, byTestId('update-check', settings));
    await settle(fixture);
    expect(text(byTestId('update-state', settings))).toBe(
      'vérification impossible : GitHub injoignable (connexion ?)',
    );

    check = () => ({ currentVersion: '0.2.0', update: null });
    await click(fixture, byTestId('update-check', settings));
    await settle(fixture);
    expect(text(byTestId('update-state', settings))).toBe('À jour.');

    check = () => UPDATE_AVAILABLE;
    await click(fixture, byTestId('update-check', settings));
    await settle(fixture);
    expect(text(byTestId('update-state', settings))).toBe('GRAM Harness 0.3.0 disponible');
    expect(text(byTestId('open-update', el))).toBe('Mise à jour 0.3.0');
    expect(countOf(calls, 'check_update')).toBe(4);

    await click(fixture, byTestId('update-open', settings));
    await settle(fixture);
    expect(dialogs()).toHaveLength(2);
    expect(text(dialogs().at(-1)!.querySelector('h2'))).toBe('Mise à jour de GRAM Harness');
  });
});
