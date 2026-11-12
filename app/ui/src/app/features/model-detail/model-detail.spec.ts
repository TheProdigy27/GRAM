import { TestBed } from '@angular/core/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import { argsOf, mockBackend } from '../../../testing/backend';
import {
  allByTestId,
  byTestId,
  chooseOption,
  click,
  press,
  queryTestId,
  text,
} from '../../../testing/dom';
import {
  CALIBRATION,
  CALIBRATION_EMPTY,
  GIB,
  MEASUREMENT,
  RUNTIMES,
  SETTINGS,
  makeDetail,
  makeEstimate,
  makeScore,
} from '../../../testing/fixtures';
import type { Calibration } from '../../core/generated/Calibration';
import type { RuntimesStatus } from '../../core/generated/RuntimesStatus';
import { DetailStore } from '../../state/detail.store';
import { DisplayStore } from '../../state/display.store';
import { HardwareStore } from '../../state/hardware.store';
import { FILTER_DEBOUNCE_MS } from '../../state/ranking.store';
import { ModelDetailPanel } from './model-detail';

async function open(runtimes: RuntimesStatus | null = null) {
  TestBed.configureTestingModule({
    imports: [ModelDetailPanel],
    providers: [{ provide: FILTER_DEBOUNCE_MS, useValue: 0 }],
  });
  TestBed.inject(HardwareStore).runtimes.set(runtimes);
  const fixture = TestBed.createComponent(ModelDetailPanel);
  TestBed.inject(DetailStore).open('Qwen/Qwen3-8B-GGUF');
  await fixture.whenStable();
  return { fixture, el: fixture.nativeElement as HTMLElement };
}

const bars = (el: HTMLElement) => allByTestId('quant-bar', el);
const selectedBar = (el: HTMLElement) =>
  bars(el).find((b) => b.getAttribute('aria-selected') === 'true')?.dataset['label'];
const commands = (el: HTMLElement) => allByTestId('command', el).map(text);
const toast = () => text(document.querySelector('.g-toast__message'));

/** Fiche où la meilleure quantification (Q8_0) est hybride et Q4_K_M tient en VRAM. */
function hybridDetail() {
  const d = makeDetail();
  const [q4, q8] = d.variants;
  q4.score = makeScore(82, { quality: 82 });
  q8.score = makeScore(86, { quality: 86 });
  q8.estimate = makeEstimate({
    weights: 13.6 * GIB,
    kv: 0.5 * GIB,
    mode: { kind: 'hybrid', gpuLayers: 33, layers: 36 },
    tokS: 30,
    ctxMax: 65536,
  });
  q8.explanation = 'Hybride : 33 couches sur 36 en VRAM';
  return d;
}

describe('ModelDetailPanel', () => {
  afterEach(() => {
    clearMocks();
    localStorage.clear();
  });

  it('en-tête, tuiles, graphique des vitesses : la meilleure quantification choisie', async () => {
    mockBackend();
    const { el } = await open();

    expect(text(byTestId('detail-title', el))).toBe('Qwen3-8B');
    // Premier du classement (fixture RANKING), score de la meilleure quantification.
    expect(text(byTestId('detail-standing', el))).toBe('n° 1 · score 86');
    expect([...byTestId('detail-tags', el).querySelectorAll('g-tag')].map(text)).toEqual([
      'Code',
      'Raisonnement',
    ]);
    const tile = (key: string) => [
      text(byTestId(`tile-${key}`, el).querySelector('.tile__label')),
      text(byTestId(`tile-${key}`, el).querySelector('.tile__value')),
    ];
    expect(['params', 'ctx', 'downloads', 'arch'].map(tile)).toEqual([
      ['Paramètres', '8,2 Md'],
      ['Contexte d’entraînement', '40k'],
      ['Téléchargements 30 j', '1,2 M'],
      ['Architecture', 'inconnue'],
    ]);

    // Une barre par quantification, couleur du mode ; Q8_0 (la meilleure) cerclée.
    expect(bars(el).map((b) => [b.dataset['label'], b.dataset['mode']])).toEqual([
      ['Q4_K_M', 'gpu'],
      ['Q8_0', 'gpu'],
      ['BF16', 'tooBig'],
    ]);
    expect(selectedBar(el)).toBe('Q8_0');
    expect(text(el.querySelector('.col--selected .col__tag'))).toBe('Q8_0 ~62 tok/s');
    expect(text(byTestId('vram-limit', el))).toBe('Limite VRAM · 14,3 Go');
    // L'infobulle de chaque barre (description accessible) détaille la quantification.
    const described = document.getElementById(bars(el)[1].getAttribute('aria-describedby')!);
    expect(described?.textContent).toBe(
      'Q8_0 · ★ meilleure pour cette machine\n8,1 Go · GPU\n~62 tok/s · contexte max 40k · score 86',
    );

    // Mémoire et explication de la quantification choisie ; pas de suggestion (tout en VRAM).
    expect(text(byTestId('explanation', el))).toBe('Tient entièrement en VRAM en Q8_0');
    expect(el.querySelectorAll('app-memory-bar .segment')).toHaveLength(3);
    expect(text(byTestId('placement', el))).toBe('VRAM 9,7 Go / 14,3 Go');
    expect(queryTestId('memory-ram', el)).toBeNull();
    expect(queryTestId('suggestion', el)).toBeNull();
  });

  it('clic et ← → sur les barres choisissent la quantification ; les commandes suivent', async () => {
    const calls = mockBackend();
    const { fixture, el } = await open();

    expect(commands(el)).toEqual([
      'ollama run hf.co/Qwen/Qwen3-8B-GGUF:Q8_0',
      '/set parameter num_ctx 8192',
      'setx OLLAMA_CONTEXT_LENGTH 8192',
    ]);
    expect(allByTestId('command', el).map((c) => text(c.closest('li')?.firstElementChild))).toEqual(
      [
        '1 · Lancer le modèle',
        '2 · Puis, dans la session',
        '3 · Ou, pour tous les modèles (puis relancer Ollama)',
      ],
    );

    await click(fixture, bars(el)[0]);
    expect(selectedBar(el)).toBe('Q4_K_M');
    expect(commands(el)[0]).toBe('ollama run hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M');
    // Une seule barre dans l'ordre de tabulation : la choisie.
    expect(bars(el).map((b) => b.tabIndex)).toEqual([0, -1, -1]);

    await press(fixture, bars(el)[0], 'ArrowRight');
    expect(selectedBar(el)).toBe('Q8_0');
    expect(document.activeElement).toBe(bars(el)[1]);
    await press(fixture, bars(el)[1], 'End');
    expect(selectedBar(el)).toBe('BF16');
    await press(fixture, bars(el)[2], 'Home');
    expect(selectedBar(el)).toBe('Q4_K_M');

    // Copier : presse-papiers, ✓ sur le bouton, toast.
    const copy = el.querySelector<HTMLButtonElement>('button[aria-label^="Copier : ollama run"]')!;
    await click(fixture, copy);
    expect(argsOf(calls, 'plugin:clipboard-manager|write_text')[0]['text']).toBe(
      'ollama run hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M',
    );
    expect(copy.querySelector('g-icon')?.getAttribute('data-icon')).toBe('check');
    expect(toast()).toBe('Commande copiée');
  });

  it('vue tableau : étoile, installée, choix au clavier, préférence gardée', async () => {
    mockBackend();
    const { fixture, el } = await open();

    await chooseOption(fixture, byTestId('quant-view', el), 'Tableau');
    expect(TestBed.inject(DisplayStore).quantView()).toBe('table');
    expect(bars(el)).toHaveLength(0);
    const rows = allByTestId('variant-row', el);
    expect(rows).toHaveLength(3);
    expect(rows.map((r) => r.dataset['fit'])).toEqual(['ok', 'ok', 'bad']);
    expect(queryTestId('variant-best', rows[1])).not.toBeNull();
    expect(text(queryTestId('variant-installed', rows[0]))).toBe('Installée');
    expect(rows[1].classList).toContain('is-selected');
    const buttons = allByTestId<HTMLButtonElement>('variant-select', el);
    expect(buttons.map((b) => b.getAttribute('aria-pressed'))).toEqual(['false', 'true', 'false']);

    await click(fixture, buttons[2]);
    expect(text(el.querySelector('#mem-title'))).toBe('Mémoire en BF16 au contexte 8k');
    expect(text(byTestId('placement', el))).toBe('Besoin 16,9 Go pour 61,6 Go utilisables');
  });

  it('suggère une quantification qui tient en VRAM quand la choisie est hybride', async () => {
    mockBackend({ get_model_detail: () => hybridDetail() });
    const { fixture, el } = await open();

    expect(text(byTestId('vram-limit', el))).toBe('Limite VRAM · 14,3 Go');
    // Hybride : la part en RAM est hachurée au bout de la barre mémoire.
    expect(queryTestId('memory-ram', el)).not.toBeNull();
    expect(text(byTestId('suggestion', el))).toBe(
      'Q4_K_M tient entièrement en VRAM : ~95 tok/s (+217 %) pour une qualité à peine moindre, ' +
        'mais 40k de contexte au plus. Choisir Q4_K_M',
    );

    await click(fixture, 'choose-suggestion');
    expect(selectedBar(el)).toBe('Q4_K_M');
    expect(queryTestId('suggestion', el)).toBeNull();
  });

  it('ouvre la page Hugging Face du modèle et des dépôts alternatifs', async () => {
    const calls = mockBackend();
    const { fixture, el } = await open();

    await click(fixture, 'open-hf');
    await click(fixture, byTestId('alternative', el));

    expect(argsOf(calls, 'plugin:opener|open_url').map((a) => a['url'])).toEqual([
      'https://huggingface.co/Qwen/Qwen3-8B-GGUF',
      'https://huggingface.co/bartowski/Qwen_Qwen3-8B-GGUF',
    ]);
    expect(text(byTestId('alternatives', el))).toBe(
      'bartowski/Qwen_Qwen3-8B-GGUF 85 k téléchargements',
    );
  });

  it('LM Studio : pas de commande, un bouton vers la page du dépôt', async () => {
    const detail = makeDetail();
    for (const v of detail.variants) {
      v.launch = {
        runtime: 'lmStudio',
        commands: [],
        openUrl: 'https://huggingface.co/Qwen/Qwen3-8B-GGUF',
        notes: ['Au chargement, régler la longueur de contexte sur 8 192 tokens.'],
      };
    }
    const calls = mockBackend({ get_model_detail: () => ({ ...detail, runtime: 'lmStudio' }) });
    const { fixture, el } = await open();

    expect(text(el.querySelector('#launch-title'))).toBe('Lancer Q8_0 avec LM Studio');
    expect(commands(el)).toHaveLength(0);
    expect(text(byTestId('launch-notes', el))).toContain(
      'régler la longueur de contexte sur 8 192 tokens',
    );
    await click(fixture, 'open-launch-url');
    expect(argsOf(calls, 'plugin:opener|open_url')).toHaveLength(1);
  });

  it('le bouton ✕ demande la fermeture à la coque', async () => {
    mockBackend();
    const { fixture } = await open();
    let dismissed = 0;
    fixture.componentInstance.dismiss.subscribe(() => dismissed++);
    await click(fixture, 'close-detail');
    expect(dismissed).toBe(1);
  });

  it('affiche l’erreur du backend', async () => {
    mockBackend({
      get_model_detail: () => {
        throw 'Modèle « x » absent du catalogue.';
      },
    });
    const { el } = await open();
    expect(text(el.querySelector('[role="alert"]'))).toBe(
      'Fiche indisponible : Modèle « x » absent du catalogue.',
    );
  });

  describe('mesure réelle avec Ollama', () => {
    const panel = (el: HTMLElement) => queryTestId('measure-panel', el);

    it('mesure la quantification installée, affiche l’écart en haltères puis recalibre', async () => {
      let calibration: Calibration = CALIBRATION_EMPTY;
      const calls = mockBackend({
        get_calibration: () => calibration,
        ollama_measure: () => {
          calibration = CALIBRATION;
          return MEASUREMENT;
        },
        apply_calibration: () => {
          calibration = {
            ...CALIBRATION,
            currentGpuEfficiency: 0.48,
            changed: false,
            summary:
              'D’après 1 mesure : efficacité GPU 0,48. Les réglages actuels correspondent déjà aux mesures.',
          };
          return { ...SETTINGS, estimate: { ...SETTINGS.estimate, gpuEfficiency: 0.48 } };
        },
      });
      const { fixture, el } = await open(RUNTIMES);

      // La meilleure (Q8_0) n'est pas installée ; Q4_K_M l'est.
      await click(fixture, bars(el)[0]);
      expect(text(panel(el)?.querySelector('.measure__head'))).toBe(
        'Mesure réelle de Q4_K_M avec Ollama Installée',
      );
      expect(text(byTestId('measure', el))).toBe('Mesurer');
      expect(queryTestId('calibration', el)).toBeNull();
      await click(fixture, 'measure');

      expect(argsOf(calls, 'ollama_measure')).toEqual([
        {
          request: { repoId: 'Qwen/Qwen3-8B-GGUF', label: 'Q4_K_M', useCase: 'general', ctx: 8192 },
        },
      ]);
      expect(toast()).toBe('Mesure terminée.');
      const speed = byTestId('dumbbell-speed', el);
      expect(text(byTestId('gap', speed))).toBe('+14 %');
      expect(text(byTestId('measured-value', speed))).toBe('83');
      expect(text(byTestId('estimated-value', speed))).toBe('95');
      expect(speed.querySelector('[role="img"]')?.getAttribute('aria-label')).toBe(
        'Vitesse mesurée 83 tokens par seconde, estimée 95 : écart +14 %.',
      );
      const memory = byTestId('dumbbell-memory', el);
      expect(text(byTestId('gap', memory))).toBe('−2 %');
      expect(text(memory.querySelector('.axis__marker'))).toBe('VRAM 14,3');
      expect(text(byTestId('measure-facts', el))).toContain(
        'contexte 8k · 128 tokens générés · chargement 2,7 s · prompt 1 476 tok/s',
      );
      expect(text(byTestId('measure', el))).toBe('Mesurer à nouveau');
      expect(text(byTestId('calibration-summary', el))).toBe(
        'D’après 1 mesure : efficacité GPU 0,48 (actuelle 0,81).',
      );

      const detailCalls = argsOf(calls, 'get_model_detail').length;
      await click(fixture, 'apply-calibration');
      expect(argsOf(calls, 'apply_calibration')).toHaveLength(1);
      // Paramètres changés : la fiche est recalculée, la proposition relue.
      expect(argsOf(calls, 'get_model_detail').length).toBeGreaterThan(detailCalls);
      expect(queryTestId('apply-calibration', el)).toBeNull();
      expect(text(byTestId('calibration-summary', el))).toContain('correspondent déjà aux mesures');

      await click(fixture, 'clear-measurements');
      expect(argsOf(calls, 'clear_measurements')).toHaveLength(1);
      expect(queryTestId('calibration', el)).toBeNull();
    });

    it('quantification non installée : commande d’installation, puis actualisation', async () => {
      const calls = mockBackend();
      const { fixture, el } = await open(RUNTIMES);

      // Q8_0, sélectionnée à l'ouverture, n'est pas installée.
      expect(queryTestId('measure', el)).toBeNull();
      expect(text(byTestId('pull-command', el))).toBe('ollama pull hf.co/Qwen/Qwen3-8B-GGUF:Q8_0');
      await click(
        fixture,
        el.querySelector<HTMLButtonElement>('button[aria-label^="Copier : ollama pull"]')!,
      );
      expect(argsOf(calls, 'plugin:clipboard-manager|write_text')[0]['text']).toBe(
        'ollama pull hf.co/Qwen/Qwen3-8B-GGUF:Q8_0',
      );

      const detailCalls = argsOf(calls, 'get_model_detail').length;
      await click(fixture, 'refresh-ollama');
      expect(argsOf(calls, 'detect_runtimes')).toHaveLength(1);
      expect(argsOf(calls, 'get_model_detail').length).toBeGreaterThan(detailCalls);
    });

    it('Ollama arrêté : invite à le lancer', async () => {
      mockBackend();
      const stopped = { ...RUNTIMES, ollama: { ...RUNTIMES.ollama, running: false } };
      const { el } = await open(stopped);
      expect(text(byTestId('ollama-stopped', el))).toContain('Ollama n’est pas lancé');
      expect(queryTestId('measure', el)).toBeNull();
    });

    it('sans Ollama installé, pas de panneau de mesure', async () => {
      mockBackend();
      const absent: RuntimesStatus = {
        ...RUNTIMES,
        ollama: { installed: false, running: false, path: null, version: null },
        ollamaModels: [],
      };
      const { el } = await open(absent);
      expect(queryTestId('detail-title', el)).not.toBeNull();
      expect(panel(el)).toBeNull();
    });

    it('affiche l’erreur de la mesure', async () => {
      mockBackend({
        ollama_measure: () => {
          throw 'Ollama ne répond pas sur http://127.0.0.1:11434 : le lancer, puis réessayer';
        },
      });
      const { fixture, el } = await open(RUNTIMES);
      await click(fixture, bars(el)[0]);
      await click(fixture, 'measure');
      expect(text(panel(el)?.querySelector('[role="alert"]'))).toBe(
        'Mesure impossible : Ollama ne répond pas sur http://127.0.0.1:11434 : le lancer, puis réessayer',
      );
      expect(byTestId('measure', el).hasAttribute('disabled')).toBe(false);
    });
  });
});
