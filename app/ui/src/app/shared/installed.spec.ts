import { INSTALLED_MODELS, SHARED_MODEL } from '../../testing/fixtures';
import type { HarnessEntry } from '../core/generated/HarnessEntry';
import type { OllamaModel } from '../core/generated/OllamaModel';
import { installedModels, modelCount, otherTitle, sameOllamaName } from './installed';

const GIB = 2 ** 30;
const entry = (o: Partial<HarnessEntry['model']> = {}): HarnessEntry => ({
  model: { ...SHARED_MODEL, ...o },
  baseRemovable: false,
});
const names = (rows: { model: OllamaModel }[]) => rows.map((r) => r.model.name);

describe('modèles installés : lignes de la liste', () => {
  it('même nom Ollama : casse et espaces autour ignorés', () => {
    expect(sameOllamaName('GRAM/Qwen3-8B:Q4_K_M ', 'gram/qwen3-8b:q4_k_m')).toBe(true);
    expect(sameOllamaName('qwen3:8b', 'qwen3:8b-q8_0')).toBe(false);
  });

  it('titre lisible : dépôt sans propriétaire ni « -GGUF », puis la quantification', () => {
    const m = (name: string, repoId: string | null, quant: string | null): OllamaModel => ({
      name,
      repoId,
      quant,
      size: 0,
    });
    expect(
      otherTitle(m('hf.co/unsloth/Qwen3-1.7B-GGUF:Q4_K_M', 'unsloth/Qwen3-1.7B-GGUF', 'Q4_K_M')),
    ).toBe('Qwen3-1.7B Q4_K_M');
    expect(otherTitle(m('hf.co/org/Model_gguf', 'org/Model_gguf', null))).toBe('Model');
    // Bibliothèque d'Ollama : le nom tel quel (l'étiquette dit déjà la taille).
    expect(otherTitle(m('qwen3:8b', null, 'Q4_K_M'))).toBe('qwen3:8b');
  });

  it('un modèle de GRAM Harness et sa base en une ligne, puis les autres par nom', () => {
    const r = installedModels(INSTALLED_MODELS, [entry()]);
    expect(r.harness.map((h) => h.entry.model.ollamaName)).toEqual(['gram/qwen3-8b:q4_k_m']);
    expect(r.harness[0].size).toBe(4.7 * GIB);
    // La base `hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M` est masquée.
    expect(names(r.others)).toEqual([
      'hf.co/huihui-ai/Huihui-DeepSeek-V4.1-Flash-1017-abliterated-GGUF:IQ1_S',
      'hf.co/unsloth/Qwen3-1.7B-GGUF:Q4_K_M',
      'qwen3:8b',
    ]);
    expect(r.count).toBe(4);
    expect(r.size).toBeCloseTo((4.7 + 1.67 + 1.1 + 5.2) * GIB, 0);
    expect(modelCount(r.count)).toBe('4 modèles');
    expect(modelCount(1)).toBe('1 modèle');
  });

  it('entrée dont le modèle dérivé a disparu d’Ollama : ignorée, sa base reste visible', () => {
    const r = installedModels(INSTALLED_MODELS, [entry({ ollamaName: 'gram/autre:q4_k_m' })]);
    expect(r.harness).toEqual([]);
    expect(names(r.others)).toContain('hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M');
    expect(names(r.others)).toContain('gram/qwen3-8b:q4_k_m');
    expect(r.count).toBe(INSTALLED_MODELS.length);
  });

  it('GRAM Harness trié par nom lisible puis quantification ; base absente acceptée', () => {
    const ollama: OllamaModel[] = ['gram/b:q8_0', 'gram/b:q4_k_m', 'gram/a:q4_k_m'].map((name) => ({
      name,
      repoId: null,
      quant: null,
      size: GIB,
    }));
    const r = installedModels(ollama, [
      entry({ ollamaName: 'gram/b:q8_0', displayName: 'b', quant: 'Q8_0' }),
      entry({ ollamaName: 'gram/a:q4_k_m', displayName: 'A', quant: 'Q4_K_M' }),
      entry({ ollamaName: 'gram/b:q4_k_m', displayName: 'B', quant: 'Q4_K_M' }),
    ]);
    expect(r.harness.map((h) => h.entry.model.ollamaName)).toEqual([
      'gram/a:q4_k_m',
      'gram/b:q4_k_m',
      'gram/b:q8_0',
    ]);
    expect(r.others).toEqual([]);
  });

  it('Ollama vide', () => {
    expect(installedModels([], [entry()])).toEqual({ harness: [], others: [], count: 0, size: 0 });
  });
});
