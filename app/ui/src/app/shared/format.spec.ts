import {
  ctxK,
  ctxToStep,
  count,
  gb,
  params,
  relativeTime,
  roundSize,
  stepToCtx,
  tokS,
} from './format';

/** Intl met une espace fine insécable entre les milliers : on la normalise pour comparer. */
const plain = (s: string) => s.replace(/[  ]/g, ' ');
const GIB = 2 ** 30;

describe('format', () => {
  it('tailles en Go (Gio), comme le Gestionnaire des tâches', () => {
    expect(gb(16 * GIB)).toBe('16,0 Go');
    expect(gb(6.34 * GIB)).toBe('6,3 Go');
    expect(roundSize(15.9 * GIB)).toBe('16 Go');
    expect(plain(roundSize(1862 * GIB))).toBe('1,8 To');
  });

  it('vitesse : même règle que le backend', () => {
    expect(tokS(37.6)).toBe('~38');
    expect(tokS(5.62)).toBe('~5,6');
    expect(tokS(0)).toBe('—');
    expect(tokS(Number.NaN)).toBe('—');
  });

  it('contexte en k', () => {
    expect(ctxK(32768)).toBe('32k');
    expect(ctxK(40960)).toBe('40k');
    expect(ctxK(512)).toBe('512');
    expect(ctxK(0)).toBe('—');
  });

  it('paramètres, dont les MoE', () => {
    expect(params(8.2e9, null)).toBe('8,2 Md');
    expect(params(70.6e9, null)).toBe('71 Md');
    expect(params(6e8, null)).toBe('600 M');
    expect(params(30.5e9, 3.3e9)).toBe('31 Md (3,3 Md actifs)');
  });

  it('compteurs compacts', () => {
    expect(plain(count(1_234_567))).toBe('1,2 M');
    expect(plain(count(35_000))).toBe('35 k');
    expect(count(950)).toBe('950');
    expect(plain(count(907_700))).toBe('908 k');
  });

  it('dates relatives', () => {
    const now = Date.parse('2026-11-09T20:00:00Z');
    expect(relativeTime('2026-11-09T19:59:30Z', now)).toBe('à l’instant');
    expect(plain(relativeTime('2026-11-09T18:00:00Z', now))).toBe('il y a 2 heures');
    expect(relativeTime('2026-11-08T12:00:00Z', now)).toBe('hier');
    expect(plain(relativeTime('2026-05-01T00:00:00Z', now))).toBe('il y a 6 mois');
  });

  it('curseur de contexte : une puissance de deux par cran, de 2k à 128k', () => {
    expect(stepToCtx(0)).toBe(2048);
    expect(stepToCtx(2)).toBe(8192);
    expect(stepToCtx(6)).toBe(131072);
    expect(stepToCtx(99)).toBe(131072);
    expect(ctxToStep(8192)).toBe(2);
    expect(ctxToStep(12288)).toBe(3); // au plus proche (échelle logarithmique)
    expect(ctxToStep(1_048_576)).toBe(6);
    expect(ctxToStep(100)).toBe(0);
  });
});
