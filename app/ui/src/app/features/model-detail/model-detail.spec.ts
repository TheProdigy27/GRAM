import { TestBed } from '@angular/core/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import { argsOf, mockBackend } from '../../../testing/backend';
import {
  CALIBRATION,
  CALIBRATION_EMPTY,
  MEASUREMENT,
  RUNTIMES,
  SETTINGS,
  makeDetail,
} from '../../../testing/fixtures';
import type { Calibration } from '../../core/generated/Calibration';
import type { RuntimesStatus } from '../../core/generated/RuntimesStatus';
import { DetailStore } from '../../state/detail.store';
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

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

describe('ModelDetailPanel', () => {
  afterEach(() => clearMocks());

  it('montre toutes les quantifications, la meilleure sélectionnée et sa barre mémoire', async () => {
    mockBackend();
    const { el } = await open();

    expect(text(el.querySelector('[data-testid="detail-title"]'))).toBe('Qwen3-8B');
    const rows = [...el.querySelectorAll('[data-testid="variant-row"]')];
    expect(rows).toHaveLength(3);
    // Q8_0 est la meilleure (index 1) : étoile, ligne sélectionnée, explication.
    expect(rows[1].querySelector('.best')).not.toBeNull();
    expect(rows[1].classList).toContain('selected');
    expect(rows[2].classList).toContain('fit-bad'); // BF16 ne tient pas
    expect(rows[0].classList).toContain('fit-ok');
    expect(text(el.querySelector('[data-testid="explanation"]'))).toBe(
      'Tient entièrement en VRAM en Q8_0',
    );
    expect(el.querySelectorAll('app-memory-bar .segment')).toHaveLength(3);
    expect(text(el.querySelector('[data-testid="placement"]'))).toBe('VRAM 9,7 Go / 14,3 Go');
  });

  it('copie les commandes de la quantification choisie', async () => {
    const calls = mockBackend();
    const { fixture, el } = await open();

    const commands = () => [...el.querySelectorAll('[data-testid="command"]')].map(text);
    expect(commands()).toEqual([
      'ollama run hf.co/Qwen/Qwen3-8B-GGUF:Q8_0',
      '/set parameter num_ctx 8192',
      'setx OLLAMA_CONTEXT_LENGTH 8192',
    ]);

    // Choisir Q4_K_M dans le tableau change les commandes.
    (el.querySelector('[data-testid="variant-row"]') as HTMLElement).click();
    await fixture.whenStable();
    expect(commands()[0]).toBe('ollama run hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M');

    (el.querySelector('button[aria-label^="Copier : ollama run"]') as HTMLButtonElement).click();
    await fixture.whenStable();
    expect(argsOf(calls, 'plugin:clipboard-manager|write_text')[0]['text']).toBe(
      'ollama run hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M',
    );
  });

  it('ouvre la page Hugging Face du modèle et des dépôts alternatifs', async () => {
    const calls = mockBackend();
    const { fixture, el } = await open();

    (el.querySelector('[data-testid="open-hf"]') as HTMLButtonElement).click();
    const alt = [...el.querySelectorAll('.alternatives button')];
    (alt[0] as HTMLButtonElement).click();
    await fixture.whenStable();

    expect(argsOf(calls, 'plugin:opener|open_url').map((a) => a['url'])).toEqual([
      'https://huggingface.co/Qwen/Qwen3-8B-GGUF',
      'https://huggingface.co/bartowski/Qwen_Qwen3-8B-GGUF',
    ]);
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

    expect(el.querySelectorAll('[data-testid="command"]')).toHaveLength(0);
    expect(el.textContent).toContain('régler la longueur de contexte sur 8 192 tokens');
    (el.querySelector('[data-testid="open-launch-url"]') as HTMLButtonElement).click();
    await fixture.whenStable();
    expect(argsOf(calls, 'plugin:opener|open_url')).toHaveLength(1);
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
    const panel = (el: HTMLElement) => el.querySelector('[data-testid="measure-panel"]');
    const click = async (
      el: HTMLElement,
      testid: string,
      fixture: { whenStable(): Promise<unknown> },
    ) => {
      (el.querySelector(`[data-testid="${testid}"]`) as HTMLButtonElement).click();
      await fixture.whenStable();
    };

    it('mesure la quantification installée, affiche l’écart puis recalibre', async () => {
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
            currentEfficiency: 0.48,
            changed: false,
            summary:
              'D’après 1 mesure : efficacité 0,48. Les réglages actuels correspondent déjà aux mesures.',
          };
          return { ...SETTINGS, estimate: { ...SETTINGS.estimate, efficiency: 0.48 } };
        },
      });
      const { fixture, el } = await open(RUNTIMES);

      // La meilleure (Q8_0) n'est pas installée ; Q4_K_M l'est (icône dans le tableau).
      const rows = [...el.querySelectorAll('[data-testid="variant-row"]')];
      expect(rows[0].querySelector('[data-testid="variant-installed"]')).not.toBeNull();
      expect(el.querySelectorAll('[data-testid="variant-installed"]')).toHaveLength(1);
      (rows[0] as HTMLElement).click();
      await fixture.whenStable();

      expect(text(panel(el)?.querySelector('[data-testid="measure"]'))).toContain('Mesurer');
      expect(panel(el)?.querySelector('[data-testid="calibration"]')).toBeNull();
      await click(el, 'measure', fixture);

      expect(argsOf(calls, 'ollama_measure')).toEqual([
        {
          request: { repoId: 'Qwen/Qwen3-8B-GGUF', label: 'Q4_K_M', useCase: 'general', ctx: 8192 },
        },
      ]);
      const table = el.querySelector('[data-testid="measurement"]');
      const cells = [...(table?.querySelectorAll('tbody tr') ?? [])].map((r) =>
        [...r.querySelectorAll('th, td')].map(text),
      );
      expect(cells).toEqual([
        ['Vitesse (tok/s)', '83', '~95', '+14 %'],
        ['Mémoire', '6,5 Go', '6,3 Go', '−2 %'],
        ['dont VRAM', '6,5 Go', '6,3 Go', ''],
      ]);
      expect(text(panel(el)?.querySelector('[data-testid="measure"]'))).toContain(
        'Mesurer à nouveau',
      );
      expect(text(el.querySelector('[data-testid="calibration"] p'))).toBe(
        'D’après 1 mesure : efficacité 0,48 (actuelle 0,55).',
      );

      const detailCalls = argsOf(calls, 'get_model_detail').length;
      await click(el, 'apply-calibration', fixture);
      expect(argsOf(calls, 'apply_calibration')).toHaveLength(1);
      // Paramètres changés : la fiche est recalculée, la proposition relue.
      expect(argsOf(calls, 'get_model_detail').length).toBeGreaterThan(detailCalls);
      expect(el.querySelector('[data-testid="apply-calibration"]')).toBeNull();
      expect(text(el.querySelector('[data-testid="calibration"] p'))).toContain(
        'correspondent déjà aux mesures',
      );

      await click(el, 'clear-measurements', fixture);
      expect(argsOf(calls, 'clear_measurements')).toHaveLength(1);
      expect(el.querySelector('[data-testid="calibration"]')).toBeNull();
    });

    it('quantification non installée : commande d’installation, puis actualisation', async () => {
      const calls = mockBackend();
      const { fixture, el } = await open(RUNTIMES);

      // Q8_0, sélectionnée à l'ouverture, n'est pas installée.
      expect(panel(el)?.querySelector('[data-testid="measure"]')).toBeNull();
      expect(text(el.querySelector('[data-testid="pull-command"]'))).toBe(
        'ollama pull hf.co/Qwen/Qwen3-8B-GGUF:Q8_0',
      );
      (el.querySelector('button[aria-label^="Copier : ollama pull"]') as HTMLButtonElement).click();
      await fixture.whenStable();
      expect(argsOf(calls, 'plugin:clipboard-manager|write_text')[0]['text']).toBe(
        'ollama pull hf.co/Qwen/Qwen3-8B-GGUF:Q8_0',
      );

      const detailCalls = argsOf(calls, 'get_model_detail').length;
      await click(el, 'refresh-ollama', fixture);
      expect(argsOf(calls, 'detect_runtimes')).toHaveLength(1);
      expect(argsOf(calls, 'get_model_detail').length).toBeGreaterThan(detailCalls);
    });

    it('Ollama arrêté : invite à le lancer', async () => {
      mockBackend();
      const stopped = { ...RUNTIMES, ollama: { ...RUNTIMES.ollama, running: false } };
      const { el } = await open(stopped);
      expect(text(el.querySelector('[data-testid="ollama-stopped"]'))).toContain(
        'Ollama n’est pas lancé',
      );
      expect(el.querySelector('[data-testid="measure"]')).toBeNull();
    });

    it('sans Ollama installé, pas de panneau de mesure', async () => {
      mockBackend();
      const absent: RuntimesStatus = {
        ...RUNTIMES,
        ollama: { installed: false, running: false, path: null, version: null },
        ollamaModels: [],
      };
      const { el } = await open(absent);
      expect(el.querySelector('[data-testid="detail-title"]')).not.toBeNull();
      expect(panel(el)).toBeNull();
    });

    it('affiche l’erreur de la mesure', async () => {
      mockBackend({
        ollama_measure: () => {
          throw 'Ollama ne répond pas sur http://127.0.0.1:11434 : le lancer, puis réessayer';
        },
      });
      const { fixture, el } = await open(RUNTIMES);
      (el.querySelector('[data-testid="variant-row"]') as HTMLElement).click();
      await fixture.whenStable();
      await click(el, 'measure', fixture);
      expect(text(panel(el)?.querySelector('[role="alert"]'))).toBe(
        'Mesure impossible : Ollama ne répond pas sur http://127.0.0.1:11434 : le lancer, puis réessayer',
      );
      expect(panel(el)?.querySelector('[data-testid="measure"]')?.hasAttribute('disabled')).toBe(
        false,
      );
    });
  });
});
