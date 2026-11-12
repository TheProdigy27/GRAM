import { niceAxis } from './chart';

describe('niceAxis', () => {
  it('choisit un pas rond et au plus quatre intervalles', () => {
    expect(niceAxis(82)).toEqual({ max: 100, step: 25, ticks: [0, 25, 50, 75, 100] });
    expect(niceAxis(100)).toEqual({ max: 100, step: 25, ticks: [0, 25, 50, 75, 100] });
    expect(niceAxis(9.7).ticks).toEqual([0, 2.5, 5, 7.5, 10]);
    expect(niceAxis(160).ticks).toEqual([0, 50, 100, 150, 200]);
    expect(niceAxis(3.1, 3).ticks).toEqual([0, 2, 4]);
  });

  it('valeur nulle ou invalide : axe de 0 à 1', () => {
    expect(niceAxis(0).max).toBe(1);
    expect(niceAxis(Number.NaN).max).toBe(1);
  });
});
