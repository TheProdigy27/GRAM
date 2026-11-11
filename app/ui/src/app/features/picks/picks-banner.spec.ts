import { TestBed } from '@angular/core/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import { argsOf, mockBackend } from '../../../testing/backend';
import { allByTestId, byTestId, click, queryTestId, text } from '../../../testing/dom';
import { BUDGET, GIB, makeItem, makePick, makeResponse } from '../../../testing/fixtures';
import type { RankResponse } from '../../core/generated/RankResponse';
import { DetailStore } from '../../state/detail.store';
import { DisplayStore } from '../../state/display.store';
import { FILTER_DEBOUNCE_MS } from '../../state/ranking.store';
import { PicksBanner } from './picks-banner';

const QWEN = 'Qwen/Qwen3-8B-GGUF';

const best = makeItem('Gemma-3-12B', { rank: 1, score: 88.4, quant: 'Q8_0', size: 12.6 * GIB });
/** Le plus rapide dans une autre quantification que celle du classement (Q8_0). */
const fastest = makeItem('Qwen3-8B', {
  rank: 3,
  repoId: QWEN,
  score: 84,
  quant: 'Q4_K_M',
  tokS: 45,
  mode: { kind: 'hybrid', gpuLayers: 30, layers: 40 },
});
const longest = makeItem('Mistral-Small-24B', { rank: 5, score: 82, tokS: 7.4, ctxMax: 131072 });

const metrics = (card: HTMLElement) =>
  [...byTestId('pick-metrics', card).querySelectorAll('li')].map(text);

const PICKS = makeResponse([best, fastest, longest], {
  picks: [
    makePick('best', best),
    makePick('fastest', fastest),
    makePick('longestContext', longest),
  ],
});

describe('PicksBanner', () => {
  afterEach(() => {
    clearMocks();
    localStorage.clear();
  });

  async function render(response: RankResponse = PICKS) {
    const calls = mockBackend({ rank: () => response });
    TestBed.configureTestingModule({ providers: [{ provide: FILTER_DEBOUNCE_MS, useValue: 0 }] });
    const fixture = TestBed.createComponent(PicksBanner);
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    const card = (kind: string) => el.querySelector<HTMLElement>(`[data-kind="${kind}"]`)!;
    return { fixture, el, calls, card };
  }

  it('trois cartes : étiquette, nom, quantification et taille, trois mesures courtes', async () => {
    const { el, card } = await render();
    expect(text(el.querySelector('h2'))).toBe('Trois conseils');
    expect(text(byTestId('picks-summary', el))).toBe('Polyvalent · contexte 8k · Ollama');
    expect(allByTestId('pick', el).map((c) => text(c.querySelector('.pick__label')))).toEqual([
      'Meilleur choix',
      'Le plus rapide',
      'Le plus long contexte',
    ]);

    const b = card('best');
    expect(b.classList).toContain('pick--best');
    expect(text(byTestId('pick-name', b))).toBe('Gemma-3-12B');
    expect(text(byTestId('pick-quant', b))).toBe('Q8_0 · 12,6 Go');
    expect(text(byTestId('pick-score', b))).toBe('Score 88');
    expect(queryTestId('pick-rank', b)).toBeNull();
    expect(metrics(b)).toEqual(['~95 tok/s instantané', 'tout en VRAM', '40k de contexte max']);

    const f = card('fastest');
    expect(f.classList).not.toContain('pick--best');
    expect(text(byTestId('pick-rank', f))).toBe('n° 3');
    expect(text(byTestId('pick-quant', f))).toBe('Q4_K_M · 4,7 Go');
    expect(metrics(f)).toEqual([
      '~45 tok/s très fluide',
      '30/40 couches en VRAM',
      '40k de contexte max',
    ]);

    // Le plus long contexte : le contexte d'abord.
    expect(metrics(card('longestContext'))).toEqual([
      '128k de contexte max',
      '~7,4 tok/s correct',
      'tout en VRAM',
    ]);

    // Nom et raison du conseil pour les lecteurs d'écran.
    const labelledBy = f.getAttribute('aria-labelledby')!.split(' ');
    expect(labelledBy.map((id) => text(document.getElementById(id)))).toEqual([
      'Le plus rapide',
      'Qwen3-8B',
    ]);
    expect(text(document.getElementById(f.getAttribute('aria-describedby')!))).toBe(
      '~140 tok/s, tout en VRAM : 1,5× plus rapide que le meilleur choix',
    );
  });

  it('« Commande Ollama » copie la commande du conseil ; « Fiche » ouvre sa quantification', async () => {
    const { fixture, calls, card } = await render();
    const command = byTestId('pick-command', card('fastest'));
    expect(text(command)).toBe('Commande Ollama');
    await click(fixture, command);
    expect(argsOf(calls, 'plugin:clipboard-manager|write_text')[0]['text']).toBe(
      `ollama run hf.co/${QWEN}:Q4_K_M`,
    );
    expect(text(document.querySelector('.g-toast__message'))).toBe('Commande copiée');

    // La fiche s'ouvre sur Q4_K_M, pas sur sa meilleure quantification (Q8_0).
    await click(fixture, byTestId('pick-detail', card('fastest')));
    await fixture.whenStable();
    const detail = TestBed.inject(DetailStore);
    expect(detail.repoId()).toBe(QWEN);
    expect(detail.detail()?.best).toBe(1);
    expect(detail.selectedLabel()).toBe('Q4_K_M');
  });

  it('« Masquer » replie le bandeau sur son titre ; « Afficher » le rouvre', async () => {
    const { fixture, el } = await render();
    const toggle = byTestId('picks-toggle', el);
    expect(text(toggle)).toBe('Masquer');
    expect(toggle.getAttribute('aria-expanded')).toBe('true');

    await click(fixture, toggle);
    expect(allByTestId('pick', el)).toHaveLength(0);
    expect(text(toggle)).toBe('Afficher');
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    expect(TestBed.inject(DisplayStore).picksCollapsed()).toBe(true);
    expect(text(el.querySelector('h2'))).toBe('Trois conseils');

    await click(fixture, toggle);
    expect(allByTestId('pick', el)).toHaveLength(3);
  });

  it('absent sans conseil (catalogue vide, aucun exécutable)', async () => {
    const { el } = await render(makeResponse([]));
    expect(queryTestId('picks', el)).toBeNull();
  });

  it('LM Studio : page du dépôt au lieu d’une commande ; mémoire unifiée : « sur le GPU »', async () => {
    const url = `https://huggingface.co/${QWEN}`;
    const { fixture, el, calls, card } = await render(
      makeResponse([fastest], {
        runtime: 'lmStudio',
        budget: { ...BUDGET, unified: true },
        picks: [
          makePick('best', fastest, {
            launch: { runtime: 'lmStudio', commands: [], openUrl: url, notes: [] },
          }),
        ],
      }),
    );
    expect(text(el.querySelector('h2'))).toBe('Conseil');
    const b = card('best');
    expect(metrics(b)).toContain('30/40 couches sur le GPU');
    expect(queryTestId('pick-command', b)).toBeNull();
    await click(fixture, byTestId('pick-open', b));
    expect(argsOf(calls, 'plugin:opener|open_url')[0]['url']).toBe(url);
  });
});
