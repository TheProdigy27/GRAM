import { BUDGET, GIB, makeEstimate } from '../../testing/fixtures';
import { memoryBar } from './memory-bar';
import { scoreTooltip } from './score';
import { makeScore } from '../../testing/fixtures';

const filled = (m: ReturnType<typeof memoryBar>) => m.segments.reduce((s, x) => s + x.pct, 0);

describe('memoryBar', () => {
  it('GPU : postes sur une échelle qui contient la VRAM utilisable, repère à 90 %', () => {
    const e = makeEstimate({ weights: 4.7 * GIB, kv: 1.1 * GIB });
    const m = memoryBar(e, BUDGET);
    expect(m.segments.map((s) => s.key)).toEqual(['weights', 'kv', 'overhead']);
    expect(m.total).toBeCloseTo(6.3 * GIB);
    expect(m.capacity).toBe(BUDGET.vram);
    expect(m.overflow).toBe(false);
    expect(m.ram).toBeNull();
    expect(m.over).toBeNull();
    // Échelle = VRAM / 0,9 : la barre est remplie à 6,3 / 14,3 de 90 %.
    expect(filled(m)).toBeCloseTo((6.3 / 14.3) * 90, 1);
    expect(m.markers).toEqual([{ label: 'VRAM utilisable', bytes: BUDGET.vram, pct: 90 }]);
  });

  it('hybride : l’échelle suit le besoin, la part en RAM est la fin de la barre', () => {
    const e = makeEstimate({
      weights: 14 * GIB,
      kv: 0.5 * GIB,
      mode: { kind: 'hybrid', gpuLayers: 62, layers: 65 },
    });
    const m = memoryBar(e, BUDGET);
    expect(m.capacity).toBe(BUDGET.vram + BUDGET.ram);
    expect(filled(m)).toBeCloseTo(90);
    expect(m.markers).toHaveLength(1);
    expect(m.markers[0].label).toBe('VRAM utilisable');
    expect(m.markers[0].pct).toBeCloseTo((14.3 / 15) * 90);
    // memCpu = 15 − 0,97 × 14,3 Go, à la fin du besoin.
    expect(m.ramBytes).toBeCloseTo(e.memCpu);
    expect(m.ram!.start + m.ram!.width).toBeCloseTo(90);
    expect(m.ram!.width).toBeCloseTo((e.memCpu / (15 * GIB)) * 90);
  });

  it('mémoire unifiée : la part du GPU est prise sur la RAM, sans s’y ajouter', () => {
    const mac = { ...BUDGET, vram: 10.7 * GIB, ram: 12 * GIB, unified: true };
    const hybrid = memoryBar(
      makeEstimate({ weights: 11 * GIB, mode: { kind: 'hybrid', gpuLayers: 30, layers: 40 } }),
      mac,
    );
    expect(hybrid.capacity).toBe(12 * GIB);
    expect(hybrid.markers.map((m) => m.label)).toEqual(['Mémoire du GPU']);
    const tooBig = memoryBar(makeEstimate({ weights: 40 * GIB, mode: { kind: 'tooBig' } }), mac);
    expect(tooBig.capacity).toBe(12 * GIB);
    expect(tooBig.markers.map((m) => m.label)).toEqual([
      'Mémoire du GPU',
      'RAM utilisable (GPU compris)',
    ]);
    expect(memoryBar(makeEstimate(), mac).markers[0].label).toBe('Mémoire du GPU');
  });

  it('trop gros : repères de la VRAM et de la mémoire utilisable, dépassement au-delà', () => {
    const e = makeEstimate({ weights: 80 * GIB, mode: { kind: 'tooBig' } });
    const m = memoryBar(e, BUDGET);
    expect(m.overflow).toBe(true);
    expect(filled(m)).toBeCloseTo(90);
    expect(m.markers.map((x) => x.label)).toEqual(['VRAM utilisable', 'VRAM + RAM utilisables']);
    const limit = m.markers.at(-1)!;
    expect(limit.pct).toBeLessThan(90);
    expect(m.over!.start).toBeCloseTo(limit.pct);
    expect(m.over!.start + m.over!.width).toBeCloseTo(90);
    expect(m.ram).toBeNull();
  });

  it('CPU seul : face à la RAM', () => {
    const e = makeEstimate({ mode: { kind: 'cpu' } });
    const m = memoryBar(e, { ...BUDGET, vram: 0, gpuBandwidthGbs: null });
    expect(m.capacity).toBe(BUDGET.ram);
    expect(m.markers.map((x) => x.label)).toEqual(['RAM utilisable']);
    expect(m.markers[0].pct).toBeCloseTo(90);
    expect(m.ram).toBeNull();
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
