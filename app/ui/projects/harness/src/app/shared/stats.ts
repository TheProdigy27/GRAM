// Mesures d'une réponse : vitesse, durée de la réflexion, contexte occupé.

import { ctxK, decimal, int } from '@gram/format';
import type { ChatMessage } from '@gram/types/ChatMessage';
import type { ChatStats } from '@gram/types/ChatStats';
import type { HarnessModel } from '@gram/types/HarnessModel';

/** Au-delà, le modèle a dû être chargé (déjà en mémoire : quelques millisecondes). */
export const LOAD_SHOWN_NS = 500_000_000;
/** Part du contexte à partir de laquelle la jauge prévient. */
export const CONTEXT_WARN = 0.8;

/** Vitesse mesurée → « 41 », « 7,5 » sous 10 (sans « ~ » : ce n'est pas une estimation). */
export function rate(tokS: number): string {
  return tokS < 10 ? decimal(tokS, 1) : int(Math.round(tokS));
}

/** Tokens → « 850 », « 3,2k », « 12k » (multiples de 1 024, comme le contexte). */
export function tokensK(n: number): string {
  if (n < 1024) return int(n);
  const k = n / 1024;
  return k < 10 ? `${decimal(k, 1)}k` : `${int(Math.round(k))}k`;
}

/** Durée → « < 1 s », « 12 s », « 1 min 5 s ». */
export function duration(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 1) return '< 1 s';
  if (s < 60) return `${s} s`;
  return `${Math.floor(s / 60)} min ${s % 60} s`;
}

/**
 * Faits sous une réponse : « 128 tokens », « 41 tok/s », et « chargement 2,1 s » quand le
 * modèle a dû être chargé.
 */
export function speedFacts(stats: ChatStats): string[] {
  const n = stats.evalCount;
  const facts = [`${int(n)} token${n > 1 ? 's' : ''}`];
  if (n > 0 && stats.evalDuration > 0) {
    facts.push(`${rate(n / (stats.evalDuration / 1e9))} tok/s`);
  }
  if (stats.loadDuration >= LOAD_SHOWN_NS) {
    facts.push(`chargement ${decimal(stats.loadDuration / 1e9, 1)} s`);
  }
  return facts;
}

/** Estimation de GRAM pour un modèle ajouté depuis GRAM : « GRAM estimait 45 tok/s (mesuré 43) ». */
export function gramEstimate(model: HarnessModel | undefined): string | null {
  if (!model?.fromGram || !model.estimatedTokS) return null;
  const measured = model.measuredTokS ? ` (mesuré ${rate(model.measuredTokS)})` : '';
  return `GRAM estimait ${rate(model.estimatedTokS)} tok/s${measured}`;
}

export interface ContextUsage {
  /** Tokens de la conversation et de la dernière réponse. */
  used: number;
  /** Contexte du modèle (chargé, sinon réglé) ; `null` : inconnu. */
  max: number | null;
  level: 'ok' | 'warn' | 'full';
  /** « 3,2k / 16k », ou « 3,2k tokens » sans contexte connu. */
  label: string;
}

/**
 * Contexte occupé à la fin de la dernière réponse : `prompt_eval_count + eval_count`, rapporté
 * au contexte du modèle chargé (`/api/ps`), à défaut à celui du modèle (`num_ctx` ou
 * manifeste). `null` sans réponse mesurée.
 */
export function contextUsage(
  messages: readonly ChatMessage[],
  models: readonly HarnessModel[],
): ContextUsage | null {
  // Sans `findLast` (Safari 15.4).
  const last = [...messages].reverse().find((m) => m.role === 'assistant' && m.stats);
  const stats = last?.stats;
  if (!stats) return null;
  const used = stats.promptEvalCount + stats.evalCount;
  const max = stats.contextLength ?? models.find((m) => m.name === last.model)?.ctx ?? null;
  const ratio = max ? used / max : 0;
  return {
    used,
    max,
    level: ratio >= 1 ? 'full' : ratio >= CONTEXT_WARN ? 'warn' : 'ok',
    label: max ? `${tokensK(used)} / ${ctxK(max)}` : `${tokensK(used)} tokens`,
  };
}
