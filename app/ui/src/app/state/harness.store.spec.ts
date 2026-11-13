import { ApplicationRef } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { emit } from '@tauri-apps/api/event';
import { clearMocks } from '@tauri-apps/api/mocks';
import { type Handler, argsOf, mockBackend } from '../../testing/backend';
import {
  ADD_OUTCOME,
  HARNESS_STATUS_ADDED,
  HARNESS_STATUS_EMPTY,
  INSTALL_PROGRESS,
  RUNTIMES,
} from '../../testing/fixtures';
import type { AddOutcome } from '../core/generated/AddOutcome';
import type { HarnessStatus } from '../core/generated/HarnessStatus';
import type { InstallDone } from '../core/generated/InstallDone';
import { ToastService } from '../shared/ui/toast/toast';
import { HardwareStore } from './hardware.store';
import { HarnessStore } from './harness.store';
import { FILTER_DEBOUNCE_MS } from './ranking.store';

const Q4_REQUEST = {
  repoId: 'Qwen/Qwen3-8B-GGUF',
  label: 'Q4_K_M',
  useCase: 'general',
  ctx: 8192,
} as const;

const DONE_Q8: InstallDone = {
  repoId: INSTALL_PROGRESS.repoId,
  displayName: INSTALL_PROGRESS.displayName,
  label: INSTALL_PROGRESS.label,
  ollamaName: INSTALL_PROGRESS.ollamaName,
  result: { kind: 'added', warnings: [] },
};

async function setup(handlers: Record<string, Handler> = {}) {
  const calls = mockBackend(handlers);
  TestBed.configureTestingModule({ providers: [{ provide: FILTER_DEBOUNCE_MS, useValue: 0 }] });
  TestBed.inject(HardwareStore).runtimes.set(RUNTIMES);
  const store = TestBed.inject(HarnessStore);
  const toasts = vi.spyOn(TestBed.inject(ToastService), 'show');
  await settle();
  return { calls, store, toasts };
}

/** Effets, promesses du backend et événements. */
async function settle() {
  await new Promise((resolve) => setTimeout(resolve));
  await TestBed.inject(ApplicationRef).whenStable();
}

describe('HarnessStore', () => {
  afterEach(() => {
    clearMocks();
    vi.restoreAllMocks();
  });

  it('lit l’état au démarrage, puis à chaque détection des runtimes', async () => {
    const { calls, store } = await setup({
      harness_status: () => HARNESS_STATUS_ADDED,
      // Chaque détection donne un nouvel objet, comme le vrai backend.
      detect_runtimes: () => ({ ...RUNTIMES }),
    });
    expect(argsOf(calls, 'harness_status')).toHaveLength(1);
    expect(
      store.entryFor({ repoId: 'Qwen/Qwen3-8B-GGUF', label: 'q4_k_m' })?.model.ollamaName,
    ).toBe('gram/qwen3-8b:q4_k_m');
    expect(store.entryFor({ repoId: 'Qwen/Qwen3-8B-GGUF', label: 'Q8_0' })).toBeUndefined();

    await TestBed.inject(HardwareStore).refreshRuntimes();
    await settle();
    expect(argsOf(calls, 'harness_status')).toHaveLength(2);
  });

  it('ajout lancé avant un rechargement : progression reprise, fin annoncée par harness-done', async () => {
    let status: HarnessStatus = { ...HARNESS_STATUS_EMPTY, install: INSTALL_PROGRESS };
    const { store, toasts } = await setup({ harness_status: () => status });
    expect(store.install()).toEqual(INSTALL_PROGRESS);

    await emit('harness-progress', { ...INSTALL_PROGRESS, phase: 'verifying' });
    expect(store.install()?.phase).toBe('verifying');

    status = HARNESS_STATUS_EMPTY;
    await emit('harness-done', {
      ...DONE_Q8,
      result: { kind: 'failed', message: 'Disque plein.' },
    });
    await settle();
    expect(store.install()).toBeNull();
    expect(store.error()).toEqual({
      repoId: 'Qwen/Qwen3-8B-GGUF',
      label: 'Q8_0',
      message: 'Disque plein.',
    });
    expect(toasts.mock.calls.map(([m]) => m)).toEqual([
      'Ajout de Qwen3-8B Q8_0 impossible : Disque plein.',
    ]);

    // Un événement de progression en retard ne fait pas revenir l'ajout.
    await emit('harness-progress', INSTALL_PROGRESS);
    expect(store.install()).toBeNull();
  });

  it('ajout lancé ici : un seul toast, même si harness-done arrive avant ou après la réponse', async () => {
    let finish: (o: AddOutcome | null) => void = () => undefined;
    const { store, toasts } = await setup({
      harness_add: () => new Promise((resolve) => (finish = resolve)),
    });

    // harness-done avant la réponse.
    const first = store.add(Q4_REQUEST);
    expect(store.install()?.phase).toBe('preparing');
    await emit('harness-done', { ...DONE_Q8, label: 'Q4_K_M' });
    finish({ ...ADD_OUTCOME, warnings: ['Vision attendue mais absente dans Ollama.'] });
    await first;
    // … et après.
    await emit('harness-done', { ...DONE_Q8, label: 'Q4_K_M' });
    await settle();

    expect(toasts.mock.calls.map(([m]) => m)).toEqual([
      'Qwen3-8B Q4_K_M ajouté à GRAM Harness. Vision attendue mais absente dans Ollama.',
    ]);
    expect(store.addedWarnings()['gram/qwen3-8b:q4_k_m']).toEqual([
      'Vision attendue mais absente dans Ollama.',
    ]);
    expect(store.install()).toBeNull();
  });

  it('ajout refusé avant de commencer : erreur gardée, ajout en cours ailleurs conservé', async () => {
    const other = { ...INSTALL_PROGRESS };
    const { store, toasts } = await setup({
      harness_status: () => ({ ...HARNESS_STATUS_EMPTY, install: other }),
      harness_add: () => {
        throw 'Ajout de Qwen3-8B Q8_0 déjà en cours : attendre sa fin, ou l’annuler.';
      },
    });
    expect(await store.add(Q4_REQUEST)).toBeNull();
    expect(store.install()).toEqual(other);
    expect(store.error()?.label).toBe('Q4_K_M');
    expect(toasts).toHaveBeenCalledTimes(1);
  });

  it('annulation : sans ajout en cours, rien n’est demandé', async () => {
    const { calls, store } = await setup();
    await store.cancel();
    expect(argsOf(calls, 'harness_cancel')).toHaveLength(0);
  });
});
