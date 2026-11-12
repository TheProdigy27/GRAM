import { BUDGET, GIB, HARDWARE, MAC_HARDWARE } from '../../../testing/fixtures';
import type { HardwareProfile } from '../../core/generated/HardwareProfile';
import type { MemoryBudget } from '../../core/generated/MemoryBudget';
import {
  type MachineCard,
  machineCards,
  machinePills,
  shortCpuName,
  shortGpuName,
  usableSummary,
} from './machine-cards';

const MAC_BUDGET: MemoryBudget = {
  vram: 8.9 * GIB,
  ram: 12 * GIB,
  gpuBandwidthGbs: 204.8,
  ramBandwidthGbs: 204.8,
  unified: true,
};

/** Titre, étiquette, lignes et barre d'une carte, en texte. */
const summary = (c: MachineCard) => [c.title, c.tag, ...c.lines, c.usage?.label ?? '—'];

describe('machineCards', () => {
  it('GPU dédié : VRAM et RAM utilisables, origine des valeurs, CPU et disque', () => {
    const [gpu, ram, cpu] = machineCards(HARDWARE, BUDGET);
    expect(summary(gpu)).toEqual([
      'NVIDIA GeForce RTX 5080',
      'GPU',
      '16 Go de VRAM · ≈960 Go/s (table de référence)',
      'CUDA 12.9 · Vulkan',
      '14,3 Go utilisables',
    ]);
    expect(gpu.usage?.fraction).toBeCloseTo(14.3 / 15.9);
    expect(gpu.usage?.tooltip).toContain('marge de sécurité');

    expect(summary(ram)).toEqual([
      '64 Go DDR5-6000',
      'RAM',
      '2 canaux (estimée) · ≈96 Go/s',
      '47,3 Go utilisables',
    ]);
    expect(ram.usage?.fraction).toBeCloseTo(47.3 / 63.1);

    expect(summary(cpu)).toEqual([
      'Ryzen 7 9800X3D',
      'CPU',
      '8 cœurs / 16 threads · AVX-512',
      'Disque C: · 210 Go libres',
      '—',
    ]);
    expect(cpu.fullName).toBe('AMD Ryzen 7 9800X3D 8-Core Processor');
    expect(cpu.metered).toBe(false);
  });

  it('sans budget (classement en cours) : pas de barre, mais une est attendue', () => {
    const [gpu, ram, cpu] = machineCards(HARDWARE, null);
    expect([gpu.usage, ram.usage, cpu.usage]).toEqual([null, null, null]);
    expect([gpu.metered, ram.metered, cpu.metered]).toEqual([true, true, false]);
  });

  it('portable : GPU intégré compté comme du CPU, RAM de type inconnu, deux disques', () => {
    const p: HardwareProfile = {
      ...HARDWARE,
      cpu: { ...HARDWARE.cpu, name: '12th Gen Intel(R) Core(TM) i7-12700H', avx512: false },
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
      disks: [
        { mount: 'C:\\', total: 512 * GIB, available: 120 * GIB },
        { mount: 'D:\\', total: 2048 * GIB, available: 1229 * GIB },
      ],
    };
    const budget = { ...BUDGET, vram: 0, gpuBandwidthGbs: null, ram: 11 * GIB };
    const [gpu, ram, cpu] = machineCards(p, budget);
    expect(summary(gpu)).toEqual([
      'Intel(R) Iris(R) Xe Graphics',
      'GPU',
      'Intégré : compté comme du CPU (mémoire partagée)',
      '—',
    ]);
    expect(gpu.metered).toBe(false);
    expect(summary(ram)).toEqual(['16 Go 3200 MT/s', 'RAM', '1 canal', '11,0 Go utilisables']);
    expect(summary(cpu)).toEqual([
      'Core i7-12700H',
      'CPU',
      '8 cœurs / 16 threads · AVX2',
      'Disques : C: 120 Go libres · D: 1,2 To libres',
      '—',
    ]);

    const [none] = machineCards({ ...p, gpu: null }, budget);
    expect(summary(none)).toEqual(['Aucun GPU', 'GPU', 'Calcul sur le CPU', '—']);
  });

  it('Mac Apple Silicon : mémoire unifiée, part du GPU prise sur la RAM', () => {
    const [gpu, ram, cpu] = machineCards(MAC_HARDWARE, MAC_BUDGET);
    expect(summary(gpu)).toEqual([
      'Apple M1 Pro',
      'GPU',
      'Mémoire unifiée : jusqu’à 12 Go pour le GPU · ≈205 Go/s (table de référence)',
      'Metal',
      '8,9 Go utilisables',
    ]);
    expect(gpu.usage?.tooltip).toContain('limite de Metal');
    expect(summary(ram)).toEqual([
      '16 Go LPDDR5-6400',
      'RAM',
      'Mémoire unifiée, partagée avec le GPU',
      '4 canaux (table de référence) · ≈205 Go/s',
      '12,0 Go utilisables',
    ]);
    expect(ram.usage?.tooltip).toContain('part du GPU comprise');
    expect(summary(cpu)).toEqual([
      'Apple M1 Pro',
      'CPU',
      '10 cœurs / 10 threads · ARM64',
      'Disque / · 259 Go libres',
      '—',
    ]);
    expect(cpu.fullName).toBe('');
  });

  it('bande passante corrigée à la main, GPU non retenus', () => {
    const p: HardwareProfile = {
      ...HARDWARE,
      gpu: { ...HARDWARE.gpu!, bandwidthGbs: 900, bandwidthSource: 'manual' },
      otherGpus: [{ ...HARDWARE.gpu!, index: 1, name: 'AMD Radeon(TM) Graphics' }],
    };
    const [gpu] = machineCards(p, BUDGET);
    expect(gpu.lines).toEqual([
      '16 Go de VRAM · ≈900 Go/s (corrigée)',
      'CUDA 12.9 · Vulkan',
      'Non utilisé : AMD Radeon(TM) Graphics',
    ]);
  });
});

describe('usableSummary', () => {
  it('VRAM puis RAM, mémoire unifiée, CPU seul', () => {
    expect(usableSummary(BUDGET)).toBe('61,6 Go utilisables par un modèle, à deux vitesses');
    expect(usableSummary(MAC_BUDGET)).toBe(
      '12,0 Go utilisables par un modèle, dont 8,9 Go pour le GPU',
    );
    expect(usableSummary({ ...BUDGET, vram: 0, gpuBandwidthGbs: null })).toBe(
      '47,3 Go utilisables par un modèle',
    );
  });
});

describe('machinePills', () => {
  const line = (p: ReturnType<typeof machinePills>[number]) => p.text + p.value + p.after;

  it('GPU dédié : VRAM puis RAM utilisables', () => {
    const pills = machinePills(BUDGET, HARDWARE);
    expect(pills.map((p) => p.kind)).toEqual(['vram', 'ram']);
    expect(pills.map(line)).toEqual([
      'RTX 5080 · 14,3 Go de VRAM utilisables',
      '64 Go DDR5-6000 · 47,3 Go de RAM utilisables',
    ]);
    expect(pills[0].tooltip).toBe('VRAM à 960 Go/s, la mémoire la plus rapide');
  });

  it('mémoire unifiée : une seule pastille, part du GPU comprise', () => {
    const pills = machinePills(MAC_BUDGET, MAC_HARDWARE);
    expect(pills.map((p) => p.kind)).toEqual(['unified']);
    expect(line(pills[0])).toContain('12,0 Go utilisables, dont 8,9 Go pour le GPU');
  });

  it('sans VRAM utilisable : la RAM seule, avec la raison', () => {
    const pills = machinePills(
      { ...BUDGET, vram: 0, gpuBandwidthGbs: null },
      { ...HARDWARE, gpu: null },
    );
    expect(pills.map((p) => p.kind)).toEqual(['ram']);
    expect(pills[0].tooltip).toContain('aucun GPU : calcul sur le CPU');
  });
});

describe('noms courts', () => {
  it('GPU', () => {
    expect(shortGpuName('NVIDIA GeForce RTX 5080')).toBe('RTX 5080');
    expect(shortGpuName('NVIDIA GeForce RTX 4090 24 Go')).toBe('RTX 4090 24 Go');
    expect(shortGpuName('AMD Radeon RX 7900 XTX')).toBe('RX 7900 XTX');
    expect(shortGpuName('AMD Radeon(TM) Graphics')).toBe('Radeon Graphics');
    expect(shortGpuName('Intel(R) Iris(R) Xe Graphics')).toBe('Iris Xe Graphics');
    expect(shortGpuName('Apple M1 Pro')).toBe('Apple M1 Pro');
  });

  it('CPU', () => {
    expect(shortCpuName('AMD Ryzen 7 9800X3D 8-Core Processor')).toBe('Ryzen 7 9800X3D');
    expect(shortCpuName('12th Gen Intel(R) Core(TM) i7-12700H')).toBe('Core i7-12700H');
    expect(shortCpuName('Intel(R) Core(TM) i7-8700K CPU @ 3.70GHz')).toBe('Core i7-8700K');
    expect(shortCpuName('Apple M1 Pro')).toBe('Apple M1 Pro');
  });
});
