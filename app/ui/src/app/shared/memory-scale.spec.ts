import { BUDGET, GIB } from '../../testing/fixtures';
import { footprint, memoryScale } from './memory-scale';

describe('memoryScale', () => {
  it('GPU dédié : VRAM puis RAM, sur une échelle de VRAM + RAM', () => {
    const s = memoryScale(BUDGET);
    expect(s.total).toBe(BUDGET.vram + BUDGET.ram);
    expect(s.vramEnd).toBeCloseTo(14.3 / 61.6);
    expect(s.unified).toBe(false);
    expect(s.gpuLimit).toBeNull();
  });

  it('mémoire unifiée : une seule zone de la taille de la RAM, repère du GPU', () => {
    const s = memoryScale({ ...BUDGET, vram: 9 * GIB, ram: 12 * GIB, unified: true });
    expect(s.total).toBe(12 * GIB);
    expect(s.vramEnd).toBe(0);
    expect(s.gpuLimit).toBeCloseTo(0.75);
  });

  it('sans GPU utilisable : tout est en RAM', () => {
    const s = memoryScale({ ...BUDGET, vram: 0, gpuBandwidthGbs: null });
    expect(s.total).toBe(BUDGET.ram);
    expect(s.vramEnd).toBe(0);
  });
});

describe('footprint', () => {
  const scale = memoryScale(BUDGET);
  const total = BUDGET.vram + BUDGET.ram;

  it('entièrement en VRAM', () => {
    const f = footprint(scale, 6.3 * GIB, 0);
    expect(f.gpu).toEqual({ start: 0, width: (6.3 * GIB) / total });
    expect(f.ram).toBeNull();
    expect(f.overflow).toBe(false);
  });

  it('hybride : la part en RAM commence à la zone RAM', () => {
    const f = footprint(scale, 13.9 * GIB, 6 * GIB);
    expect(f.gpu?.width).toBeCloseTo(13.9 / 61.6);
    expect(f.ram?.start).toBeCloseTo(scale.vramEnd);
    expect(f.ram?.width).toBeCloseTo(6 / 61.6);
  });

  it('CPU : seulement dans la zone RAM', () => {
    const f = footprint(scale, 0, 5 * GIB);
    expect(f.gpu).toBeNull();
    expect(f.ram?.start).toBeCloseTo(scale.vramEnd);
  });

  it('trop gros : rogné à la fin de l’échelle et signalé', () => {
    const f = footprint(scale, 14.3 * GIB, 60 * GIB);
    expect(f.overflow).toBe(true);
    expect(f.ram!.start + f.ram!.width).toBeCloseTo(1);
  });

  it('mémoire unifiée : la part du CPU suit celle du GPU', () => {
    const unified = memoryScale({ ...BUDGET, vram: 9 * GIB, ram: 12 * GIB, unified: true });
    const f = footprint(unified, 6 * GIB, 3 * GIB);
    expect(f.gpu).toEqual({ start: 0, width: 0.5 });
    expect(f.ram).toEqual({ start: 0.5, width: 0.25 });
    expect(f.overflow).toBe(false);
  });
});
