import { BUDGET, MEASUREMENT } from '../../../testing/fixtures';
import { memoryDumbbell, speedDumbbell } from './measure-chart';

describe('graphiques de la mesure', () => {
  it('vitesse : mesuré et estimé sur un axe rond, écart, textes de part et d’autre', () => {
    const d = speedDumbbell(MEASUREMENT);
    // 95 × 1,15 → axe 0 → 150 tok/s.
    expect(d.ticks.map((t) => t.label)).toEqual(['0', '75', '150 tok/s']);
    expect(d.measured.pct).toBeCloseTo((83 / 150) * 100);
    expect(d.estimated.pct).toBeCloseTo((95 / 150) * 100);
    expect([d.measured.side, d.estimated.side]).toEqual(['left', 'right']);
    expect(d.span.width).toBeCloseTo((12 / 150) * 100);
    expect(d.gap).toBe('+14 %');
    expect(d.marker).toBeNull();
  });

  it('mémoire : repère de la VRAM utilisable, rien si Ollama ne l’a pas donnée', () => {
    const d = memoryDumbbell(MEASUREMENT, BUDGET)!;
    // VRAM 14,3 Go × 1,12 → axe 0 → 20 Go.
    expect(d.ticks.map((t) => t.label)).toEqual(['0', '10', '20 Go']);
    expect(d.marker?.label).toBe('VRAM 14,3');
    expect(d.marker?.pct).toBeCloseTo((14.3 / 20) * 100);
    expect([d.measured.label, d.estimated.label]).toEqual(['6,5', '6,3']);
    // Mesuré plus grand qu'estimé : son texte à droite.
    expect([d.measured.side, d.estimated.side]).toEqual(['right', 'left']);
    expect(d.gap).toBe('−2 %');
    expect(d.ariaLabel).toBe(
      'Mémoire mesurée 6,5 Go, estimée 6,3 Go : écart −2 %. VRAM utilisable : 14,3 Go.',
    );

    expect(memoryDumbbell({ ...MEASUREMENT, memTotal: null }, BUDGET)).toBeNull();
    const cpu = memoryDumbbell(MEASUREMENT, { ...BUDGET, vram: 0, gpuBandwidthGbs: null })!;
    expect(cpu.marker).toBeNull();
    expect(cpu.ticks.at(-1)!.label).toBe('8 Go');
    expect(cpu.measured.pct).toBeCloseTo((6.45 / 8) * 100);
  });
});
