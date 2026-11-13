// Groupes de la barre latérale et libellés des messages.

import type { ConversationSummary } from '@gram/types/ConversationSummary';
import type { HarnessModel } from '@gram/types/HarnessModel';

export interface ConversationGroup {
  key: 'today' | 'week' | 'older';
  label: string;
  items: ConversationSummary[];
}

/**
 * Conversations par date du dernier message (heure locale) : aujourd'hui, les 7 jours d'avant,
 * plus ancien. L'ordre de la liste (la plus récente d'abord) est gardé ; les groupes vides sont
 * omis.
 */
export function groupConversations(
  list: readonly ConversationSummary[],
  now: Date = new Date(),
): ConversationGroup[] {
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const week = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 7).getTime();
  const groups: ConversationGroup[] = [
    { key: 'today', label: 'Aujourd’hui', items: [] },
    { key: 'week', label: '7 derniers jours', items: [] },
    { key: 'older', label: 'Plus ancien', items: [] },
  ];
  for (const c of list) {
    const t = Date.parse(c.updatedAt);
    groups[t >= today ? 0 : t >= week ? 1 : 2].items.push(c);
  }
  return groups.filter((g) => g.items.length > 0);
}

/**
 * Nom d'un modèle dans le fil : son nom lisible (et sa quantification) s'il est encore dans
 * Ollama, sinon son nom technique.
 */
export function modelLabel(models: readonly HarnessModel[], name: string): string {
  const m = models.find((x) => x.name === name);
  if (!m) return name;
  return m.fromGram && m.quant ? `${m.displayName} ${m.quant}` : m.displayName;
}
