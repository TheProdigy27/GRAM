import { TestBed } from '@angular/core/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import { argsOf, mockBackend } from '../../../testing/backend';
import { makeDetail } from '../../../testing/fixtures';
import { DetailStore } from '../../state/detail.store';
import { FILTER_DEBOUNCE_MS } from '../../state/ranking.store';
import { ModelDetailPanel } from './model-detail';

async function open() {
  TestBed.configureTestingModule({
    imports: [ModelDetailPanel],
    providers: [{ provide: FILTER_DEBOUNCE_MS, useValue: 0 }],
  });
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
});
