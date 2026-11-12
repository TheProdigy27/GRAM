// Carte mémoire de la bande « Votre machine » : la machine en une échelle de 0 à « VRAM + RAM
// utilisables », zone VRAM puis zone RAM, et l'empreinte du modèle sélectionné sur la même
// échelle (plan-refonte-ui.md, phase 2).

import type { HardwareProfile } from '../../core/generated/HardwareProfile';
import type { MemoryBudget } from '../../core/generated/MemoryBudget';
import type { RamInfo } from '../../core/generated/RamInfo';
import { gb, int, roundSize } from '../../shared/format';
import { RAM_KIND_LABELS } from '../../shared/labels';
import { type Footprint, footprint, memoryScale, scaleTicks } from '../../shared/memory-scale';

const GIB = 2 ** 30;

/** Modèle dont l'empreinte est placée sous la carte. */
export interface MemorySelection {
  name: string;
  quant: string;
  /** Octets en VRAM (part du GPU en mémoire unifiée). */
  memGpu: number;
  /** Octets en RAM. */
  memCpu: number;
  /** Modèle ouvert dans la fiche (« Sélection ») ; sinon le premier du classement. */
  opened: boolean;
}

export interface CardZone {
  kind: 'vram' | 'ram' | 'unified';
  /** Largeur en fraction de l'échelle. */
  width: number;
  title: string;
  detail: string;
}

export interface CardTick {
  /** Position en fraction de l'échelle. */
  pos: number;
  label: string;
  align: 'start' | 'center' | 'end';
}

export interface MemoryCard {
  /** « 61,6 Go utilisables par un modèle, à deux vitesses ». */
  summary: string;
  /** Pourquoi il n'y a pas de zone VRAM (pas de GPU, GPU intégré…) ; vide sinon. */
  note: string;
  zones: CardZone[];
  /** Repère « GPU jusqu'à X Go » en mémoire unifiée. */
  gpuLimit: { pos: number; label: string } | null;
  ticks: CardTick[];
  footprint: Footprint | null;
  selection: { label: string; model: string; where: string } | null;
  ariaLabel: string;
}

/** « NVIDIA GeForce RTX 5080 » → « RTX 5080 », « AMD Radeon RX 7900 XTX » → « RX 7900 XTX ». */
export function shortGpuName(name: string): string {
  const short = name
    .replace(/\((R|TM)\)/gi, '')
    .replace(/^\s*(NVIDIA|AMD|ATI|Intel)\s+/i, '')
    // « Radeon Graphics » (iGPU) garde sa marque : il ne resterait que « Graphics ».
    .replace(/^(GeForce|Radeon)\s+(?=(RTX|GTX|GT|MX|RX|R\d|HD|Pro|Vega|\d))/i, '')
    .replace(/\s+/g, ' ')
    .trim();
  return short || name;
}

/**
 * « AMD Ryzen 7 9800X3D 8-Core Processor » → « Ryzen 7 9800X3D »,
 * « 12th Gen Intel(R) Core(TM) i7-12700H » → « Core i7-12700H ».
 */
export function shortCpuName(name: string): string {
  const short = name
    .replace(/\((R|TM)\)/gi, '')
    .replace(/\b\d+(th|rd|nd|st) Gen\b/i, '')
    .replace(/\b(AMD|Intel)\s+/i, '')
    .replace(/\s+(\d+|Six|Eight|Twelve|Sixteen)-Core Processor\b.*$/i, '')
    .replace(/\s+(CPU|Processor)\b.*$/i, '')
    .replace(/\s+@.*$/, '')
    .replace(/\s+/g, ' ')
    .trim();
  return short || name;
}

/** « 960 Go/s ». */
function speed(gbs: number): string {
  return `${int(Math.round(gbs))} Go/s`;
}

/** « 64 Go DDR5-6000 », « 16 Go 3200 MT/s » (type inconnu). */
export function ramSizeType(r: RamInfo): string {
  const kind = RAM_KIND_LABELS[r.kind];
  const type =
    kind && r.speedMts ? `${kind}-${r.speedMts}` : kind || (r.speedMts ? `${r.speedMts} MT/s` : '');
  return [roundSize(r.installed ?? r.total), type].filter(Boolean).join(' ');
}

/** Jeu d'instructions utile aux runtimes : AVX-512, AVX2… (ARM64 sur une puce Apple). */
export function cpuIsa(p: HardwareProfile): string {
  const c = p.cpu;
  return c.arch === 'arm' ? 'ARM64' : c.avx512 ? 'AVX-512' : c.avx2 ? 'AVX2' : 'sans AVX2';
}

/** « 2 canaux », « 1 canal ». */
export function ramChannels(r: RamInfo): string {
  return `${r.channels} ${r.channels > 1 ? 'canaux' : 'canal'}`;
}

/** « 64 Go DDR5-6000, 2 canaux ». */
export function ramDescription(p: HardwareProfile): string {
  return `${ramSizeType(p.ram)}, ${ramChannels(p.ram)}`;
}

/** Pourquoi la machine n'a pas de zone VRAM. */
function cpuOnlyNote(p: HardwareProfile): string {
  const g = p.gpu;
  if (!g) return 'aucun GPU : calcul sur le CPU';
  if (g.integrated && !g.unifiedMemory) return 'GPU intégré compté comme du CPU';
  return 'pas de VRAM utilisable : calcul sur le CPU';
}

/** Rapport des bandes passantes VRAM / RAM, arrondi ; `null` s'il n'apprend rien (< 2×). */
function slowdown(b: MemoryBudget): number | null {
  if (!b.gpuBandwidthGbs || b.ramBandwidthGbs <= 0) return null;
  const ratio = Math.round(b.gpuBandwidthGbs / b.ramBandwidthGbs);
  return ratio >= 2 ? ratio : null;
}

export function memoryCard(
  b: MemoryBudget,
  p: HardwareProfile,
  selection: MemorySelection | null,
): MemoryCard {
  const scale = memoryScale(b);
  const total = gb(scale.total);
  const zones: CardZone[] = [];
  let summary: string;
  let note = '';
  let description: string;

  if (scale.unified) {
    summary = `${total} utilisables par un modèle, dont ${gb(b.vram)} pour le GPU`;
    zones.push({
      kind: 'unified',
      width: 1,
      title: `Mémoire unifiée · ${ramDescription(p)}`,
      detail: `${total} · ${speed(b.ramBandwidthGbs)}`,
    });
    description =
      `Échelle de 0 à ${total} de mémoire unifiée à ${speed(b.ramBandwidthGbs)}, ` +
      `dont ${gb(b.vram)} utilisables par le GPU.`;
  } else if (scale.vramEnd > 0) {
    summary = `${total} utilisables par un modèle, à deux vitesses`;
    const gpuName = p.gpu ? shortGpuName(p.gpu.name) : 'GPU';
    const ratio = slowdown(b);
    const gpuSpeed = b.gpuBandwidthGbs ? speed(b.gpuBandwidthGbs) : null;
    zones.push(
      {
        kind: 'vram',
        width: scale.vramEnd,
        title: `VRAM · ${gpuName}`,
        detail: [gb(b.vram), gpuSpeed].filter(Boolean).join(' · '),
      },
      {
        kind: 'ram',
        width: 1 - scale.vramEnd,
        title: `RAM · ${ramDescription(p)}`,
        detail: [
          gb(b.ram),
          speed(b.ramBandwidthGbs),
          ratio ? `${ratio}× plus lente que la VRAM` : '',
        ]
          .filter(Boolean)
          .join(' · '),
      },
    );
    description =
      `Échelle de 0 à ${total} : ${gb(b.vram)} de VRAM${gpuSpeed ? ` à ${gpuSpeed}` : ''}, ` +
      `puis ${gb(b.ram)} de RAM à ${speed(b.ramBandwidthGbs)}` +
      `${ratio ? `, ${ratio} fois plus lente` : ''}.`;
  } else {
    note = cpuOnlyNote(p);
    summary = `${total} utilisables par un modèle`;
    zones.push({
      kind: 'ram',
      width: 1,
      title: `RAM · ${ramDescription(p)}`,
      detail: `${total} · ${speed(b.ramBandwidthGbs)}`,
    });
    description = `Échelle de 0 à ${total} de RAM à ${speed(b.ramBandwidthGbs)}, sans VRAM : ${note}.`;
  }

  const values = scaleTicks(scale.total / GIB);
  const ticks: CardTick[] = values.map((t, i) => {
    const pos = scale.total > 0 ? (t * GIB) / scale.total : 0;
    const last = i === values.length - 1;
    return {
      pos,
      label: last ? `${t} Go` : String(t),
      align: i === 0 ? 'start' : pos > 0.92 ? 'end' : 'center',
    };
  });

  let fp: Footprint | null = null;
  let sel: MemoryCard['selection'] = null;
  if (selection) {
    fp = footprint(scale, selection.memGpu, selection.memCpu);
    const [gpuPart, cpuPart] = scale.unified
      ? ['pour le GPU', 'pour le CPU']
      : ['en VRAM', 'en RAM'];
    const parts = [
      selection.memGpu > 0 ? `${gb(selection.memGpu)} ${gpuPart}` : '',
      selection.memCpu > 0 ? `${gb(selection.memCpu)} ${cpuPart}` : '',
    ].filter(Boolean);
    const where =
      (parts.join(', ') || 'aucune mémoire') +
      (fp.overflow ? ' : plus que la mémoire utilisable' : '');
    const label = selection.opened ? 'Sélection' : 'En tête du classement';
    sel = { label, model: `${selection.name} · ${selection.quant}`, where };
    description += ` ${label} : ${selection.name} en ${selection.quant}, ${where}.`;
  }

  return {
    summary,
    note,
    zones,
    gpuLimit:
      scale.gpuLimit !== null ? { pos: scale.gpuLimit, label: `GPU jusqu’à ${gb(b.vram)}` } : null,
    ticks,
    footprint: fp,
    selection: sel,
    ariaLabel: description,
  };
}

/** Une pastille de la bande machine resserrée (fiche ouverte). */
export interface MachinePill {
  kind: 'vram' | 'ram' | 'unified';
  /** « RTX 5080 · ». */
  text: string;
  /** « 14,3 Go », en gras. */
  value: string;
  /** « VRAM utilisables ». */
  after: string;
  /** Détail (bande passante, raison de l'absence de VRAM). */
  tooltip: string;
}

/**
 * Bande machine en une ligne, quand la fiche est ouverte : VRAM et RAM utilisables (une seule
 * pastille en mémoire unifiée, ou sans VRAM utilisable).
 */
export function machinePills(b: MemoryBudget, p: HardwareProfile): MachinePill[] {
  const scale = memoryScale(b);
  const ram = ramSizeType(p.ram);
  if (scale.unified) {
    return [
      {
        kind: 'unified',
        text: `Mémoire unifiée ${ram} · `,
        value: gb(b.ram),
        after: ` utilisables, dont ${gb(b.vram)} pour le GPU`,
        tooltip: `Mémoire unifiée à ${speed(b.ramBandwidthGbs)} : le GPU calcule dans la RAM.`,
      },
    ];
  }
  const ramPill: MachinePill = {
    kind: 'ram',
    text: `${ram} · `,
    value: gb(b.ram),
    after: ' de RAM utilisables',
    tooltip: `RAM ${ramDescription(p)} à ${speed(b.ramBandwidthGbs)}`,
  };
  if (scale.vramEnd <= 0) {
    return [{ ...ramPill, tooltip: `${ramPill.tooltip} · ${cpuOnlyNote(p)}` }];
  }
  const gpuName = p.gpu ? shortGpuName(p.gpu.name) : 'GPU';
  return [
    {
      kind: 'vram',
      text: `${gpuName} · `,
      value: gb(b.vram),
      after: ' de VRAM utilisables',
      tooltip: b.gpuBandwidthGbs
        ? `VRAM à ${speed(b.gpuBandwidthGbs)}, la mémoire la plus rapide`
        : 'VRAM utilisable par un modèle',
    },
    ramPill,
  ];
}
