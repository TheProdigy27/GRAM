import { TestBed } from '@angular/core/testing';
import { emit } from '@tauri-apps/api/event';
import { clearMocks } from '@tauri-apps/api/mocks';
import { type Handler, argsOf, mockBackend } from '../../../testing/backend';
import { byTestId, click, queryTestId, setChecked, text } from '../../../testing/dom';
import {
  ADD_OUTCOME,
  HARNESS_STATUS_ADDED,
  HARNESS_STATUS_EMPTY,
  INSTALL_PROGRESS,
  PREFLIGHT,
  PREFLIGHT_DOWNLOAD,
  RUNTIMES,
  SHARED_MODEL,
} from '../../../testing/fixtures';
import type { AddOutcome } from '../../core/generated/AddOutcome';
import type { HarnessStatus } from '../../core/generated/HarnessStatus';
import type { RuntimesStatus } from '../../core/generated/RuntimesStatus';
import type { SharedModel } from '../../core/generated/SharedModel';
import { DetailStore } from '../../state/detail.store';
import { HardwareStore } from '../../state/hardware.store';
import { FILTER_DEBOUNCE_MS } from '../../state/ranking.store';
import { ModelDetailPanel } from './model-detail';

/** Q8_0 ajouté à GRAM Harness, base téléchargée par GRAM. */
const Q8_MODEL: SharedModel = {
  ...SHARED_MODEL,
  ollamaName: 'gram/qwen3-8b:q8_0',
  baseName: 'hf.co/Qwen/Qwen3-8B-GGUF:Q8_0',
  baseInstalledByGram: true,
  quant: 'Q8_0',
  size: PREFLIGHT_DOWNLOAD.size,
  estimatedTokS: 62,
  measuredTokS: null,
};

/** Fiche de Qwen3-8B, Q8_0 (pas encore dans Ollama) choisie à l'ouverture. */
async function open(
  handlers: Record<string, Handler> = {},
  runtimes: RuntimesStatus | null = RUNTIMES,
) {
  const calls = mockBackend(handlers);
  TestBed.configureTestingModule({
    imports: [ModelDetailPanel],
    providers: [{ provide: FILTER_DEBOUNCE_MS, useValue: 0 }],
  });
  TestBed.inject(HardwareStore).runtimes.set(runtimes);
  const fixture = TestBed.createComponent(ModelDetailPanel);
  const detail = TestBed.inject(DetailStore);
  detail.open('Qwen/Qwen3-8B-GGUF');
  await fixture.whenStable();
  const el = fixture.nativeElement as HTMLElement;
  return { calls, fixture, el, detail };
}

/** Attend une promesse du backend résolue par le test (non suivie par Angular). */
async function settle(fixture: { whenStable(): Promise<unknown> }) {
  await new Promise((resolve) => setTimeout(resolve));
  await fixture.whenStable();
}

/** `harness_add` qui attend `finish` (ajout en cours). */
function pendingAdd() {
  let finish: (outcome: AddOutcome | null) => void = () => undefined;
  let fail: (message: string) => void = () => undefined;
  const handler: Handler = () =>
    new Promise((resolve, reject) => {
      finish = resolve;
      fail = reject;
    });
  return { handler, finish: (o: AddOutcome | null) => finish(o), fail: (m: string) => fail(m) };
}

const toast = () => text(document.querySelector('.g-toast__message'));
const panel = (el: HTMLElement) => byTestId('harness-panel', el);
const addButton = (el: HTMLElement) => byTestId<HTMLButtonElement>('harness-add', el);

describe('HarnessPanel (Discuter avec)', () => {
  afterEach(() => {
    clearMocks();
    localStorage.clear();
  });

  it('pas encore ajoutée : faits, ajout suivi par événements, puis ajoutée', async () => {
    const add = pendingAdd();
    let status: HarnessStatus = HARNESS_STATUS_EMPTY;
    const { calls, fixture, el } = await open({
      harness_status: () => status,
      harness_add: add.handler,
    });

    expect(text(panel(el).querySelector('h3'))).toBe('Discuter avec Q8_0');
    expect(text(byTestId('harness-facts', el))).toBe(
      'Télécharge 8,1 Go dans Ollama · 1 To libres sur C:\\ · contexte 8k',
    );
    expect(argsOf(calls, 'harness_preflight').at(-1)).toEqual({
      request: { repoId: 'Qwen/Qwen3-8B-GGUF', label: 'Q8_0', useCase: 'general', ctx: 8192 },
    });
    expect(queryTestId('harness-warnings', el)).toBeNull();
    expect(addButton(el).disabled).toBe(false);

    await click(fixture, 'harness-add');
    expect(argsOf(calls, 'harness_add')).toEqual([
      { request: { repoId: 'Qwen/Qwen3-8B-GGUF', label: 'Q8_0', useCase: 'general', ctx: 8192 } },
    ]);
    // Retour immédiat, avant le premier événement ; le focus passe à « Annuler ».
    expect(text(byTestId('harness-step', el))).toBe('Préparation…');
    const bar = byTestId('harness-progress', el).querySelector('[role="progressbar"]')!;
    expect(bar.hasAttribute('aria-valuenow')).toBe(false);
    expect(document.activeElement).toBe(byTestId('harness-cancel', el));
    expect(queryTestId('harness-add', el)).toBeNull();

    await emit('harness-progress', INSTALL_PROGRESS);
    await fixture.whenStable();
    expect(text(byTestId('harness-step', el))).toBe('Téléchargement · 3,1 / 8,1 Go · 38 Mo/s');
    expect(text(byTestId('harness-progress', el))).toContain('38 %');
    const determinate = byTestId('harness-progress', el).querySelector('[role="progressbar"]')!;
    expect(Number(determinate.getAttribute('aria-valuenow'))).toBeCloseTo(38.27, 1);

    await emit('harness-progress', { ...INSTALL_PROGRESS, phase: 'creating', bytesPerS: null });
    await fixture.whenStable();
    expect(text(byTestId('harness-step', el))).toBe('Réglage du contexte…');

    const runtimesBefore = argsOf(calls, 'detect_runtimes').length;
    status = { models: [{ model: Q8_MODEL, baseRemovable: true }], install: null, presence: null };
    add.finish({ model: Q8_MODEL, warnings: [] });
    await settle(fixture);

    expect(toast()).toBe('Qwen3-8B Q8_0 ajouté à GRAM Harness.');
    expect(queryTestId('harness-progress', el)).toBeNull();
    expect(text(byTestId('harness-tag', el))).toBe('Dans GRAM Harness');
    expect(text(byTestId('harness-name', el))).toBe('gram/qwen3-8b:q8_0');
    expect(text(byTestId('harness-added', el))).toContain(
      'gram/qwen3-8b:q8_0 · contexte 8k · ~62 tok/s estimés',
    );
    expect(text(byTestId('harness-command', el))).toBe('ollama run gram/qwen3-8b:q8_0');
    // Modèles d'Ollama relus (la fiche voit la base installée) ; focus rendu au titre.
    expect(argsOf(calls, 'detect_runtimes').length).toBeGreaterThan(runtimesBefore);
    expect(document.activeElement?.id).toBe('harness-title');
  });

  it('déjà dans Ollama : rien à télécharger', async () => {
    const { fixture, el, detail } = await open();
    detail.select('Q4_K_M');
    await fixture.whenStable();
    expect(text(panel(el).querySelector('h3'))).toBe('Discuter avec Q4_K_M');
    expect(text(byTestId('harness-facts', el))).toBe('Déjà dans Ollama · contexte 8k');
  });

  it('avertissements du préflight ; trop gros : bouton désactivé avec la raison', async () => {
    const { fixture, el, detail } = await open({
      harness_preflight: (a) =>
        (a['request'] as { label: string }).label === 'Q8_0'
          ? {
              ...PREFLIGHT_DOWNLOAD,
              warnings: ['Dépôt protégé : accepter ses conditions.', 'Lent sur cette machine.'],
            }
          : { ...PREFLIGHT_DOWNLOAD, label: 'BF16', blocked: 'BF16 ne tient pas en mémoire.' },
    });
    const warnings = byTestId('harness-warnings', el).querySelectorAll('g-callout');
    expect([...warnings].map(text)).toEqual([
      'Dépôt protégé : accepter ses conditions.',
      'Lent sur cette machine.',
    ]);
    expect(queryTestId('harness-blocked', el)).toBeNull();

    detail.select('BF16');
    await fixture.whenStable();
    expect(addButton(el).disabled).toBe(true);
    expect(text(byTestId('harness-blocked', el))).toBe('BF16 ne tient pas en mémoire.');
    expect(addButton(el).getAttribute('aria-describedby')).toBe('harness-blocked');
  });

  it('un ajout en cours sur une autre quantification désactive le bouton, avec son nom', async () => {
    const { el } = await open({
      harness_status: () => ({
        ...HARNESS_STATUS_EMPTY,
        install: { ...INSTALL_PROGRESS, label: 'Q4_K_M', ollamaName: 'gram/qwen3-8b:q4_k_m' },
      }),
    });
    expect(queryTestId('harness-progress', el)).toBeNull();
    expect(addButton(el).disabled).toBe(true);
    expect(text(byTestId('harness-blocked', el))).toBe(
      'Ajout de Qwen3-8B Q4_K_M en cours : attendre sa fin, ou l’annuler.',
    );
  });

  it('reprend la progression d’un ajout lancé avant un rechargement', async () => {
    const { el } = await open({
      harness_status: () => ({ ...HARNESS_STATUS_EMPTY, install: INSTALL_PROGRESS }),
    });
    expect(text(byTestId('harness-step', el))).toBe('Téléchargement · 3,1 / 8,1 Go · 38 Mo/s');
    expect(queryTestId('harness-cancel', el)).not.toBeNull();
  });

  it('annule l’ajout : Ollama garde la partie téléchargée', async () => {
    const add = pendingAdd();
    const { calls, fixture, el } = await open({
      harness_add: add.handler,
      harness_cancel: () => true,
    });
    await click(fixture, 'harness-add');
    await emit('harness-progress', INSTALL_PROGRESS);
    await fixture.whenStable();

    await click(fixture, 'harness-cancel');
    expect(argsOf(calls, 'harness_cancel')).toHaveLength(1);
    expect(text(byTestId('harness-cancel', el))).toBe('Annulation…');

    add.finish(null);
    await settle(fixture);
    expect(toast()).toBe(
      'Ajout de Qwen3-8B Q8_0 annulé. Ollama garde la partie téléchargée : un nouvel ajout reprendra là.',
    );
    expect(queryTestId('harness-progress', el)).toBeNull();
    expect(addButton(el).disabled).toBe(false);
  });

  it('échec : toast et message dans la section', async () => {
    const message =
      'Hugging Face refuse Q8_0 : dépôt protégé. Accepter ses conditions, puis réessayer.';
    const { fixture, el } = await open({
      harness_add: () => {
        throw message;
      },
    });
    await click(fixture, 'harness-add');
    await settle(fixture);
    expect(toast()).toBe(`Ajout de Qwen3-8B Q8_0 impossible : ${message}`);
    expect(text(byTestId('harness-error', el))).toBe(`Ajout impossible : ${message}`);
    expect(addButton(el).disabled).toBe(false);
  });

  it('ajoutée à un autre contexte : « Régler le contexte » refait le modèle dérivé', async () => {
    const { calls, fixture, el, detail } = await open({
      harness_status: () => HARNESS_STATUS_ADDED,
      harness_preflight: () => ({ ...PREFLIGHT, ctx: 16384 }),
      harness_add: () => ({ ...ADD_OUTCOME, model: { ...SHARED_MODEL, ctx: 16384 } }),
    });
    detail.select('Q4_K_M');
    await fixture.whenStable();

    expect(text(byTestId('harness-added', el))).toContain(
      'gram/qwen3-8b:q4_k_m · contexte 8k · ~95 tok/s estimés · 83 mesurés',
    );
    expect(text(byTestId('harness-ctx', el))).toContain(
      'Réglé au contexte 8k ; l’estimation actuelle retient 16k.',
    );
    await click(fixture, 'harness-ctx-update');
    await settle(fixture);
    expect(toast()).toBe('Qwen3-8B Q4_K_M réglé au contexte 16k dans GRAM Harness.');
    expect(argsOf(calls, 'harness_add')).toEqual([
      { request: { repoId: 'Qwen/Qwen3-8B-GGUF', label: 'Q4_K_M', useCase: 'general', ctx: 8192 } },
    ]);
  });

  it('retirer : confirmation, base gardée quand GRAM ne l’a pas téléchargée', async () => {
    let status: HarnessStatus = HARNESS_STATUS_ADDED;
    const { calls, fixture, el, detail } = await open({ harness_status: () => status });
    detail.select('Q4_K_M');
    await fixture.whenStable();
    expect(queryTestId('harness-ctx', el)).toBeNull();

    await click(fixture, 'harness-remove');
    const dialog = document.querySelector<HTMLElement>('[role="dialog"]')!;
    expect(text(dialog.querySelector('h2'))).toBe('Retirer de GRAM Harness');
    expect(text(dialog)).toContain('Supprime gram/qwen3-8b:q4_k_m d’Ollama');
    expect(queryTestId('harness-remove-base')).toBeNull();
    expect(text(byTestId('harness-base-kept'))).toBe(
      'hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M reste dans Ollama : il y était avant l’ajout.',
    );
    await click(fixture, byTestId('harness-remove-cancel'));
    expect(argsOf(calls, 'harness_remove')).toHaveLength(0);
    expect(document.querySelector('[role="dialog"]')).toBeNull();

    await click(fixture, 'harness-remove');
    status = HARNESS_STATUS_EMPTY;
    await click(fixture, byTestId('harness-remove-confirm'));
    await settle(fixture);
    expect(argsOf(calls, 'harness_remove')).toEqual([
      { ollamaName: 'gram/qwen3-8b:q4_k_m', removeBase: false },
    ]);
    expect(toast()).toBe('Qwen3-8B Q4_K_M retiré de GRAM Harness.');
    expect(queryTestId('harness-added', el)).toBeNull();
    expect(queryTestId('harness-add', el)).not.toBeNull();
  });

  it('retirer : la base téléchargée par GRAM peut être supprimée aussi', async () => {
    const { calls, fixture, detail } = await open({
      harness_status: () => ({
        ...HARNESS_STATUS_EMPTY,
        models: [{ model: { ...SHARED_MODEL, baseInstalledByGram: true }, baseRemovable: true }],
      }),
      harness_remove: () => ({ removed: true, baseRemoved: true }),
    });
    detail.select('Q4_K_M');
    await fixture.whenStable();

    await click(fixture, 'harness-remove');
    expect(text(byTestId('harness-remove-base'))).toBe(
      'Supprimer aussi le modèle d’Ollama (4,7 Go)',
    );
    await setChecked(fixture, byTestId('harness-remove-base'), true);
    await click(fixture, byTestId('harness-remove-confirm'));
    await settle(fixture);
    expect(argsOf(calls, 'harness_remove')).toEqual([
      { ollamaName: 'gram/qwen3-8b:q4_k_m', removeBase: true },
    ]);
    expect(toast()).toBe(
      'Qwen3-8B Q4_K_M retiré de GRAM Harness. hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M est supprimé d’Ollama (4,7 Go libérés).',
    );
  });

  it('Ollama arrêté : invite à le lancer, sans préflight', async () => {
    const stopped = { ...RUNTIMES, ollama: { ...RUNTIMES.ollama, running: false } };
    const { calls, el } = await open({}, stopped);
    expect(text(byTestId('harness-ollama-stopped', el))).toContain('Ollama n’est pas lancé');
    expect(argsOf(calls, 'harness_preflight')).toHaveLength(0);
    expect(queryTestId('harness-add', el)).toBeNull();
  });

  it('Ollama absent : invite à l’installer, puis actualiser', async () => {
    const { calls, fixture, el } = await open({}, null);
    expect(text(byTestId('harness-ollama-absent', el))).toContain('Ollama, qui n’est pas installé');
    await click(fixture, 'harness-refresh');
    expect(argsOf(calls, 'detect_runtimes')).toHaveLength(1);
    // Ollama détecté et lancé : le préflight part (puis repart avec la fiche recalculée).
    expect(argsOf(calls, 'harness_preflight').length).toBeGreaterThan(0);
    expect(queryTestId('harness-add', el)).not.toBeNull();
  });

  it('préflight impossible : message et Actualiser', async () => {
    const { el } = await open({
      harness_preflight: () => {
        throw 'Ollama ne répond pas sur http://127.0.0.1:11434 : le lancer, puis réessayer';
      },
    });
    expect(text(panel(el).querySelector('[role="alert"]'))).toBe(
      'Vérification impossible : Ollama ne répond pas sur http://127.0.0.1:11434 : le lancer, puis réessayer',
    );
    expect(queryTestId('harness-refresh', el)).not.toBeNull();
  });
});
