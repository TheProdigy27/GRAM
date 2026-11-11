// Données de test typées avec les types générés par ts-rs : si un type Rust change, ces
// fixtures ne compilent plus, ce qui oblige à mettre les tests à jour.
// Valeurs inspirées de la machine de développement (Ryzen 7 9800X3D, RTX 5080, 64 Go DDR5).

import type { CacheInfo } from '../app/core/generated/CacheInfo';
import type { Calibration } from '../app/core/generated/Calibration';
import type { Estimate } from '../app/core/generated/Estimate';
import type { HardwareOptions } from '../app/core/generated/HardwareOptions';
import type { HardwareProfile } from '../app/core/generated/HardwareProfile';
import type { LaunchPlan } from '../app/core/generated/LaunchPlan';
import type { Measurement } from '../app/core/generated/Measurement';
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
  ollamaModels: [
    {
      name: 'hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M',
      repoId: 'Qwen/Qwen3-8B-GGUF',
      quant: 'Q4_K_M',
      size: 4.7 * GIB,
    },
  ],
};

export const SETTINGS: Settings = {
  runtime: 'ollama',
  estimate: {
    vramMargin: 0.1,
    ramShare: 0.75,
    closeApps: false,
    overheadBytes: GIB / 2,
    gpuEfficiency: 0.81,
    ramEfficiency: 0.62,
    tokenOverheadMs: 0.8,
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
  catalogTtlHours: 24,
  detailGroups: 150,
  pageSize: 25,
};

/** Dialogue « Corriger… » : matériel détecté (avec un iGPU en plus) et GPU simulables. */
export const HARDWARE_OPTIONS: HardwareOptions = {
  detected: {
    ...HARDWARE,
    otherGpus: [
      {
        index: 1,
        name: 'AMD Radeon(TM) Graphics',
        vendor: 'amd',
        vramTotal: 0.5 * GIB,
        vramFree: null,
        sharedMem: 31.5 * GIB,
        integrated: true,
        bandwidthGbs: 96,
        bandwidthSource: 'estimated',
        cudaCc: null,
      },
    ],
  },
  gpus: [
    { label: 'NVIDIA GeForce RTX 3060 12 Go', vram: 12 * GIB, bandwidthGbs: 360 },
    { label: 'NVIDIA GeForce RTX 4090 24 Go', vram: 24 * GIB, bandwidthGbs: 1008 },
    { label: 'AMD Radeon RX 7800 XT 16 Go', vram: 16 * GIB, bandwidthGbs: 624 },
    { label: 'Intel Arc A770 16 Go', vram: 16 * GIB, bandwidthGbs: 560 },
  ],
};

export const CACHE_INFO: CacheInfo = {
  path: 'C:\\Users\\x\\AppData\\Local\\fr.gram.desktop\\cache',
  files: 312,
  bytes: 0.25 * GIB,
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
  /** Quantifications installées dans Ollama. */
  installed?: string[];
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
    installed: o.installed ?? [],
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
    installed: ['Q4_K_M'],
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
    ollamaModel: null,
    ollamaPull: `ollama pull hf.co/${repoId}:${label}`,
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
  const q4 = variant(repo, 'Q4_K_M', 4.7, makeEstimate({ weights: 4.7 * GIB }), 82);
  q4.ollamaModel = 'hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M';
  const variants = [
    q4,
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

/** Mesure de Q4_K_M (fiche `makeDetail`) : ~95 tok/s estimés, 83 mesurés. */
export const MEASUREMENT: Measurement = {
  repoId: 'Qwen/Qwen3-8B-GGUF',
  displayName: 'Qwen3-8B',
  label: 'Q4_K_M',
  ollamaModel: 'hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M',
  ctx: 8192,
  measuredAt: '2026-11-10T20:00:00Z',
  tokS: 83.0,
  evalCount: 128,
  promptTokS: 1476,
  loadMs: 2731,
  memTotal: 6.45 * GIB,
  memVram: 6.45 * GIB,
  ctxLoaded: 8192,
  estimatedTokS: 95,
  estimatedMode: { kind: 'gpu' },
  estimatedMemTotal: 6.3 * GIB,
  estimatedMemVram: 6.3 * GIB,
  tokSError: 0.1446,
  memError: -0.023,
  activeBytes: 4.7 * GIB,
  modelOverheadMs: 0,
  gpuShare: 1,
  reliable: true,
  notes: [],
};

export const CALIBRATION_EMPTY: Calibration = {
  measurements: [],
  used: 0,
  currentGpuEfficiency: 0.81,
  currentRamEfficiency: 0.62,
  currentTokenOverheadMs: 0.8,
  gpuEfficiency: null,
  ramEfficiency: null,
  tokenOverheadMs: null,
  changed: false,
  summary:
    'Aucune mesure exploitable : mesurer une quantification déjà installée dans Ollama, depuis la fiche du modèle.',
};

export const CALIBRATION: Calibration = {
  ...CALIBRATION_EMPTY,
  measurements: [MEASUREMENT],
  used: 1,
  gpuEfficiency: 0.48,
  changed: true,
  summary: 'D’après 1 mesure : efficacité GPU 0,48 (actuelle 0,81).',
};

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
