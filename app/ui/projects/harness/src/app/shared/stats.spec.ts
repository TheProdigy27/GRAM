import type { ChatMessage } from '@gram/types/ChatMessage';
import type { ChatStats } from '@gram/types/ChatStats';
import { GRAM_MODEL, OTHER_MODEL, VISION_MODEL } from '../../testing/fixtures';
import { contextUsage, duration, gramEstimate, rate, speedFacts, tokensK } from './stats';

const STATS: ChatStats = {
  doneReason: 'stop',
  promptEvalCount: 3000,
  evalCount: 128,
  evalDuration: 3_120_000_000,
  loadDuration: 2_000_000,
  promptEvalDuration: 60_000_000,
  totalDuration: 3_200_000_000,
  contextLength: null,
};

function answer(stats: Partial<ChatStats> | null, model = GRAM_MODEL.name): ChatMessage {
  return {
    role: 'assistant',
    content: 'x',
    images: [],
    thinking: null,
    thinkingMs: null,
    model,
    stats: stats && { ...STATS, ...stats },
    interrupted: false,
    error: null,
    createdAt: '2026-11-17T10:00:00Z',
  };
}

describe('Mesures d’une réponse', () => {
  it('vitesse : tokens, tok/s, chargement seulement s’il a eu lieu', () => {
    expect(speedFacts(STATS)).toEqual(['128 tokens', '41 tok/s']);
    expect(speedFacts({ ...STATS, loadDuration: 2_100_000_000 })).toEqual([
      '128 tokens',
      '41 tok/s',
      'chargement 2,1 s',
    ]);
    expect(speedFacts({ ...STATS, evalCount: 1, evalDuration: 200_000_000 })).toEqual([
      '1 token',
      '5 tok/s',
    ]);
    expect(speedFacts({ ...STATS, evalCount: 0, evalDuration: 0 })).toEqual(['0 token']);
    expect(speedFacts({ ...STATS, evalCount: 1165 })[0]).toBe('1\u202f165 tokens'); // espace fine insécable des milliers
    expect(rate(7.46)).toBe('7,5');
    expect(rate(43.4)).toBe('43');
  });

  it('estimation de GRAM, pour un modèle ajouté depuis GRAM', () => {
    expect(gramEstimate(GRAM_MODEL)).toBe('GRAM estimait 45 tok/s (mesuré 43)');
    expect(gramEstimate(VISION_MODEL)).toBe('GRAM estimait 88 tok/s');
    expect(gramEstimate(OTHER_MODEL)).toBeNull();
    expect(gramEstimate(undefined)).toBeNull();
  });

  it('durée de la réflexion', () => {
    expect(duration(300)).toBe('< 1 s');
    expect(duration(12_400)).toBe('12 s');
    expect(duration(59_999)).toBe('59 s');
    expect(duration(65_000)).toBe('1 min 5 s');
  });

  it('tokens en k', () => {
    expect(tokensK(850)).toBe('850');
    expect(tokensK(3254)).toBe('3,2k');
    expect(tokensK(8192)).toBe('8k');
    expect(tokensK(12_800)).toBe('13k');
  });

  it('jauge : dernière réponse mesurée, contexte chargé puis celui du modèle, seuils', () => {
    expect(contextUsage([], [GRAM_MODEL])).toBeNull();
    expect(contextUsage([answer(null)], [GRAM_MODEL])).toBeNull();

    // Contexte du modèle (16k) faute de `/api/ps`.
    const ok = contextUsage([answer({ contextLength: null })], [GRAM_MODEL])!;
    expect(ok).toEqual({ used: 3128, max: 16384, level: 'ok', label: '3,1k / 16k' });
    // Contexte du modèle chargé, prioritaire.
    expect(contextUsage([answer({ contextLength: 4096 })], [GRAM_MODEL])!.label).toBe('3,1k / 4k');
    // 80 % : on prévient ; 100 % : sorti du contexte.
    const at = (used: number) =>
      contextUsage([answer({ promptEvalCount: used - 128, contextLength: 10_000 })], [])!.level;
    expect(at(7999)).toBe('ok');
    expect(at(8000)).toBe('warn');
    expect(at(10_000)).toBe('full');
    // Contexte inconnu (modèle de la bibliothèque d'Ollama, `/api/ps` muet).
    expect(contextUsage([answer({}, OTHER_MODEL.name)], [OTHER_MODEL])).toEqual({
      used: 3128,
      max: null,
      level: 'ok',
      label: '3,1k tokens',
    });
    // La dernière réponse mesurée compte (une réponse interrompue n'a pas de mesure).
    const messages = [answer({ promptEvalCount: 100, evalCount: 50 }), answer(null)];
    expect(contextUsage(messages, [GRAM_MODEL])!.used).toBe(150);
  });
});
