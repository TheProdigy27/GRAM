import { speedWord } from './labels';

describe('speedWord', () => {
  it('seuils fixes, puis le seuil de lenteur du cas d’usage', () => {
    expect(speedWord(106, 10)).toBe('instantané');
    expect(speedWord(80, 10)).toBe('instantané');
    expect(speedWord(33, 10)).toBe('très fluide');
    expect(speedWord(15, 10)).toBe('fluide');
    expect(speedWord(12, 10)).toBe('correct');
    expect(speedWord(10, 10)).toBe('correct');
    expect(speedWord(9.9, 10)).toBe('lent');
  });
});
