// Fixtures typées de GRAM Harness (types générés depuis app/harness-tauri/src/models.rs).

import type { ChatMessage } from '@gram/types/ChatMessage';
import type { Conversation } from '@gram/types/Conversation';
import type { HarnessModel } from '@gram/types/HarnessModel';
import type { ModelList } from '@gram/types/ModelList';

/** Ajouté depuis GRAM, mesuré : réflexion et outils. */
export const GRAM_MODEL: HarnessModel = {
  name: 'gram/qwen3-8b:q4_k_m',
  displayName: 'Qwen3 8B',
  fromGram: true,
  quant: 'Q4_K_M',
  size: 5_027_797_508,
  ctx: 16384,
  vision: false,
  thinking: true,
  tools: true,
  estimatedTokS: 45.2,
  measuredTokS: 43.1,
  useCase: 'general',
};

/** Ajouté depuis GRAM, pas encore mesuré : vision. */
export const VISION_MODEL: HarnessModel = {
  name: 'gram/gemma-3-4b-it:q4_k_m',
  displayName: 'Gemma 3 4B',
  fromGram: true,
  quant: 'Q4_K_M',
  size: 3_338_801_664,
  ctx: 32768,
  vision: true,
  thinking: false,
  tools: false,
  estimatedTokS: 88,
  measuredTokS: null,
  useCase: 'vision',
};

/** Bibliothèque d'Ollama, contexte non réglé. */
export const OTHER_MODEL: HarnessModel = {
  name: 'qwen3:8b',
  displayName: 'qwen3:8b',
  fromGram: false,
  quant: 'Q4_K_M',
  size: 5_225_387_923,
  ctx: null,
  vision: false,
  thinking: true,
  tools: true,
  estimatedTokS: null,
  measuredTokS: null,
  useCase: null,
};

export const MODELS: ModelList = {
  ollama: 'running',
  models: [GRAM_MODEL, VISION_MODEL, OTHER_MODEL],
};
export const NO_MODELS: ModelList = { ollama: 'running', models: [] };
export const OLLAMA_STOPPED: ModelList = { ollama: 'stopped', models: [] };
export const OLLAMA_ABSENT: ModelList = { ollama: 'absent', models: [] };

// ── Conversations (dates relatives au lancement des tests) ───────────────────────────────

const DAY = 86_400_000;
const ago = (ms: number) => new Date(Date.now() - ms).toISOString();

function message(role: ChatMessage['role'], content: string, at: string): ChatMessage {
  return {
    role,
    content,
    thinking: null,
    model: role === 'assistant' ? GRAM_MODEL.name : null,
    stats: null,
    interrupted: false,
    error: null,
    createdAt: at,
  };
}

/** Réponse riche : titres, liste, tableau, code, lien. */
export const RICH_ANSWER = `## En bref

**Q4_K_M** garde l’essentiel de la qualité pour environ *moitié moins* de mémoire que Q8_0 :

- Q4_K_M : 4,9 Go, très proche de l’original ;
- Q8_0 : 8,7 Go, quasi identique.

| Quantification | Taille | Qualité |
|---|---|---|
| Q4_K_M | 4,9 Go | 97 % |
| Q8_0 | 8,7 Go | 99,5 % |

\`\`\`python
def taille(params: float, bits: float) -> float:
    """Taille approximative en Go."""
    return params * bits / 8
\`\`\`

Voir la [documentation de llama.cpp](https://github.com/ggml-org/llama.cpp) et \`ollama show\`.`;

/** Aujourd'hui : une question et sa réponse riche. */
export const CONVERSATION_TODAY: Conversation = {
  id: '20261113-090000000',
  title: 'Différence entre Q4_K_M et Q8_0',
  model: GRAM_MODEL.name,
  instructions: null,
  createdAt: ago(60_000 * 5),
  updatedAt: ago(60_000 * 4),
  messages: [
    message('user', 'Quelle différence entre Q4_K_M et Q8_0 ?', ago(60_000 * 5)),
    {
      ...message('assistant', RICH_ANSWER, ago(60_000 * 4)),
      thinking: 'L’utilisateur compare deux quantifications.',
      stats: {
        doneReason: 'stop',
        promptEvalCount: 18,
        evalCount: 212,
        evalDuration: 4_700_000_000,
        loadDuration: 2_100_000_000,
        promptEvalDuration: 60_000_000,
        totalDuration: 6_900_000_000,
      },
    },
  ],
};

/** Il y a trois jours : réponse interrompue, d'un autre modèle. */
export const CONVERSATION_WEEK: Conversation = {
  id: '20261110-150000000',
  title: 'Écrire un conte',
  model: OTHER_MODEL.name,
  instructions: null,
  createdAt: ago(3 * DAY),
  updatedAt: ago(3 * DAY),
  messages: [
    message('user', 'Écris un long conte.', ago(3 * DAY)),
    {
      ...message('assistant', 'Il était une fois, dans un village', ago(3 * DAY)),
      model: OTHER_MODEL.name,
      interrupted: true,
    },
  ],
};

/** Il y a un mois : réponse en échec (Ollama arrêté). */
export const CONVERSATION_OLD: Conversation = {
  id: '20261013-100000000',
  title: 'Bonjour',
  model: GRAM_MODEL.name,
  instructions: null,
  createdAt: ago(31 * DAY),
  updatedAt: ago(31 * DAY),
  messages: [
    message('user', 'Bonjour', ago(31 * DAY)),
    {
      ...message('assistant', '', ago(31 * DAY)),
      error: 'Ollama ne répond pas sur http://127.0.0.1:11434 : le lancer, puis réessayer',
    },
  ],
};

export const CONVERSATIONS: readonly Conversation[] = [
  CONVERSATION_TODAY,
  CONVERSATION_WEEK,
  CONVERSATION_OLD,
];
