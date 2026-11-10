import { BUDGET, GIB, makeEstimate } from '../../testing/fixtures';
import { memoryBar } from './memory-bar';
import { scoreTooltip } from './score';
import { makeScore } from '../../testing/fixtures';

describe('memoryBar', () => {
  it('GPU : postes face à la VRAM utilisable', () => {
    const e = makeEstimate({ weights: 4.7 * GIB, kv: 1.1 * GIB });
    const m = memoryBar(e, BUDGET);
    expect(m.segments.map((s) => s.key)).toEqual(['weights', 'kv', 'overhead']);
    expect(m.total).toBeCloseTo(6.3 * GIB);
    expect(m.capacity).toBe(BUDGET.vram);
    expect(m.overflow).toBe(false);
    // Échelle = VRAM : la barre est remplie à 6,3 / 14,3.
    const filled = m.segments.reduce((s, x) => s + x.pct, 0);
    expect(filled).toBeCloseTo((6.3 / 14.3) * 100, 1);
    expect(m.markers).toEqual([{ label: 'VRAM utilisable', bytes: BUDGET.vram, pct: 100 }]);
  });

  it('hybride : repère à la fin de la VRAM, échelle VRAM + RAM', () => {
    const e = makeEstimate({
      weights: 40 * GIB,
      mode: { kind: 'hybrid', gpuLayers: 37, layers: 80 },
    });
    const m = memoryBar(e, BUDGET);
    const capacity = BUDGET.vram + BUDGET.ram;
    expect(m.capacity).toBe(capacity);
    expect(m.markers[0].label).toBe('VRAM');
    expect(m.markers[0].pct).toBeCloseTo((BUDGET.vram / capacity) * 100);
  });

  it('trop gros : l’échelle suit le besoin pour montrer le dépassement', () => {
    const e = makeEstimate({ weights: 80 * GIB, mode: { kind: 'tooBig' } });
    const m = memoryBar(e, BUDGET);
    expect(m.overflow).toBe(true);
    expect(m.segments.reduce((s, x) => s + x.pct, 0)).toBeCloseTo(100);
    const limit = m.markers.at(-1)!;
    expect(limit.label).toBe('VRAM + RAM');
    expect(limit.pct).toBeLessThan(100);
  });

  it('CPU seul : face à la RAM', () => {
    const e = makeEstimate({ mode: { kind: 'cpu' } });
    const m = memoryBar(e, { ...BUDGET, vram: 0, gpuBandwidthGbs: null });
    expect(m.capacity).toBe(BUDGET.ram);
    expect(m.markers.map((x) => x.label)).toEqual(['RAM utilisable']);
  });
});

describe('scoreTooltip', () => {
  it('détaille les axes, puis les facteurs seulement s’ils s’appliquent', () => {
    const plain = scoreTooltip(makeScore(84)).split('\n');
    expect(plain[0]).toBe('Score 84 sur 100');
    expect(plain[1]).toBe('Qualité 82 (poids 0,45)');
    expect(plain).toHaveLength(6);

    const penalized = scoreTooltip(makeScore(70, { useCaseFactor: 0.92, confidence: 0.95 }));
    expect(penalized).toContain('× 0,92 : ne correspond pas au cas d’usage');
    expect(penalized).toContain('× 0,95 : estimation approximative');
  });
});
