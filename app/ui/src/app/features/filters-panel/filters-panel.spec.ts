import { TestBed } from '@angular/core/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import { argsOf, mockBackend } from '../../../testing/backend';
import {
  byTestId,
  chooseOption,
  clearField,
  click,
  press,
  queryTestId,
  text,
  typeInto,
} from '../../../testing/dom';
import { RUNTIMES } from '../../../testing/fixtures';
import type { RankRequest } from '../../core/generated/RankRequest';
import { DISPLAY_PREFS_KEY, DisplayStore } from '../../state/display.store';
import { HardwareStore } from '../../state/hardware.store';
import { FILTER_DEBOUNCE_MS } from '../../state/ranking.store';
import { SettingsStore } from '../../state/settings.store';
import { FiltersPanel } from './filters-panel';

describe('FiltersPanel', () => {
  afterEach(() => {
    clearMocks();
    localStorage.clear();
  });

  async function render(handlers: Parameters<typeof mockBackend>[0] = {}) {
    const calls = mockBackend(handlers);
    TestBed.configureTestingModule({ providers: [{ provide: FILTER_DEBOUNCE_MS, useValue: 0 }] });
    const fixture = TestBed.createComponent(FiltersPanel);
    await Promise.all([TestBed.inject(HardwareStore).load(), TestBed.inject(SettingsStore).load()]);
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    const last = () => argsOf(calls, 'rank').at(-1)!['request'] as RankRequest;
    return { fixture, el, calls, last };
  }

  it('usage : cinq boutons à bascule avec icône et touche', async () => {
    const { fixture, el, last } = await render();
    const group = byTestId('filter-use-case', el);
    expect(group.getAttribute('role')).toBe('group');
    const options = [...group.querySelectorAll('button')];
    expect(options.map((o) => text(o.querySelector('.opt__label')))).toEqual([
      'Polyvalent',
      'Code',
      'Raisonnement',
      'Chat rapide',
      'Vision',
    ]);
    expect(options.map((o) => text(o.querySelector('kbd')))).toEqual(['1', '2', '3', '4', '5']);
    expect(options.map((o) => o.getAttribute('aria-keyshortcuts'))).toEqual([
      '1',
      '2',
      '3',
      '4',
      '5',
    ]);
    expect(options[0].getAttribute('aria-pressed')).toBe('true');
    await click(fixture, options[4]);
    expect(last().useCase).toBe('vision');
    expect(options[4].getAttribute('aria-pressed')).toBe('true');
    expect(options[0].getAttribute('aria-pressed')).toBe('false');
  });

  it('contexte : pastilles en grille de 4, « défaut » puis « Contexte par défaut »', async () => {
    const { fixture, el, last } = await render();
    const chips = byTestId('filter-ctx', el);
    expect(chips.style.gridTemplateColumns).toBe('repeat(4, minmax(0, 1fr))');
    expect([...chips.querySelectorAll('button')].map(text)).toEqual([
      '2k',
      '4k',
      '8k',
      '16k',
      '32k',
      '64k',
      '128k',
    ]);
    expect(text(byTestId('ctx-value', el))).toBe('8k · défaut');
    expect(chips.querySelector('[aria-pressed="true"]')?.textContent?.trim()).toBe('8k');
    expect(queryTestId('ctx-reset', el)).toBeNull();

    await chooseOption(fixture, chips, '32k');
    expect(last().ctx).toBe(32768);
    expect(text(byTestId('ctx-value', el))).toBe('32k');

    await click(fixture, byTestId('ctx-reset', el));
    expect(last()).not.toHaveProperty('ctx');
    expect(text(byTestId('ctx-value', el))).toBe('8k · défaut');
  });

  it('runtime : état de chacun, aide si le runtime choisi est absent', async () => {
    const { fixture, el, last } = await render({
      detect_runtimes: () => ({
        ...RUNTIMES,
        llamaCpp: {
          installed: true,
          running: false,
          path: 'C:\\llama\\llama-server.exe',
          version: null,
        },
      }),
    });
    const options = [...byTestId('filter-runtime', el).querySelectorAll('button')];
    expect(options.map(text)).toEqual([
      'Ollama en cours',
      'LM Studio absent',
      'llama.cpp installé',
    ]);
    expect(options.map((o) => o.dataset['state'])).toEqual(['running', 'absent', 'installed']);
    expect(options[0].getAttribute('aria-pressed')).toBe('true');
    expect(queryTestId('runtime-hint', el)).toBeNull();

    await click(fixture, options[1]);
    expect(last().runtime).toBe('lmStudio');
    expect(text(byTestId('runtime-hint', el))).toBe(
      'LM Studio n’est pas détecté : le classement et les commandes restent valables.',
    );

    // ↓ ↑ passent d'un runtime à l'autre.
    options[1].focus();
    await press(fixture, options[1], 'ArrowDown');
    expect(document.activeElement).toBe(options[2]);
    await press(fixture, options[2], 'Home');
    expect(document.activeElement).toBe(options[0]);
  });

  it('taille max : saisie avec virgule, bouton effacer, « Tient en VRAM »', async () => {
    const { fixture, el, last } = await render();
    const input = byTestId<HTMLInputElement>('filter-max-size', el);
    expect(el.querySelector('label[for="filter-max-size"]')?.textContent?.trim()).toBe(
      'Taille max',
    );

    await typeInto(fixture, input, '12,5');
    expect(last().maxSize).toBe(Math.round(12.5 * 2 ** 30));
    await clearField(fixture, input.closest('g-field')!);
    expect(input.value).toBe('');
    expect(last()).not.toHaveProperty('maxSize');

    // VRAM utilisable : 14,3 Go.
    const fit = byTestId('fit-vram', el);
    expect(fit.getAttribute('aria-pressed')).toBe('false');
    await click(fixture, fit);
    expect(fit.getAttribute('aria-pressed')).toBe('true');
    expect(input.value).toBe('14,3');
    expect(last().maxSize).toBe(Math.round(14.3 * 2 ** 30));
    await click(fixture, fit);
    expect(input.value).toBe('');
    expect(last()).not.toHaveProperty('maxSize');
  });

  it('interrupteurs : regroupement des variantes (préférence gardée), trop gros', async () => {
    const { fixture, el, last, calls } = await render();
    const group = byTestId('filter-group', el).querySelector('[role="switch"]')!;
    expect(group.getAttribute('aria-checked')).toBe('true');
    const requests = argsOf(calls, 'rank').length;
    await click(fixture, group);
    expect(group.getAttribute('aria-checked')).toBe('false');
    expect(TestBed.inject(DisplayStore).groupVariants()).toBe(false);
    expect(JSON.parse(localStorage.getItem(DISPLAY_PREFS_KEY)!)).toMatchObject({
      groupVariants: false,
    });
    // Le regroupement se fait à l'affichage : pas de nouveau calcul.
    expect(argsOf(calls, 'rank')).toHaveLength(requests);

    const tooBig = byTestId('filter-too-big', el).querySelector('[role="switch"]')!;
    expect(text(tooBig)).toBe('Afficher les trop gros');
    await click(fixture, tooBig);
    expect(tooBig.getAttribute('aria-checked')).toBe('true');
    expect(last().showTooBig).toBe(true);
  });
});
