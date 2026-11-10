import { GIB, makeItem } from '../../../testing/fixtures';
import { rankedRows, sortRows } from './sort';

const items = [
  makeItem('Bravo', { score: 90, tokS: 20, size: 8 * GIB }),
  makeItem('alpha', { score: 80, tokS: 60, size: 4 * GIB, mode: { kind: 'cpu' } }),
  makeItem('Charlie', {
    score: 70,
    tokS: 60,
    size: 12 * GIB,
    mode: { kind: 'hybrid', gpuLayers: 1, layers: 2 },
  }),
];
const names = (rows: ReturnType<typeof sortRows>) => rows.map((r) => r.item.displayName);

describe('tri du classement', () => {
  it('numérote dans l’ordre du score et le garde sans tri actif', () => {
    const rows = rankedRows(items);
    expect(rows.map((r) => r.rank)).toEqual([1, 2, 3]);
    expect(names(sortRows(rows, '', ''))).toEqual(['Bravo', 'alpha', 'Charlie']);
    expect(names(sortRows(rows, 'tokS', ''))).toEqual(['Bravo', 'alpha', 'Charlie']);
  });

  it('trie par colonne ; à égalité, l’ordre du score', () => {
    const rows = rankedRows(items);
    expect(names(sortRows(rows, 'tokS', 'desc'))).toEqual(['alpha', 'Charlie', 'Bravo']);
    expect(names(sortRows(rows, 'size', 'asc'))).toEqual(['alpha', 'Bravo', 'Charlie']);
    expect(names(sortRows(rows, 'model', 'asc'))).toEqual(['alpha', 'Bravo', 'Charlie']);
    expect(names(sortRows(rows, 'mode', 'asc'))).toEqual(['Bravo', 'Charlie', 'alpha']);
    expect(names(sortRows(rows, 'rank', 'desc'))).toEqual(['Charlie', 'alpha', 'Bravo']);
  });
});
