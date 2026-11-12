import { TestBed } from '@angular/core/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import { argsOf, mockBackend } from '../../../testing/backend';
import { byTestId, click, press, queryTestId, text } from '../../../testing/dom';
import { GIB, HARDWARE, MAC_HARDWARE, makeResponse } from '../../../testing/fixtures';
import type { HardwareProfile } from '../../core/generated/HardwareProfile';
import { DetailStore } from '../../state/detail.store';
import { HardwareStore } from '../../state/hardware.store';
import { FILTER_DEBOUNCE_MS } from '../../state/ranking.store';
import {
  type BannerPart,
  HardwareBanner,
  cpuParts,
  diskParts,
  gpuParts,
  ramParts,
  simulationLabel,
} from './hardware-banner';

const join = (parts: BannerPart[]) =>
  parts.map((p) => (p.source ? `${p.text} (${p.source})` : p.text)).join(' · ');

describe('valeurs du matériel', () => {
  it('machine de développement', () => {
    expect(join(cpuParts(HARDWARE))).toBe(
      'AMD Ryzen 7 9800X3D 8-Core Processor · 8c/16t · AVX-512',
    );
    expect(join(ramParts(HARDWARE))).toBe('64 Go DDR5-6000 · 2 canaux (estimée) · ≈96 Go/s');
    expect(join(gpuParts(HARDWARE))).toBe(
      'NVIDIA GeForce RTX 5080 · 16 Go · ≈960 Go/s (table de référence) · CUDA 12.9 · Vulkan',
    );
    expect(join(diskParts(HARDWARE))).toBe('C: 210 Go libres');
  });

  it('portable sans GPU dédié, RAM de type inconnu', () => {
    const p: HardwareProfile = {
      ...HARDWARE,
      cpu: { ...HARDWARE.cpu, avx512: false },
      ram: {
        ...HARDWARE.ram,
        installed: null,
        total: 15.7 * GIB,
        kind: 'unknown',
        speedMts: 3200,
        channels: 1,
        channelsSource: 'detected',
        bandwidthGbs: null,
      },
      gpu: { ...HARDWARE.gpu!, name: 'Intel(R) Iris(R) Xe Graphics', integrated: true },
      backends: { cuda: false, cudaDriverVersion: null, vulkan: true, metal: false },
    };
    expect(join(cpuParts(p))).toContain('AVX2');
    expect(join(ramParts(p))).toBe('16 Go 3200 MT/s · 1 canal');
    expect(join(gpuParts(p))).toBe(
      'Intel(R) Iris(R) Xe Graphics · intégré : compté comme du CPU (mémoire partagée)',
    );
    expect(join(gpuParts({ ...p, gpu: null }))).toBe('Aucun GPU : calcul sur le CPU');
  });

  it('Mac Apple Silicon : mémoire unifiée, GPU Metal', () => {
    expect(join(cpuParts(MAC_HARDWARE))).toBe('Apple M1 Pro · 10c/10t · ARM64');
    expect(join(ramParts(MAC_HARDWARE))).toBe(
      '16 Go LPDDR5-6400 · 4 canaux (table de référence) · ≈205 Go/s',
    );
    expect(join(gpuParts(MAC_HARDWARE))).toBe(
      'Apple M1 Pro · mémoire unifiée : 12 Go pour le GPU · ≈205 Go/s (table de référence) · Metal',
    );
    expect(join(diskParts(MAC_HARDWARE))).toBe('/ 259 Go libres');
  });

  it('bande passante corrigée à la main', () => {
    const p = {
      ...HARDWARE,
      gpu: { ...HARDWARE.gpu!, bandwidthGbs: 900, bandwidthSource: 'manual' as const },
    };
    expect(join(gpuParts(p))).toContain('≈900 Go/s (corrigée)');
  });

  it('libellé de la simulation', () => {
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

  /** Titre et détail de chaque zone de la carte mémoire. */
  const zones = (root: Element) =>
    [...root.querySelectorAll('.zone')].map((z) => [
      text(z.querySelector('.zone__title')),
      text(z.querySelector('.zone__detail')),
    ]);

  /** Texte de l'infobulle (reliée par aria-describedby). */
  const described = (el: Element) =>
    (el.getAttribute('aria-describedby') ?? '')
      .split(' ')
      .map((id) => document.getElementById(id)?.textContent ?? '')
      .join('\n');

  it('carte mémoire, processeur, runtimes et détails de la machine', async () => {
    const { el } = await render();
    expect(text(byTestId('hw-usable', el))).toBe(
      '61,6 Go utilisables par un modèle, à deux vitesses',
    );
    const card = byTestId('memory-card', el);
    expect(card.getAttribute('role')).toBe('img');
    expect(card.getAttribute('aria-label')).toContain('14,3 Go de VRAM à 960 Go/s');
    expect(zones(card)).toEqual([
      ['VRAM · RTX 5080', '14,3 Go · 960 Go/s'],
      ['RAM · 64 Go DDR5-6000, 2 canaux', '47,3 Go · 96 Go/s · 10× plus lente que la VRAM'],
    ]);
    // Sans fiche ouverte : le premier du classement.
    expect(text(byTestId('memory-selection', el))).toBe(
      'En tête du classement : Qwen3-8B · Q8_0 — 9,7 Go en VRAM',
    );
    expect(byTestId('memory-footprint', el).querySelectorAll('.fp--gpu')).toHaveLength(1);

    expect(text(byTestId('hw-cpu', el))).toBe('Ryzen 7 9800X3D · 8c/16t · AVX-512');
    expect([...byTestId('hw-runtimes', el).querySelectorAll('li')].map(text)).toEqual([
      'Ollama en cours',
      'LM Studio absent',
      'llama.cpp absent',
    ]);
    const details = described(byTestId('machine-details', el));
    expect(details).toContain('≈960 Go/s (table de référence)');
    expect(details).toContain('2 canaux (estimée)');
    expect(details).toContain('Disques : C: 210 Go libres');
    expect(queryTestId('hw-warnings', el)).toBeNull();
  });

  it('l’empreinte suit le modèle ouvert et sa quantification', async () => {
    const { fixture, el } = await render();
    const detail = TestBed.inject(DetailStore);
    detail.open('Qwen/Qwen3-8B-GGUF');
    await fixture.whenStable();
    expect(text(byTestId('memory-selection', el))).toBe(
      'Sélection : Qwen3-8B · Q8_0 — 9,7 Go en VRAM',
    );
    detail.select('Q4_K_M');
    await fixture.whenStable();
    expect(text(byTestId('memory-selection', el))).toBe(
      'Sélection : Qwen3-8B · Q4_K_M — 6,3 Go en VRAM',
    );
  });

  it('fiche ouverte : une ligne de pastilles, « Détails de la machine » rouvre la bande', async () => {
    const { fixture, el } = await render();
    fixture.componentRef.setInput('compact', true);
    await fixture.whenStable();
    expect(queryTestId('memory-card', el)).toBeNull();
    expect([...byTestId('machine-pills', el).querySelectorAll('li')].map(text)).toEqual([
      'RTX 5080 · 14,3 Go de VRAM utilisables',
      '64 Go DDR5-6000 · 47,3 Go de RAM utilisables',
      'Ollama en cours',
    ]);
    expect(el.querySelector('#machine-title')).not.toBeNull();

    await click(fixture, 'machine-expand');
    expect(queryTestId('memory-card', el)).not.toBeNull();
    expect(byTestId('machine-collapse', el).getAttribute('aria-expanded')).toBe('true');
    await click(fixture, 'machine-collapse');
    expect(queryTestId('memory-card', el)).toBeNull();

    // Rouverte puis fiche refermée et rouverte : de nouveau resserrée.
    await click(fixture, 'machine-expand');
    fixture.componentRef.setInput('compact', false);
    await fixture.whenStable();
    expect(queryTestId('machine-collapse', el)).toBeNull();
    fixture.componentRef.setInput('compact', true);
    await fixture.whenStable();
    expect(queryTestId('machine-pills', el)).not.toBeNull();
  });

  it('Mac à mémoire unifiée : une seule zone et le repère du GPU', async () => {
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
    expect(zones(el)).toEqual([
      ['Mémoire unifiée · 16 Go LPDDR5-6400, 4 canaux', '12,0 Go · 205 Go/s'],
    ]);
    expect(text(el.querySelector('.gpu-limit'))).toBe('GPU jusqu’à 8,9 Go');
    expect(queryTestId('memory-selection', el)).toBeNull();
    expect(text(byTestId('hw-cpu', el))).toBe('Apple M1 Pro · 10c/10t · ARM64');
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
    expect(queryTestId('memory-card', el)).not.toBeNull();
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
