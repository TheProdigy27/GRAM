import type { ConversationSummary } from '@gram/types/ConversationSummary';
import { GRAM_MODEL, MODELS, OTHER_MODEL } from '../../testing/fixtures';
import { groupConversations, modelLabel } from './conversations';

function summary(id: string, updated: Date): ConversationSummary {
  const at = updated.toISOString();
  return { id, title: id, model: 'm', createdAt: at, updatedAt: at };
}

describe('groupConversations', () => {
  // 13 novembre, 15 h (heure locale).
  const now = new Date(2026, 10, 13, 15, 0);

  it('aujourd’hui, 7 derniers jours, plus ancien, dans l’ordre de la liste', () => {
    const list = [
      summary('ce-matin', new Date(2026, 10, 13, 0, 5)),
      summary('hier-soir', new Date(2026, 10, 12, 23, 59)),
      summary('il-y-a-7-jours', new Date(2026, 10, 6, 0, 0)),
      summary('il-y-a-8-jours', new Date(2026, 10, 5, 23, 59)),
      summary('l-an-dernier', new Date(2025, 10, 13, 12, 0)),
    ];
    const groups = groupConversations(list, now);
    expect(groups.map((g) => [g.label, g.items.map((c) => c.id)])).toEqual([
      ['Aujourd’hui', ['ce-matin']],
      ['7 derniers jours', ['hier-soir', 'il-y-a-7-jours']],
      ['Plus ancien', ['il-y-a-8-jours', 'l-an-dernier']],
    ]);
  });

  it('groupes vides omis', () => {
    expect(groupConversations([], now)).toEqual([]);
    const groups = groupConversations([summary('vieux', new Date(2020, 0, 1))], now);
    expect(groups.map((g) => g.key)).toEqual(['older']);
  });
});

describe('modelLabel', () => {
  it('nom lisible et quantification pour un modèle de GRAM, nom technique sinon', () => {
    expect(modelLabel(MODELS.models, GRAM_MODEL.name)).toBe('Qwen3 8B Q4_K_M');
    expect(modelLabel(MODELS.models, OTHER_MODEL.name)).toBe('qwen3:8b');
    // Plus dans Ollama : le nom enregistré.
    expect(modelLabel(MODELS.models, 'gram/disparu:q8_0')).toBe('gram/disparu:q8_0');
  });
});
