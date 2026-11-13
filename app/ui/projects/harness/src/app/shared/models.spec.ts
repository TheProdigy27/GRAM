import { GRAM_MODEL, OTHER_MODEL, VISION_MODEL } from '../../testing/fixtures';
import { GRAM_GROUP, OTHER_GROUP, capabilities, modelFacts, modelOptions } from './models';

describe('mise en forme des modèles', () => {
  it('capacités dans un ordre fixe', () => {
    expect(capabilities(GRAM_MODEL).map((c) => c.label)).toEqual(['Réflexion', 'Outils']);
    expect(capabilities(VISION_MODEL).map((c) => c.key)).toEqual(['vision']);
    const all = { ...GRAM_MODEL, vision: true };
    expect(capabilities(all).map((c) => c.key)).toEqual(['vision', 'thinking', 'tools']);
  });

  it('options : nom lisible et quantification pour GRAM, nom technique sinon', () => {
    const options = modelOptions([GRAM_MODEL, OTHER_MODEL]);
    expect(options).toEqual([
      {
        value: GRAM_MODEL.name,
        label: 'Qwen3 8B Q4_K_M',
        hint: '4,7 Go',
        tags: ['réflexion', 'outils'],
        group: GRAM_GROUP,
      },
      {
        value: 'qwen3:8b',
        label: 'qwen3:8b',
        hint: '4,9 Go',
        tags: ['réflexion', 'outils'],
        group: OTHER_GROUP,
      },
    ]);
    // Modèle de GRAM sans quantification connue : le nom seul.
    expect(modelOptions([{ ...GRAM_MODEL, quant: null }])[0].label).toBe('Qwen3 8B');
  });

  it('faits : quantification, taille, contexte, vitesses de GRAM', () => {
    expect(modelFacts(GRAM_MODEL)).toEqual([
      'Q4_K_M',
      '4,7 Go',
      'contexte 16k',
      '~45 tok/s estimés',
      '~43 tok/s mesurés',
    ]);
    expect(modelFacts(VISION_MODEL)).toEqual([
      'Q4_K_M',
      '3,1 Go',
      'contexte 32k',
      '~88 tok/s estimés',
    ]);
    expect(modelFacts({ ...OTHER_MODEL, quant: null, size: 600_000_000 })).toEqual([
      '572 Mo',
      'contexte choisi par Ollama',
    ]);
  });
});
