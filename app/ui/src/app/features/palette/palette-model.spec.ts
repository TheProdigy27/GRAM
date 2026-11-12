import { makeItem, makeResponse } from '../../../testing/fixtures';
import {
  type PaletteCommand,
  foundCount,
  foundModels,
  matchesWords,
  matchingCommands,
  paletteGroups,
  searchWords,
} from './palette-model';

const command = (id: string, label: string, o: Partial<PaletteCommand> = {}): PaletteCommand => ({
  id,
  label,
  hint: '',
  icon: 'search',
  keys: [],
  run: () => undefined,
  ...o,
});

const COMMANDS = [
  command('rescan', 'Ré-analyser le matériel', { hint: 'GPU, RAM, runtimes' }),
  command('settings', 'Ouvrir les paramètres', { keywords: 'réglages' }),
  command('ctx-32k', 'Contexte : 32k', { searchOnly: true }),
];

describe('palette-model', () => {
  it('mots de la saisie', () => {
    expect(searchWords('  Qwen3   8B ')).toEqual(['qwen3', '8b']);
    expect(searchWords('   ')).toEqual([]);
  });

  it('même règle que la recherche Rust : séparateurs ignorés pour les mots alphanumériques', () => {
    const match = (query: string, ...texts: string[]) => matchesWords(searchWords(query), texts);
    expect(match('llama3', 'Llama-3.3-70B-Instruct')).toBe(true);
    expect(match('qwen3 8b', 'Qwen3-8B')).toBe(true);
    expect(match('qwen3 8b', 'Qwen3-VL-8B-Instruct')).toBe(true);
    expect(match('qwen3 8b', 'Qwen3-30B')).toBe(false);
    // Un mot avec un séparateur est cherché tel quel.
    expect(match('3.3', 'Qwen3-30B')).toBe(false);
    expect(match('3.3', 'Llama-3.3-70B')).toBe(true);
    expect(match('bartowski/qwen3', 'gemma', 'bartowski/Qwen3-8B-GGUF')).toBe(true);
    // Accents ignorés (commandes).
    expect(match('reanalyser', 'Ré-analyser le matériel')).toBe(true);
    expect(match('ré', 'Re-analyser')).toBe(true);
  });

  it('commandes : principales sans saisie, toutes celles qui correspondent sinon', () => {
    const ids = (q: string) => matchingCommands(COMMANDS, searchWords(q)).map((c) => c.id);
    expect(ids('')).toEqual(['rescan', 'settings']);
    expect(ids('32k')).toEqual(['ctx-32k']);
    expect(ids('gpu')).toEqual(['rescan']);
    expect(ids('reglages')).toEqual(['settings']);
    expect(ids('qwen')).toEqual([]);
  });

  it('modèles trouvés : exécutables, lents puis trop gros, six au plus', () => {
    const items = ['A', 'B', 'C', 'D', 'E'].map((n, i) => makeItem(n, { rank: i + 1 }));
    const r = makeResponse(items, {
      totalItems: 12,
      slow: [makeItem('Lent', { rank: 3 })],
      totalSlow: 4,
      tooBig: [makeItem('Gros', { rank: 1 })],
      tooBigCount: 2,
    });
    const found = foundModels(r);
    expect(found.map((m) => m.displayName)).toEqual(['A', 'B', 'C', 'D', 'E', 'Lent']);
    expect(found[5].section).toBe('slow');
    expect(foundModels(r, Infinity).at(-1)).toMatchObject({
      displayName: 'Gros',
      section: 'tooBig',
    });
    expect(foundCount(r)).toBe(18);
  });

  it('groupes : recherche, modèles, commandes ; sans saisie, récents et commandes', () => {
    const models = foundModels(makeResponse([makeItem('Qwen3-8B')]));
    const withQuery = paletteGroups('  qwen3   8b ', models, COMMANDS, 3);
    expect(withQuery.map((g) => g.label)).toEqual(['Recherche', 'Modèles']);
    expect(withQuery[0].options).toEqual([{ kind: 'filter', query: 'qwen3 8b', count: 3 }]);

    const empty = paletteGroups('', models, COMMANDS, null);
    expect(empty.map((g) => g.label)).toEqual(['Récents', 'Commandes']);
    expect(empty[1].options).toHaveLength(2);
    expect(paletteGroups('', [], COMMANDS, null).map((g) => g.label)).toEqual(['Commandes']);
  });
});
