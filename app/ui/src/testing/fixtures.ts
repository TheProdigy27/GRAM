// Données de test typées avec les types générés par ts-rs : si un type Rust change, ces
// fixtures ne compilent plus, ce qui oblige à mettre les tests à jour.
// Valeurs inspirées de la machine de développement (Ryzen 7 9800X3D, RTX 5080, 64 Go DDR5).

import type { Estimate } from '../app/core/generated/Estimate';
import type { HardwareProfile } from '../app/core/generated/HardwareProfile';
import type { LaunchPlan } from '../app/core/generated/LaunchPlan';
import type { MemoryBudget } from '../app/core/generated/MemoryBudget';
import type { ModelDetail } from '../app/core/generated/ModelDetail';
import type { ModelEntry } from '../app/core/generated/ModelEntry';
import type { RankItem } from '../app/core/generated/RankItem';
import type { RankResponse } from '../app/core/generated/RankResponse';
import type { RunMode } from '../app/core/generated/RunMode';
import type { RuntimesStatus } from '../app/core/generated/RuntimesStatus';
import type { Score } from '../app/core/generated/Score';
import type { Settings } from '../app/core/generated/Settings';
import type { SyncSummary } from '../app/core/generated/SyncSummary';
import type { VariantDetail } from '../app/core/generated/VariantDetail';

export const GIB = 2 ** 30;

export const HARDWARE: HardwareProfile = {
  cpu: {
    name: 'AMD Ryzen 7 9800X3D 8-Core Processor',
    coresPhysical: 8,
    coresLogical: 16,
    maxClockMhz: 4700,
    avx2: true,
    avx512: true,
    fma: true,
    f16c: true,
  },
  ram: {
    total: 63.1 * GIB,
    available: 48 * GIB,
    installed: 64 * GIB,
    kind: 'ddr5',
    speedMts: 6000,
    modules: 2,
    slots: 4,
    channels: 2,
    channelsSource: 'estimated',
    bandwidthGbs: 96,
  },
  gpu: {
    index: 0,
    name: 'NVIDIA GeForce RTX 5080',
    vendor: 'nvidia',
    vramTotal: 15.9 * GIB,
    vramFree: 14.8 * GIB,
    sharedMem: 31.5 * GIB,
    integrated: false,
    bandwidthGbs: 960,
    bandwidthSource: 'table',
    cudaCc: [12, 0],
  },
  otherGpus: [],
  backends: { cuda: true, cudaDriverVersion: '12.9', vulkan: true },
  disks: [{ mount: 'C:\\', total: 1862 * GIB, available: 210 * GIB }],
  overrides: {
    ramChannels: null,
    gpuBandwidthGbs: null,
    gpuIntegrated: null,
    gpuIndex: null,
    simulatedGpu: null,
    simulatedRamTotal: null,
  },
  warnings: [],
};

export const RUNTIMES: RuntimesStatus = {
  ollama: { installed: true, running: true, path: 'C:\\Ollama\\ollama.exe', version: '0.13.1' },
  lmStudio: { installed: false, running: false, path: null, version: null },
  llamaCpp: { installed: false, running: false, path: null, version: null },
};

export const SETTINGS: Settings = {
  runtime: 'ollama',
  estimate: {
    vramMargin: 0.1,
    ramShare: 0.75,
    closeApps: false,
    overheadBytes: GIB / 2,
    efficiency: 0.55,
    tokenOverheadMs: 1.5,
    kvCache: 'f16',
    minBpw: 2.5,
    minCtx: 2048,
  },
  useCases: {
    general: {
      ctx: 8192,
      slowTokS: 5,
      weights: { quality: 0.45, speed: 0.3, fit: 0.15, context: 0.1 },
    },
    code: {
      ctx: 16384,
      slowTokS: 5,
      weights: { quality: 0.5, speed: 0.2, fit: 0.1, context: 0.2 },
    },
    reasoning: {
      ctx: 16384,
      slowTokS: 8,
      weights: { quality: 0.55, speed: 0.2, fit: 0.1, context: 0.15 },
    },
    fastChat: {
      ctx: 4096,
      slowTokS: 10,
      weights: { quality: 0.3, speed: 0.5, fit: 0.15, context: 0.05 },
    },
    vision: {
      ctx: 8192,
      slowTokS: 5,
      weights: { quality: 0.45, speed: 0.25, fit: 0.15, context: 0.15 },
    },
  },
  minDownloads: 500,
  pageSize: 25,
};

export const BUDGET: MemoryBudget = {
  vram: 14.3 * GIB,
  ram: 47.3 * GIB,
  gpuBandwidthGbs: 960,
  ramBandwidthGbs: 96,
};

export function makeScore(total: number, overrides: Partial<Score> = {}): Score {
  return {
    quality: 82,
    speed: 71,
    fit: 100,
    context: 100,
    weights: { quality: 0.45, speed: 0.3, fit: 0.15, context: 0.1 },
    weighted: 82.2,
    popularity: 3.2,
    useCaseFactor: 1,
    confidence: 1,
    total,
    ...overrides,
  };
}

export interface EstimateOptions {
  mode?: RunMode;
  tokS?: number;
  ctxMax?: number;
  ctxUsed?: number;
  weights?: number;
  kv?: number;
  approximate?: boolean;
}

export function makeEstimate(o: EstimateOptions = {}): Estimate {
  const weights = o.weights ?? 4.7 * GIB;
  const kv = o.kv ?? 1.1 * GIB;
  const overhead = GIB / 2;
  const total = weights + kv + overhead;
  const mode = o.mode ?? { kind: 'gpu' };
  const memGpu = mode.kind === 'gpu' ? total : mode.kind === 'hybrid' ? BUDGET.vram * 0.97 : 0;
  return {
    mode,
    memGpu,
    memCpu: mode.kind === 'gpu' ? 0 : total - memGpu,
    breakdown: { weights, kv, overhead, vision: 0 },
    kvPerToken: 144 * 1024,
    memUsage: total / BUDGET.vram,
    ctxUsed: o.ctxUsed ?? 8192,
    ctxMax: o.ctxMax ?? 40960,
    tokS: o.tokS ?? 95,
    fit: mode.kind === 'gpu' ? 'perfect' : mode.kind === 'tooBig' ? 'tooTight' : 'marginal',
    approximate: o.approximate ?? false,
    warnings: [],
  };
}

export interface ItemOptions extends EstimateOptions {
  repoId?: string;
  score?: number;
  quant?: string;
  size?: number;
  detailed?: boolean;
  vision?: boolean;
  gated?: boolean;
}

export function makeItem(name: string, o: ItemOptions = {}): RankItem {
  const repoId = o.repoId ?? `org/${name}-GGUF`;
  const size = o.size ?? 4.7 * GIB;
  return {
    repoId,
    displayName: name,
    baseModel: `org/${name}`,
    paramsTotal: 8.2e9,
    paramsActive: null,
    pipeline: o.vision ? 'image-text-to-text' : 'text-generation',
    useCases: o.vision ? ['general', 'vision'] : ['general'],
    downloads: 1_200_000,
    likes: 300,
    gated: o.gated ?? false,
    createdAt: '2026-04-29T00:00:00Z',
    detailed: o.detailed ?? true,
    variantCount: 12,
    best: {
      variant: {
        label: o.quant ?? 'Q4_K_M',
        repoId,
        files: [`${name}-Q4_K_M.gguf`],
        sizeBytes: size,
        bpw: 4.9,
        exact: o.detailed ?? true,
      },
      estimate: makeEstimate({ weights: size, ...o }),
    },
    score: makeScore(o.score ?? 80),
    slow: false,
    explanation: `Tient entièrement en VRAM (6,3 / 14,3 Go) en ${o.quant ?? 'Q4_K_M'} · ~95 tok/s estimés · contexte 40k possible`,
  };
}

export function makeResponse(items: RankItem[], o: Partial<RankResponse> = {}): RankResponse {
  return {
    useCase: 'general',
    ctx: 8192,
    runtime: 'ollama',
    budget: BUDGET,
    slowTokS: 5,
    items,
    totalItems: items.length,
    slow: [],
    totalSlow: 0,
    tooBig: [],
    tooBigCount: 0,
    dataSource: { kind: 'cache', fetchedAt: '2026-11-09T18:00:00Z' },
    notes: [
      'Sans réglage, Ollama choisit le contexte selon la VRAM (4 096 tokens sur cette machine) : régler num_ctx sur le contexte indiqué, sinon la mémoire utilisée diffère de l’estimation.',
    ],
    ...o,
  };
}

export const RANKING: RankResponse = makeResponse([
  makeItem('Qwen3-8B', {
    repoId: 'Qwen/Qwen3-8B-GGUF',
    score: 86,
    tokS: 95,
    quant: 'Q8_0',
    size: 8.1 * GIB,
  }),
  makeItem('Gemma-3-12B', { score: 84, tokS: 62, vision: true, gated: true }),
  makeItem('Mistral-Small-24B', {
    score: 71,
    tokS: 7.4,
    mode: { kind: 'hybrid', gpuLayers: 30, layers: 40 },
    size: 14 * GIB,
  }),
]);

function ollamaPlan(repoId: string, quant: string, ctx: number): LaunchPlan {
  return {
    runtime: 'ollama',
    commands: [
      { label: 'Lancer le modèle', command: `ollama run hf.co/${repoId}:${quant}` },
      { label: 'Puis, dans la session', command: `/set parameter num_ctx ${ctx}` },
      {
        label: 'Ou, pour tous les modèles (puis relancer Ollama)',
        command: `setx OLLAMA_CONTEXT_LENGTH ${ctx}`,
      },
    ],
    openUrl: null,
    notes: [],
  };
}

function variant(
  repoId: string,
  label: string,
  sizeGb: number,
  estimate: Estimate,
  total: number,
): VariantDetail {
  return {
    variant: {
      label,
      repoId,
      files: [`Qwen3-8B-${label}.gguf`],
      sizeBytes: sizeGb * GIB,
      bpw: (sizeGb * GIB * 8) / 8.2e9,
      exact: true,
    },
    estimate,
    score: makeScore(total),
    slow: false,
    withinSize: true,
    explanation: `Tient entièrement en VRAM en ${label}`,
    launch: ollamaPlan(repoId, label, estimate.ctxUsed),
  };
}

export const MODEL: ModelEntry = {
  repoId: 'Qwen/Qwen3-8B-GGUF',
  sha: 'abc123',
  baseModel: 'Qwen/Qwen3-8B',
  displayName: 'Qwen3-8B',
  paramsTotal: 8.2e9,
  paramsActive: null,
  arch: null,
  ctxTrain: 40960,
  pipeline: 'text-generation',
  useCases: ['general', 'code', 'reasoning'],
  downloads: 1_200_000,
  likes: 300,
  createdAt: '2026-04-29T00:00:00Z',
  lastModified: null,
  gated: false,
  variants: [],
  mmprojBytes: null,
  alternatives: [
    { repoId: 'bartowski/Qwen_Qwen3-8B-GGUF', sha: 'def', downloads: 85_000, gated: false },
  ],
};

export function makeDetail(): ModelDetail {
  const repo = MODEL.repoId;
  const variants = [
    variant(repo, 'Q4_K_M', 4.7, makeEstimate({ weights: 4.7 * GIB }), 82),
    variant(repo, 'Q8_0', 8.1, makeEstimate({ weights: 8.1 * GIB, tokS: 62 }), 86),
    variant(
      repo,
      'BF16',
      15.3,
      makeEstimate({ weights: 15.3 * GIB, mode: { kind: 'tooBig' }, tokS: 0, ctxMax: 0 }),
      40,
    ),
  ];
  return {
    model: { ...MODEL, variants: variants.map((v) => v.variant) },
    useCase: 'general',
    ctx: 8192,
    runtime: 'ollama',
    budget: BUDGET,
    slowTokS: 5,
    variants,
    best: 1,
    hfUrl: `https://huggingface.co/${repo}`,
    notes: [],
  };
}

export const SYNC_SUMMARY: SyncSummary = {
  source: { kind: 'cache', fetchedAt: '2026-11-09T18:00:00Z' },
  repos: 412,
  models: 1165,
  detailed: 150,
  withArch: 140,
  apiCalls: 0,
  resolverCalls: 0,
  rateLimit: null,
  durationMs: 12,
  warnings: [],
};
