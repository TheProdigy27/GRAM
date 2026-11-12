import { BUDGET, GIB, HARDWARE, MAC_HARDWARE } from '../../../testing/fixtures';
import type { MemoryBudget } from '../../core/generated/MemoryBudget';
import {
  type MemorySelection,
  machinePills,
  memoryCard,
  shortCpuName,
  shortGpuName,
} from './memory-card';

const QWEN: MemorySelection = {
  name: 'Qwen3-8B',
  quant: 'Q4_K_M',
  memGpu: 6.3 * GIB,
  memCpu: 0,
  opened: false,
};

const MAC_BUDGET: MemoryBudget = {
  vram: 8.9 * GIB,
  ram: 12 * GIB,
  gpuBandwidthGbs: 204.8,
  ramBandwidthGbs: 204.8,
  unified: true,
};

describe('memoryCard', () => {
  it('GPU dédié : zone VRAM puis zone RAM, rapport des vitesses, graduations', () => {
    const c = memoryCard(BUDGET, HARDWARE, QWEN);
    expect(c.summary).toBe('61,6 Go utilisables par un modèle, à deux vitesses');
    expect(c.note).toBe('');
    expect(c.zones.map(({ kind, title, detail }) => [kind, title, detail])).toEqual([
      ['vram', 'VRAM · RTX 5080', '14,3 Go · 960 Go/s'],
      ['ram', 'RAM · 64 Go DDR5-6000, 2 canaux', '47,3 Go · 96 Go/s · 10× plus lente que la VRAM'],
    ]);
    expect(c.zones[0].width).toBeCloseTo(14.3 / 61.6);
    expect(c.ticks.map((t) => t.label)).toEqual(['0', '20', '40', '60 Go']);
    expect(c.ticks.map((t) => t.align)).toEqual(['start', 'center', 'center', 'end']);
    expect(c.gpuLimit).toBeNull();
    expect(c.selection).toEqual({
      label: 'En tête du classement',
      model: 'Qwen3-8B · Q4_K_M',
      where: '6,3 Go en VRAM',
    });
    expect(c.ariaLabel).toBe(
      'Échelle de 0 à 61,6 Go : 14,3 Go de VRAM à 960 Go/s, puis 47,3 Go de RAM à 96 Go/s, ' +
        '10 fois plus lente. En tête du classement : Qwen3-8B en Q4_K_M, 6,3 Go en VRAM.',
    );
  });

  it('modèle ouvert en hybride : part en VRAM et part en RAM', () => {
    const c = memoryCard(BUDGET, HARDWARE, {
      ...QWEN,
      name: 'Mistral-Small-24B',
      memGpu: 13.9 * GIB,
      memCpu: 6 * GIB,
      opened: true,
    });
    expect(c.selection?.label).toBe('Sélection');
    expect(c.selection?.where).toBe('13,9 Go en VRAM, 6,0 Go en RAM');
    expect(c.footprint?.ram?.start).toBeCloseTo(c.zones[0].width);
  });

  it('trop gros : signalé dans le texte', () => {
    const c = memoryCard(BUDGET, HARDWARE, { ...QWEN, memGpu: 14.3 * GIB, memCpu: 70 * GIB });
    expect(c.footprint?.overflow).toBe(true);
    expect(c.selection?.where).toBe(
      '14,3 Go en VRAM, 70,0 Go en RAM : plus que la mémoire utilisable',
    );
  });

  it('sans sélection : ni empreinte ni texte', () => {
    const c = memoryCard(BUDGET, HARDWARE, null);
    expect(c.footprint).toBeNull();
    expect(c.selection).toBeNull();
    expect(c.ariaLabel).not.toContain('classement');
  });

  it('mémoire unifiée : une seule zone, repère du GPU, rien ne s’additionne', () => {
    const c = memoryCard(MAC_BUDGET, MAC_HARDWARE, { ...QWEN, memGpu: 6 * GIB, opened: true });
    expect(c.summary).toBe('12,0 Go utilisables par un modèle, dont 8,9 Go pour le GPU');
    expect(c.zones.map(({ kind, title, detail }) => [kind, title, detail])).toEqual([
      ['unified', 'Mémoire unifiée · 16 Go LPDDR5-6400, 4 canaux', '12,0 Go · 205 Go/s'],
    ]);
    expect(c.gpuLimit?.label).toBe('GPU jusqu’à 8,9 Go');
    expect(c.gpuLimit?.pos).toBeCloseTo(8.9 / 12);
    expect(c.ticks.map((t) => t.label)).toEqual(['0', '5', '10 Go']);
    expect(c.selection?.where).toBe('6,0 Go pour le GPU');
    expect(c.footprint?.gpu).toEqual({ start: 0, width: 0.5 });
    expect(c.ariaLabel).toContain('Échelle de 0 à 12,0 Go de mémoire unifiée à 205 Go/s');
  });

  it('sans GPU, ou GPU intégré compté comme du CPU : une seule zone RAM, avec une note', () => {
    const cpuBudget = { ...BUDGET, vram: 0, gpuBandwidthGbs: null };
    const noGpu = memoryCard(
      cpuBudget,
      { ...HARDWARE, gpu: null },
      { ...QWEN, memGpu: 0, memCpu: 6 * GIB },
    );
    expect(noGpu.summary).toBe('47,3 Go utilisables par un modèle');
    expect(noGpu.note).toBe('aucun GPU : calcul sur le CPU');
    expect(noGpu.zones.map((z) => [z.kind, z.width])).toEqual([['ram', 1]]);
    expect(noGpu.selection?.where).toBe('6,0 Go en RAM');
    expect(noGpu.footprint?.ram?.start).toBe(0);

    const igpu = { ...HARDWARE, gpu: { ...HARDWARE.gpu!, integrated: true } };
    expect(memoryCard(cpuBudget, igpu, null).note).toBe('GPU intégré compté comme du CPU');
  });

  it('bande passante du GPU inconnue : pas de rapport des vitesses', () => {
    const c = memoryCard({ ...BUDGET, gpuBandwidthGbs: null }, HARDWARE, null);
    expect(c.zones.map((z) => z.detail)).toEqual(['14,3 Go', '47,3 Go · 96 Go/s']);
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
