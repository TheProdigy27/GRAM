import { makeItem } from '../../../testing/fixtures';
import type { RankItem } from '../../core/generated/RankItem';
import {
  type ModelRow,
  type TreeRow,
  flatRows,
  groupFamilies,
  sortFamilies,
  treeRows,
} from './families';

const QWEN = 'org/Qwen3-8B-GGUF';

/** Classement : une variante de Qwen3-8B mieux classée que lui, deux autres plus bas. */
const ITEMS: RankItem[] = [
  makeItem('Qwen3-8B-abliterated', { rank: 1, score: 91, family: QWEN, tokS: 60 }),
  makeItem('Gemma-3-12B', { rank: 2, score: 88, tokS: 40 }),
  makeItem('Qwen3-8B', { rank: 3, score: 86, repoId: QWEN, tokS: 95 }),
  makeItem('Qwen3-8B-Heretic', { rank: 4, score: 84, family: QWEN, tokS: 70 }),
  makeItem('Qwen3-8B-Coder', { rank: 5, score: 80, family: QWEN, tokS: 99 }),
];

const names = (rows: TreeRow[]) =>
  rows.map((r) =>
    r.kind === 'model' ? `${'  '.repeat(r.level - 1)}${r.item.displayName}` : `  +${r.hidden}`,
  );

describe('familles', () => {
  it('regroupe sous le modèle officiel, qui garde sa place ; les variantes dans l’ordre', () => {
    const families = groupFamilies(ITEMS);
    expect(families.map((f) => f.head.displayName)).toEqual(['Gemma-3-12B', 'Qwen3-8B']);
    const qwen = families[1];
    expect(qwen.key).toBe(QWEN);
    expect(qwen.official).toBe(true);
    expect(qwen.variants.map((v) => v.displayName)).toEqual([
      'Qwen3-8B-abliterated',
      'Qwen3-8B-Heretic',
      'Qwen3-8B-Coder',
    ]);
  });

  it('sans le modèle officiel dans la liste : la meilleure variante mène la famille', () => {
    const families = groupFamilies(ITEMS.filter((i) => i.repoId !== QWEN));
    expect(families.map((f) => f.head.displayName)).toEqual([
      'Qwen3-8B-abliterated',
      'Gemma-3-12B',
    ]);
    expect(families[0].official).toBe(false);
    expect(families[0].variants).toHaveLength(2);
  });

  it('lignes : repliée, dépliée (3 variantes puis « Voir les N autres »), tout montré', () => {
    const many = [
      ...ITEMS,
      makeItem('Qwen3-8B-Distill', { rank: 6, score: 79, family: QWEN }),
      makeItem('Qwen3-8B-Roleplay', { rank: 7, score: 78, family: QWEN }),
    ];
    const families = groupFamilies(many);
    const closed = treeRows(
      families,
      () => false,
      () => false,
    );
    expect(names(closed)).toEqual(['Gemma-3-12B', 'Qwen3-8B']);
    expect((closed[0] as ModelRow).expanded).toBeNull();
    expect((closed[1] as ModelRow).expanded).toBe(false);

    const open = treeRows(
      families,
      () => true,
      () => false,
    );
    expect(names(open)).toEqual([
      'Gemma-3-12B',
      'Qwen3-8B',
      '  Qwen3-8B-abliterated',
      '  Qwen3-8B-Heretic',
      '  Qwen3-8B-Coder',
      '  +2',
    ]);
    expect(open.at(-1)).toMatchObject({ kind: 'more', id: `more:${QWEN}`, hidden: 2 });

    const all = treeRows(
      families,
      () => true,
      () => true,
    );
    expect(all).toHaveLength(7);
  });

  it('« mieux noté que l’officiel » : score affiché plus haut, et seulement sous l’officiel', () => {
    const tied = [...ITEMS, makeItem('Qwen3-8B-Tied', { rank: 6, score: 86.3, family: QWEN })];
    const rows = treeRows(
      groupFamilies(tied),
      () => true,
      () => true,
    ) as ModelRow[];
    const better = rows.filter((r) => r.better).map((r) => r.item.displayName);
    expect(better).toEqual(['Qwen3-8B-abliterated']);

    // Sans l'officiel, la ligne de famille est une variante : rien à signaler.
    const orphans = ITEMS.filter((i) => i.repoId !== QWEN);
    const rows2 = treeRows(
      groupFamilies(orphans),
      () => true,
      () => true,
    ) as ModelRow[];
    expect(rows2.some((r) => r.better)).toBe(false);
  });

  it('tri : les familles d’après leur ligne de famille, les variantes entre elles', () => {
    const sorted = sortFamilies(groupFamilies(ITEMS), 'tokS', 'desc');
    expect(
      names(
        treeRows(
          sorted,
          () => true,
          () => true,
        ),
      ),
    ).toEqual([
      'Qwen3-8B',
      '  Qwen3-8B-Coder',
      '  Qwen3-8B-Heretic',
      '  Qwen3-8B-abliterated',
      'Gemma-3-12B',
    ]);
  });

  it('sans regroupement : une ligne par modèle', () => {
    expect(names(flatRows(ITEMS, '', ''))).toEqual(ITEMS.map((i) => i.displayName));
  });
});
