import { TestBed } from '@angular/core/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import { type Handler, argsOf, mockBackend } from '../../../testing/backend';
import { allByTestId, byTestId, click, queryTestId, text } from '../../../testing/dom';
import {
  HARNESS_STATUS_ADDED,
  HARNESS_STATUS_EMPTY,
  INSTALL_PROGRESS,
  INSTALLED_MODELS,
  RUNTIMES,
  RUNTIMES_INSTALLED,
  SHARED_MODEL,
} from '../../../testing/fixtures';
import type { HarnessStatus } from '../../core/generated/HarnessStatus';
import type { RuntimesStatus } from '../../core/generated/RuntimesStatus';
import { DetailStore } from '../../state/detail.store';
import { HarnessStore } from '../../state/harness.store';
import { FILTER_DEBOUNCE_MS } from '../../state/ranking.store';
import { InstalledDialog } from './installed-dialog';

const HUIHUI = 'hf.co/huihui-ai/Huihui-DeepSeek-V4.1-Flash-1017-abliterated-GGUF:IQ1_S';
const QWEN_1_7 = 'hf.co/unsloth/Qwen3-1.7B-GGUF:Q4_K_M';
const BASE = 'hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M';

/**
 * Dialogue ouvert sur un Ollama qui a `runtimes` et un manifeste `status` ; tous deux suivent ce
 * que le test fait (suppression, retrait) par `ollama_delete` et `harness_remove`.
 */
async function open(
  handlers: Record<string, Handler> = {},
  initial: { runtimes?: RuntimesStatus; status?: HarnessStatus } = {},
) {
  const backend = {
    runtimes: initial.runtimes ?? RUNTIMES_INSTALLED,
    status: initial.status ?? HARNESS_STATUS_ADDED,
  };
  const forget = (name: string) => {
    backend.runtimes = {
      ...backend.runtimes,
      ollamaModels: backend.runtimes.ollamaModels.filter((m) => m.name !== name),
    };
  };
  const calls = mockBackend({
    detect_runtimes: () => structuredClone(backend.runtimes),
    harness_status: () => backend.status,
    ollama_delete: (a) => {
      forget(a['name'] as string);
      return true;
    },
    harness_remove: (a) => {
      forget(a['ollamaName'] as string);
      backend.status = HARNESS_STATUS_EMPTY;
      return { removed: true, baseRemoved: false };
    },
    ...handlers,
  });
  TestBed.configureTestingModule({
    imports: [InstalledDialog],
    providers: [{ provide: FILTER_DEBOUNCE_MS, useValue: 0 }],
  });
  const store = TestBed.inject(HarnessStore);
  const fixture = TestBed.createComponent(InstalledDialog);
  await settle(fixture);
  const el = fixture.nativeElement as HTMLElement;
  return { calls, fixture, el, store, backend };
}

/** Attend une promesse du backend (non suivie par Angular). */
async function settle(fixture: { whenStable(): Promise<unknown> }) {
  await new Promise((resolve) => setTimeout(resolve));
  await fixture.whenStable();
  await new Promise((resolve) => setTimeout(resolve));
  await fixture.whenStable();
}

const names = (root: HTMLElement, list: string) =>
  allByTestId(list === 'harness' ? 'installed-harness' : 'installed-others', root).flatMap((l) =>
    allByTestId('installed-row', l).map((r) => r.dataset['name']),
  );
const row = (root: HTMLElement, name: string) =>
  root.querySelector<HTMLElement>(`[data-testid="installed-row"][data-name="${name}"]`)!;
const toast = () => text(document.querySelector('.g-toast__message'));
const dialogs = () => [...document.querySelectorAll<HTMLElement>('[role="dialog"]')];

describe('InstalledDialog (Modèles installés)', () => {
  afterEach(() => clearMocks());

  it('ceux de GRAM Harness d’abord (base comprise), puis les autres ; relus à l’ouverture', async () => {
    const { calls, el } = await open();
    expect(argsOf(calls, 'detect_runtimes')).toHaveLength(1);
    expect(text(byTestId('installed-summary', el))).toBe('Dans Ollama · 4 modèles · 12,7 Go');
    expect(names(el, 'harness')).toEqual(['gram/qwen3-8b:q4_k_m']);
    expect(names(el, 'others')).toEqual([HUIHUI, QWEN_1_7, 'qwen3:8b']);

    const added = row(el, 'gram/qwen3-8b:q4_k_m');
    expect(text(added.querySelector('.row__title'))).toBe('Qwen3-8B Q4_K_M');
    expect(text(added.querySelector('.row__facts'))).toBe('gram/qwen3-8b:q4_k_m · contexte 8k');
    expect(text(added.querySelector('.row__size'))).toBe('4,7 Go');
    const remove = byTestId('installed-remove', added);
    expect(text(remove)).toBe('Retirer');
    expect(remove.getAttribute('aria-label')).toBe('Retirer Qwen3-8B Q4_K_M');

    const hf = row(el, QWEN_1_7);
    expect(text(hf.querySelector('.row__title'))).toBe('Qwen3-1.7B Q4_K_M');
    expect(text(hf.querySelector('.row__facts'))).toBe(QWEN_1_7);
    expect(byTestId('installed-delete', hf).getAttribute('aria-label')).toBe(
      `Supprimer ${QWEN_1_7}`,
    );
    // Bibliothèque d'Ollama : le nom en titre, la quantification dessous.
    const library = row(el, 'qwen3:8b');
    expect(text(library.querySelector('.row__title'))).toBe('qwen3:8b');
    expect(text(library.querySelector('.row__facts'))).toBe('Q4_K_M');
    expect(text(library.querySelector('.row__size'))).toBe('5,2 Go');
  });

  it('un clic sur une ligne ouvre la fiche, sur la quantification installée', async () => {
    const { fixture, el } = await open();
    const detail = TestBed.inject(DetailStore);
    const added = byTestId('installed-open', row(el, 'gram/qwen3-8b:q4_k_m'));
    expect(added.getAttribute('aria-label')).toBe('Ouvrir la fiche de Qwen3-8B Q4_K_M');
    expect(byTestId('installed-open', row(el, QWEN_1_7)).getAttribute('aria-label')).toBe(
      'Ouvrir la fiche de Qwen3-1.7B Q4_K_M',
    );
    // Hors du catalogue (dépôt inconnu, bibliothèque d'Ollama) : pas de fiche.
    expect(queryTestId('installed-open', row(el, HUIHUI))).toBeNull();
    expect(queryTestId('installed-open', row(el, 'qwen3:8b'))).toBeNull();

    await click(fixture, added);
    await settle(fixture);
    expect(detail.repoId()).toBe('Qwen/Qwen3-8B-GGUF');
    // La meilleure quantification de la fiche est Q8_0 : c'est bien celle installée qui est choisie.
    expect(detail.selectedLabel()).toBe('Q4_K_M');

    await click(fixture, byTestId('installed-open', row(el, QWEN_1_7)));
    expect(detail.repoId()).toBe('Qwen/Qwen3-1.7B-GGUF');
  });

  it('supprimer : confirmation, puis la ligne disparaît et le focus passe à la suivante', async () => {
    const { calls, fixture, el } = await open();
    await click(fixture, byTestId('installed-delete', row(el, HUIHUI)));
    const confirm = dialogs().at(-1)!;
    expect(text(confirm.querySelector('h2'))).toBe('Supprimer d’Ollama');
    expect(text(confirm)).toContain(`Supprime ${HUIHUI} d’Ollama et libère jusqu’à 1,7 Go.`);
    await click(fixture, byTestId('ollama-delete-cancel'));
    expect(argsOf(calls, 'ollama_delete')).toHaveLength(0);
    expect(dialogs()).toHaveLength(0);

    await click(fixture, byTestId('installed-delete', row(el, HUIHUI)));
    await click(fixture, byTestId('ollama-delete-confirm'));
    await settle(fixture);
    expect(argsOf(calls, 'ollama_delete')).toEqual([{ name: HUIHUI }]);
    expect(toast()).toBe(`${HUIHUI} est supprimé d’Ollama.`);
    expect(names(el, 'others')).toEqual([QWEN_1_7, 'qwen3:8b']);
    expect(text(byTestId('installed-summary', el))).toBe('Dans Ollama · 3 modèles · 11,0 Go');
    expect(document.activeElement).toBe(byTestId('installed-delete', row(el, QWEN_1_7)));
    // Ollama relu ensuite (fiche et repère « Installée » recalculés).
    expect(argsOf(calls, 'detect_runtimes')).toHaveLength(2);
  });

  it('retirer : le dialogue de la fiche ; la base gardée réapparaît parmi les autres', async () => {
    const { calls, fixture, el } = await open();
    await click(fixture, byTestId('installed-remove', row(el, 'gram/qwen3-8b:q4_k_m')));
    const dialog = dialogs().at(-1)!;
    expect(text(dialog.querySelector('h2'))).toBe('Retirer de GRAM Harness');
    expect(text(byTestId('harness-base-kept'))).toBe(
      `${BASE} reste dans Ollama : il y était avant l’ajout.`,
    );
    await click(fixture, byTestId('harness-remove-confirm'));
    await settle(fixture);
    expect(argsOf(calls, 'harness_remove')).toEqual([
      { ollamaName: 'gram/qwen3-8b:q4_k_m', removeBase: false },
    ]);
    expect(toast()).toBe('Qwen3-8B Q4_K_M retiré de GRAM Harness.');
    expect(queryTestId('installed-harness', el)).toBeNull();
    expect(text(el.querySelector('#installed-others-title'))).toBe('Modèles d’Ollama');
    expect(names(el, 'others')).toEqual([HUIHUI, BASE, QWEN_1_7, 'qwen3:8b']);
    expect(document.activeElement).toBe(byTestId('installed-delete', row(el, HUIHUI)));
  });

  it('une suppression à la fois ; un modèle en cours d’ajout ne se retire pas', async () => {
    let finish: (v: boolean) => void = () => undefined;
    const { fixture, el, store } = await open({
      ollama_delete: () => new Promise((resolve) => (finish = resolve)),
    });
    await click(fixture, byTestId('installed-delete', row(el, 'qwen3:8b')));
    await click(fixture, byTestId('ollama-delete-confirm'));
    const busy = byTestId<HTMLButtonElement>('installed-delete', row(el, 'qwen3:8b'));
    expect(busy.getAttribute('aria-busy')).toBe('true');
    const others = allByTestId<HTMLButtonElement>('installed-delete', el).filter((b) => b !== busy);
    expect(others.every((b) => b.disabled)).toBe(true);
    expect(byTestId<HTMLButtonElement>('installed-remove', el).disabled).toBe(true);
    finish(true);
    await settle(fixture);
    expect(allByTestId<HTMLButtonElement>('installed-delete', el).some((b) => b.disabled)).toBe(
      false,
    );
    expect(byTestId<HTMLButtonElement>('installed-remove', el).disabled).toBe(false);

    // « Régler le contexte » refait ce modèle dérivé.
    store.install.set({
      ...INSTALL_PROGRESS,
      label: 'Q4_K_M',
      ollamaName: SHARED_MODEL.ollamaName,
    });
    await fixture.whenStable();
    expect(byTestId<HTMLButtonElement>('installed-remove', el).disabled).toBe(true);
    expect(text(byTestId('installed-installing', el))).toBe('· ajout en cours');
  });

  it('échec : message d’Ollama, la ligne reste', async () => {
    const { calls, fixture, el } = await open({
      ollama_delete: () => {
        throw 'Ollama a répondu 500 : modèle en cours d’utilisation';
      },
    });
    await click(fixture, byTestId('installed-delete', row(el, 'qwen3:8b')));
    await click(fixture, byTestId('ollama-delete-confirm'));
    await settle(fixture);
    expect(argsOf(calls, 'ollama_delete')).toHaveLength(1);
    expect(toast()).toBe(
      'Suppression impossible : Ollama a répondu 500 : modèle en cours d’utilisation',
    );
    expect(names(el, 'others')).toContain('qwen3:8b');
  });

  it('Ollama arrêté, absent, vide ; « Actualiser » relit', async () => {
    const stopped = {
      ...RUNTIMES,
      ollama: { ...RUNTIMES.ollama, running: false },
      ollamaModels: [],
    };
    const { calls, fixture, el, backend } = await open({}, { runtimes: stopped });
    expect(text(byTestId('installed-stopped', el))).toContain('Ollama est arrêté');
    expect(queryTestId('installed-summary', el)).toBeNull();
    expect(queryTestId('installed-row', el)).toBeNull();

    backend.runtimes = { ...RUNTIMES, ollamaModels: [] };
    await click(fixture, 'installed-refresh');
    await settle(fixture);
    expect(argsOf(calls, 'detect_runtimes')).toHaveLength(2);
    expect(text(byTestId('installed-summary', el))).toBe('Dans Ollama · aucun modèle');
    expect(text(byTestId('installed-empty', el))).toContain('Aucun modèle dans Ollama');

    backend.runtimes = {
      ...RUNTIMES,
      ollama: { installed: false, running: false, path: null, version: null },
      ollamaModels: [],
    };
    await click(fixture, 'installed-refresh');
    await settle(fixture);
    expect(text(byTestId('installed-absent', el))).toContain('Ollama n’est pas installé');

    backend.runtimes = { ...RUNTIMES, ollamaModels: INSTALLED_MODELS };
    await click(fixture, 'installed-refresh');
    await settle(fixture);
    expect(names(el, 'harness')).toEqual(['gram/qwen3-8b:q4_k_m']);
  });
});
