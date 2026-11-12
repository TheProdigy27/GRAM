import { GIB, makeDetail, makeEstimate, makeScore } from '../../../testing/fixtures';
import type { VariantDetail } from '../../core/generated/VariantDetail';
import { gpuSuggestion } from './suggestion';

/** Q3 et Q4 en VRAM, Q5 hybride (la choisie dans la plupart des cas). */
function variants(): VariantDetail[] {
  const base = makeDetail().variants[0];
  const make = (label: string, size: number, o: Partial<VariantDetail> = {}): VariantDetail => ({
    ...base,
    variant: { ...base.variant, label, sizeBytes: size * GIB },
    ...o,
  });
  return [
    make('Q3_K_L', 3.9, {
      estimate: makeEstimate({ weights: 3.9 * GIB, tokS: 110 }),
      score: makeScore(80, { quality: 70 }),
    }),
    make('Q4_K_M', 4.7, {
      estimate: makeEstimate({ weights: 4.7 * GIB, tokS: 53, ctxMax: 10240 }),
      score: makeScore(84, { quality: 80 }),
    }),
    make('Q5_K_M', 14.6, {
      estimate: makeEstimate({
        weights: 14.6 * GIB,
        mode: { kind: 'hybrid', gpuLayers: 62, layers: 65 },
        tokS: 33,
        ctxMax: 209920,
      }),
      score: makeScore(86, { quality: 84 }),
    }),
  ];
}

describe('gpuSuggestion', () => {
  it('hybride : la plus grosse quantification qui tient en VRAM, gain et contexte', () => {
    expect(gpuSuggestion(variants(), 'Q5_K_M')).toEqual({
      label: 'Q4_K_M',
      speed: '~53 tok/s',
      after: ' (+61 %) pour une qualité à peine moindre, mais 10k de contexte au plus.',
    });
  });

  it('rien à suggérer : déjà en VRAM, aucune en VRAM, ou gain trop faible', () => {
    expect(gpuSuggestion(variants(), 'Q4_K_M')).toBeNull();
    const noGpu = variants().map((v) => ({
      ...v,
      estimate: { ...v.estimate, mode: { kind: 'cpu' as const } },
    }));
    expect(gpuSuggestion(noGpu, 'Q5_K_M')).toBeNull();
    const close = variants();
    close[2].estimate = { ...close[2].estimate, tokS: 50 };
    expect(gpuSuggestion(close, 'Q5_K_M')).toBeNull();
  });

  it('préfère une quantification sous la taille maximale ; qualité nettement moindre', () => {
    const list = variants();
    list[1].withinSize = false;
    expect(gpuSuggestion(list, 'Q5_K_M')).toEqual({
      label: 'Q3_K_L',
      speed: '~110 tok/s',
      after: ' (+233 %) mais une qualité nettement moindre et 40k de contexte au plus.',
    });
  });
});
