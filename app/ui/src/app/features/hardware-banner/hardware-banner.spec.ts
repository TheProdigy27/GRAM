import { TestBed } from '@angular/core/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import { argsOf, mockBackend } from '../../../testing/backend';
import { byTestId, click, press, queryTestId, text } from '../../../testing/dom';
import { GIB, HARDWARE, MAC_HARDWARE, makeResponse } from '../../../testing/fixtures';
import type { HardwareProfile } from '../../core/generated/HardwareProfile';
import { HardwareStore } from '../../state/hardware.store';
import { FILTER_DEBOUNCE_MS } from '../../state/ranking.store';
import { HardwareBanner, simulationLabel } from './hardware-banner';

describe('libellé de la simulation', () => {
  it('GPU simulé, RAM simulée, ou les deux', () => {
    const sim = (overrides: Partial<HardwareProfile['overrides']>) =>
      simulationLabel({ ...HARDWARE, overrides: { ...HARDWARE.overrides, ...overrides } });
    expect(sim({ simulatedGpu: 'NVIDIA GeForce RTX 3060 12 Go' })).toBe('RTX 3060 12 Go');
    expect(sim({ simulatedRamTotal: 32 * GIB })).toBe('RAM 32 Go');
    expect(
      sim({ simulatedGpu: 'AMD Radeon RX 7900 XTX 24 Go', simulatedRamTotal: 128 * GIB }),
    ).toBe('RX 7900 XTX 24 Go, RAM 128 Go');
  });
});

describe('bande « Votre machine »', () => {
  afterEach(() => clearMocks());

  async function render(handlers: Parameters<typeof mockBackend>[0] = {}) {
    const calls = mockBackend(handlers);
    TestBed.configureTestingModule({ providers: [{ provide: FILTER_DEBOUNCE_MS, useValue: 0 }] });
    void TestBed.inject(HardwareStore).load();
    const fixture = TestBed.createComponent(HardwareBanner);
    await fixture.whenStable();
    return { calls, fixture, el: fixture.nativeElement as HTMLElement };
  }

  /** Titre, étiquette, lignes et barre d'une carte. */
  const card = (id: string, root: Element) => {
    const c = byTestId(id, root);
    return [
      text(c.querySelector('.card__title')),
      text(c.querySelector('.card__tag')),
      ...[...c.querySelectorAll('.card__line')].map(text),
      text(c.querySelector('.usage__label')) || '—',
    ];
  };

  /** Texte de l'infobulle (reliée par aria-describedby). */
  const described = (el: Element) =>
    (el.getAttribute('aria-describedby') ?? '')
      .split(' ')
      .map((id) => document.getElementById(id)?.textContent ?? '')
      .join('\n');

  it('cartes GPU, RAM, CPU et runtimes, sous la mémoire utilisable', async () => {
    const { el } = await render();
    expect(text(byTestId('hw-usable', el))).toBe(
      '61,6 Go utilisables par un modèle, à deux vitesses',
    );
    expect(card('hw-gpu', el)).toEqual([
      'NVIDIA GeForce RTX 5080',
      'GPU',
      '16 Go de VRAM · ≈960 Go/s (table de référence)',
      'CUDA 12.9 · Vulkan',
      '14,3 Go utilisables',
    ]);
    expect(card('hw-ram', el)).toEqual([
      '64 Go DDR5-6000',
      'RAM',
      '2 canaux (estimée) · ≈96 Go/s',
      '47,3 Go utilisables',
    ]);
    expect(card('hw-cpu', el)).toEqual([
      'Ryzen 7 9800X3D',
      'CPU',
      '8 cœurs / 16 threads · AVX-512',
      'Disque C: · 210 Go libres',
      '—',
    ]);
    expect([...byTestId('hw-runtimes', el).querySelectorAll('li')].map(text)).toEqual([
      'Ollama en cours · 0.13.1',
      'LM Studio absent',
      'llama.cpp absent',
    ]);

    // Barre à la proportion de la VRAM utilisable, explication en infobulle au clavier.
    const usage = byTestId('hw-gpu', el).querySelector<HTMLElement>('.usage')!;
    const fill = usage.querySelector<HTMLElement>('.usage__fill')!;
    expect(parseFloat(fill.style.width)).toBeCloseTo((14.3 / 15.9) * 100, 1);
    expect(usage.getAttribute('tabindex')).toBe('0');
    expect(described(usage)).toContain('marge de sécurité');
    // Nom complet du processeur en infobulle.
    expect(described(byTestId('hw-cpu', el).querySelector('.card__title')!)).toBe(
      'AMD Ryzen 7 9800X3D 8-Core Processor',
    );
    expect(text(byTestId('open-corrections', el))).toBe('Corriger ou simuler…');
    expect(queryTestId('hw-warnings', el)).toBeNull();
  });

  it('classement en cours : squelette à la place des barres de la VRAM et de la RAM', async () => {
    // Classement en attente : l'application n'est jamais stable, on rafraîchit à la main.
    let answer!: (r: ReturnType<typeof makeResponse>) => void;
    mockBackend({ rank: () => new Promise((resolve) => (answer = resolve)) });
    TestBed.configureTestingModule({ providers: [{ provide: FILTER_DEBOUNCE_MS, useValue: 0 }] });
    await TestBed.inject(HardwareStore).load();
    const fixture = TestBed.createComponent(HardwareBanner);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    expect(queryTestId('hw-usable', el)).toBeNull();
    expect(byTestId('hw-gpu', el).querySelector('g-skeleton')).not.toBeNull();
    expect(byTestId('hw-ram', el).querySelector('g-skeleton')).not.toBeNull();
    expect(byTestId('hw-cpu', el).querySelector('g-skeleton')).toBeNull();

    answer(makeResponse([]));
    await fixture.whenStable();
    expect(text(byTestId('hw-gpu', el).querySelector('.usage__label'))).toBe('14,3 Go utilisables');
  });

  it('classement en échec : mémoire utilisable inconnue, cartes sans barre', async () => {
    const { el } = await render({
      rank: () => {
        throw 'catalogue illisible';
      },
    });
    expect(text(byTestId('hw-usable', el))).toBe(
      'Mémoire utilisable inconnue : elle est calculée avec le classement.',
    );
    expect(byTestId('hw-gpu', el).querySelector('.usage, g-skeleton')).toBeNull();
    expect(card('hw-gpu', el)[0]).toBe('NVIDIA GeForce RTX 5080');
  });

  it('fiche ouverte : une ligne de pastilles, « Détails de la machine » rouvre la bande', async () => {
    const { fixture, el } = await render();
    fixture.componentRef.setInput('compact', true);
    await fixture.whenStable();
    expect(queryTestId('hw-gpu', el)).toBeNull();
    expect([...byTestId('machine-pills', el).querySelectorAll('li')].map(text)).toEqual([
      'RTX 5080 · 14,3 Go de VRAM utilisables',
      '64 Go DDR5-6000 · 47,3 Go de RAM utilisables',
      'Ollama en cours',
    ]);
    expect(el.querySelector('#machine-title')).not.toBeNull();

    await click(fixture, 'machine-expand');
    expect(queryTestId('hw-gpu', el)).not.toBeNull();
    expect(byTestId('machine-collapse', el).getAttribute('aria-expanded')).toBe('true');
    await click(fixture, 'machine-collapse');
    expect(queryTestId('hw-gpu', el)).toBeNull();

    // Rouverte puis fiche refermée et rouverte : de nouveau resserrée.
    await click(fixture, 'machine-expand');
    fixture.componentRef.setInput('compact', false);
    await fixture.whenStable();
    expect(queryTestId('machine-collapse', el)).toBeNull();
    fixture.componentRef.setInput('compact', true);
    await fixture.whenStable();
    expect(queryTestId('machine-pills', el)).not.toBeNull();
  });

  it('Mac à mémoire unifiée : part du GPU prise sur la RAM', async () => {
    const { el } = await render({
      get_hardware: () => MAC_HARDWARE,
      rank: () =>
        makeResponse([], {
          budget: {
            vram: 8.9 * GIB,
            ram: 12 * GIB,
            gpuBandwidthGbs: 204.8,
            ramBandwidthGbs: 204.8,
            unified: true,
          },
        }),
    });
    expect(text(byTestId('hw-usable', el))).toBe(
      '12,0 Go utilisables par un modèle, dont 8,9 Go pour le GPU',
    );
    expect(card('hw-gpu', el)).toEqual([
      'Apple M1 Pro',
      'GPU',
      'Mémoire unifiée : jusqu’à 12 Go pour le GPU · ≈205 Go/s (table de référence)',
      'Metal',
      '8,9 Go utilisables',
    ]);
    expect(card('hw-ram', el)).toEqual([
      '16 Go LPDDR5-6400',
      'RAM',
      'Mémoire unifiée, partagée avec le GPU',
      '4 canaux (table de référence) · ≈205 Go/s',
      '12,0 Go utilisables',
    ]);
    expect(card('hw-cpu', el)[2]).toBe('10 cœurs / 10 threads · ARM64');
  });

  it('simulation : étiquette avec le GPU simulé, « Arrêter » garde les corrections', async () => {
    const simulated: HardwareProfile = {
      ...HARDWARE,
      overrides: { ...HARDWARE.overrides, simulatedGpu: 'NVIDIA GeForce RTX 4090 24 Go' },
    };
    const { calls, fixture, el } = await render({ get_hardware: () => simulated });
    expect(text(byTestId('simulation', el))).toBe('Simulation · RTX 4090 24 Go Arrêter');
    await click(fixture, 'stop-simulation');
    expect(argsOf(calls, 'set_hardware_overrides')).toEqual([
      { overrides: { ...simulated.overrides, simulatedGpu: null } },
    ]);
    expect(queryTestId('simulation', el)).toBeNull();
  });

  it('avertissements de détection : icône nommée, texte en infobulle', async () => {
    const { el } = await render({
      get_hardware: () => ({ ...HARDWARE, warnings: ['Canaux estimés.', 'VRAM occupée.'] }),
    });
    const icon = byTestId('hw-warnings', el);
    expect(icon.getAttribute('aria-label')).toBe('Avertissements de détection (2)');
    expect(icon.getAttribute('tabindex')).toBe('0');
    expect(described(icon)).toBe('Canaux estimés.\nVRAM occupée.');
  });

  it('erreur de détection : encadré, puis « Réessayer » relance l’analyse', async () => {
    let fail = true;
    const { calls, fixture, el } = await render({
      get_hardware: () => {
        throw 'WMI indisponible';
      },
      rescan_hardware: () => {
        if (fail) throw 'WMI indisponible';
        return HARDWARE;
      },
    });
    const callout = el.querySelector('g-callout')!;
    expect(callout.getAttribute('role')).toBe('alert');
    expect(text(callout)).toContain('Détection du matériel impossible');
    expect(text(callout)).toContain('WMI indisponible');

    fail = false;
    await click(fixture, 'retry-hardware');
    expect(argsOf(calls, 'rescan_hardware')).toHaveLength(1);
    expect(el.querySelector('g-callout')).toBeNull();
    expect(queryTestId('hw-gpu', el)).not.toBeNull();
  });

  it('pendant l’analyse : squelette et aria-busy', async () => {
    const { el } = await render({ get_hardware: () => new Promise(() => undefined) });
    expect(el.querySelector('.machine')?.getAttribute('aria-busy')).toBe('true');
    expect(el.querySelectorAll('g-skeleton').length).toBeGreaterThan(0);
    expect(text(el.querySelector('.g-sr-only'))).toBe('Analyse du matériel…');
  });

  it('la touche R ré-analyse, mais pas dans un champ', async () => {
    const { calls, fixture } = await render();
    await press(fixture, document.body, 'r');
    expect(argsOf(calls, 'rescan_hardware')).toHaveLength(1);

    const input = document.createElement('input');
    document.body.appendChild(input);
    await press(fixture, input, 'r');
    input.remove();
    expect(argsOf(calls, 'rescan_hardware')).toHaveLength(1);
  });
});
