import { BUDGET, GIB, makeEstimate } from '../../../testing/fixtures';
import type { MemoryBudget } from '../../core/generated/MemoryBudget';
import { rowFootprint, scaleLabel } from './footprint';

const UNIFIED: MemoryBudget = {
  vram: 11.8 * GIB,
  ram: 14 * GIB,
  gpuBandwidthGbs: 204.8,
  ramBandwidthGbs: 204.8,
  unified: true,
};
const CPU_ONLY: MemoryBudget = { ...BUDGET, vram: 0, gpuBandwidthGbs: null };

describe('empreinte d’une ligne', () => {
  it('échelle de la carte mémoire, en Go ronds', () => {
    expect(scaleLabel(BUDGET)).toBe('0–62 Go');
    expect(scaleLabel(UNIFIED)).toBe('0–14 Go');
  });

  it('GPU : la part en VRAM part de 0, la zone VRAM est teintée', () => {
    const fp = rowFootprint(BUDGET, makeEstimate());
    expect(fp.vramEnd).toBeCloseTo(14.3 / 61.6);
    expect(fp.gpu?.start).toBe(0);
    expect(fp.gpu?.width).toBeCloseTo(6.3 / 61.6);
    expect(fp.ram).toBeNull();
    expect(fp.over).toBeNull();
    expect(fp.label).toBe('6,3 Go en VRAM');
  });

  it('CPU seul : pas de zone VRAM, la part en RAM part de 0', () => {
    const fp = rowFootprint(CPU_ONLY, makeEstimate({ mode: { kind: 'cpu' } }));
    expect(fp.vramEnd).toBe(0);
    expect(fp.gpu).toBeNull();
    expect(fp.ram).toEqual({ start: 0, width: expect.closeTo(6.3 / 47.3) });
    expect(fp.label).toBe('6,3 Go en RAM');
  });

  it('mémoire unifiée : une seule zone, repère du GPU, parts GPU et CPU', () => {
    const e = { ...makeEstimate(), memGpu: 8 * GIB, memCpu: 2 * GIB };
    const fp = rowFootprint(UNIFIED, e);
    expect(fp.unified).toBe(true);
    expect(fp.vramEnd).toBe(1);
    expect(fp.gpuLimit).toBeCloseTo(11.8 / 14);
    expect(fp.ram?.start).toBeCloseTo(8 / 14);
    expect(fp.label).toBe('8,0 Go pour le GPU + 2,0 Go pour le CPU');
  });

  it('trop gros : la mémoire nécessaire déborde jusqu’au bout de l’échelle', () => {
    const e = makeEstimate({ mode: { kind: 'tooBig' }, weights: 70 * GIB, kv: 2 * GIB });
    const fp = rowFootprint(BUDGET, { ...e, memGpu: 0, memCpu: 0 });
    expect(fp.gpu).toBeNull();
    expect(fp.ram).toBeNull();
    expect(fp.over).toEqual({ start: 0, width: 1 });
    expect(fp.label).toBe('Environ 72,5 Go nécessaires, pour 61,6 Go utilisables');
  });
});
