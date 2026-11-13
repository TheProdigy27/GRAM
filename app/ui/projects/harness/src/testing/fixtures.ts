// Fixtures typées de GRAM Harness (types générés depuis app/harness-tauri/src/models.rs).

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
