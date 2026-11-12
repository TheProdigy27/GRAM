// Cartes de la bande « Votre machine » (maquette A1 · Atelier) : une carte par composant (GPU,
// RAM, CPU et disques) avec l'origine des valeurs qui ne sont pas mesurées directement et, pour la
// VRAM et la RAM, une barre « Go utilisables par un modèle » tirée du `budget` du classement.
// Aussi : la bande resserrée en pastilles (fiche ouverte) et les noms courts.

import type { DiskInfo } from '../../core/generated/DiskInfo';
import type { HardwareProfile } from '../../core/generated/HardwareProfile';
import type { MemoryBudget } from '../../core/generated/MemoryBudget';
import type { RamInfo } from '../../core/generated/RamInfo';
import type { Source } from '../../core/generated/Source';
import { bandwidth, gb, int, roundSize } from '../../shared/format';
import { RAM_KIND_LABELS, SOURCE_LABELS } from '../../shared/labels';
import { memoryScale } from '../../shared/memory-scale';

/** Barre d'une carte : part de sa mémoire qu'un modèle peut occuper. */
export interface CardUsage {
  /** Fraction de la mémoire de la carte, de 0 à 1. */
  fraction: number;
  /** « 14,3 Go utilisables ». */
  label: string;
  /** D'où vient cette valeur (infobulle). */
  tooltip: string;
}

export interface MachineCard {
  kind: 'gpu' | 'ram' | 'cpu';
  /** « NVIDIA GeForce RTX 5080 », « 64 Go DDR5-6000 », « Ryzen 7 9800X3D ». */
  title: string;
  /** Nom complet quand le titre est raccourci ; vide sinon. */
  fullName: string;
  /** « GPU », « RAM », « CPU ». */
  tag: string;
  /** Lignes de détail ; l'origine d'une valeur estimée ou corrigée entre parenthèses. */
  lines: string[];
  /** Barre des Go utilisables ; `null` sans budget ou sans mémoire propre (CPU, iGPU). */
  usage: CardUsage | null;
  /** La carte aura une barre une fois le budget connu (squelette en attendant). */
  metered: boolean;
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

/** « 8 cœurs / 16 threads · AVX-512 ». */
export function cpuSpecs(p: HardwareProfile): string {
  return `${p.cpu.coresPhysical} cœurs / ${p.cpu.coresLogical} threads · ${cpuIsa(p)}`;
}

/** « 2 canaux », « 1 canal ». */
export function ramChannels(r: RamInfo): string {
  return `${r.channels} ${r.channels > 1 ? 'canaux' : 'canal'}`;
}

/** « 64 Go DDR5-6000, 2 canaux ». */
export function ramDescription(p: HardwareProfile): string {
  return `${ramSizeType(p.ram)}, ${ramChannels(p.ram)}`;
}

/** « (table de référence) » après une valeur qui n'est pas une mesure directe ; vide sinon. */
function sourceNote(s: Source): string {
  return s === 'detected' || s === 'nvml' ? '' : ` (${SOURCE_LABELS[s]})`;
}

/** Pourquoi la machine n'a pas de VRAM utilisable. */
function cpuOnlyNote(p: HardwareProfile): string {
  const g = p.gpu;
  if (!g) return 'aucun GPU : calcul sur le CPU';
  if (g.integrated && !g.unifiedMemory) return 'GPU intégré compté comme du CPU';
  return 'pas de VRAM utilisable : calcul sur le CPU';
}

const clamp01 = (x: number) => (Number.isFinite(x) ? Math.min(Math.max(x, 0), 1) : 0);

/** « 61,6 Go utilisables par un modèle, à deux vitesses » (VRAM puis RAM). */
export function usableSummary(b: MemoryBudget): string {
  const scale = memoryScale(b);
  const total = `${gb(scale.total)} utilisables par un modèle`;
  if (scale.unified) return `${total}, dont ${gb(b.vram)} pour le GPU`;
  return scale.vramEnd > 0 ? `${total}, à deux vitesses` : total;
}

function gpuCard(p: HardwareProfile, b: MemoryBudget | null): MachineCard {
  const g = p.gpu;
  const others = p.otherGpus.map((o) => o.name);
  const unused =
    others.length > 0 ? [`Non utilisé${others.length > 1 ? 's' : ''} : ${others.join(', ')}`] : [];
  const card = { kind: 'gpu', fullName: '', tag: 'GPU', usage: null, metered: false } as const;
  if (!g) return { ...card, title: 'Aucun GPU', lines: ['Calcul sur le CPU', ...unused] };
  if (g.integrated && !g.unifiedMemory) {
    return {
      ...card,
      title: g.name,
      lines: ['Intégré : compté comme du CPU (mémoire partagée)', ...unused],
    };
  }
  const memory = g.unifiedMemory
    ? `Mémoire unifiée : jusqu’à ${roundSize(g.vramTotal)} pour le GPU`
    : `${roundSize(g.vramTotal)} de VRAM`;
  const bw = g.bandwidthGbs ? bandwidth(g.bandwidthGbs) + sourceNote(g.bandwidthSource) : '';
  const k = p.backends;
  const apis = [
    k.cuda && (k.cudaDriverVersion ? `CUDA ${k.cudaDriverVersion}` : 'CUDA'),
    k.vulkan && 'Vulkan',
    k.metal && 'Metal',
  ];
  return {
    ...card,
    title: g.name,
    lines: [
      [memory, bw].filter(Boolean).join(' · '),
      apis.filter(Boolean).join(' · '),
      ...unused,
    ].filter(Boolean),
    usage: b && {
      fraction: clamp01(b.vram / g.vramTotal),
      label: `${gb(b.vram)} utilisables`,
      tooltip: g.unifiedMemory
        ? 'Part de la mémoire unifiée que le GPU peut occuper : la limite de Metal moins la marge de sécurité, au plus la RAM utilisable.'
        : 'VRAM qu’un modèle peut occuper, d’après la marge de sécurité réglée dans les paramètres et la VRAM libre.',
    },
    metered: true,
  };
}

function ramCard(p: HardwareProfile, b: MemoryBudget | null): MachineCard {
  const r = p.ram;
  const unified = !!p.gpu?.unifiedMemory;
  const specs = [ramChannels(r) + sourceNote(r.channelsSource)];
  if (r.bandwidthGbs) specs.push(bandwidth(r.bandwidthGbs));
  return {
    kind: 'ram',
    title: ramSizeType(r),
    fullName: '',
    tag: 'RAM',
    lines: [unified ? 'Mémoire unifiée, partagée avec le GPU' : '', specs.join(' · ')].filter(
      Boolean,
    ),
    usage: b && {
      fraction: clamp01(b.ram / r.total),
      label: `${gb(b.ram)} utilisables`,
      tooltip:
        (unified
          ? 'Mémoire qu’un modèle peut occuper, part du GPU comprise'
          : 'RAM qu’un modèle peut occuper') +
        ', d’après la part de la RAM réglée dans les paramètres et la RAM disponible.',
    },
    metered: true,
  };
}

/** « Disque C: · 210 Go libres » ; « Disques : C: 210 Go libres · D: 1,2 To libres ». */
function disksLine(disks: DiskInfo[]): string {
  const mount = (d: DiskInfo) => d.mount.replace(/\\$/, '');
  const free = (d: DiskInfo) => `${roundSize(d.available)} libres`;
  if (disks.length === 1) return `Disque ${mount(disks[0])} · ${free(disks[0])}`;
  return disks.length > 1
    ? `Disques : ${disks.map((d) => `${mount(d)} ${free(d)}`).join(' · ')}`
    : '';
}

function cpuCard(p: HardwareProfile): MachineCard {
  const title = shortCpuName(p.cpu.name);
  return {
    kind: 'cpu',
    title,
    fullName: title !== p.cpu.name ? p.cpu.name : '',
    tag: 'CPU',
    lines: [cpuSpecs(p), disksLine(p.disks)].filter(Boolean),
    usage: null,
    metered: false,
  };
}

/** Cartes GPU, RAM et CPU ; `budget` absent tant que le classement n'est pas calculé. */
export function machineCards(p: HardwareProfile, b: MemoryBudget | null): MachineCard[] {
  return [gpuCard(p, b), ramCard(p, b), cpuCard(p)];
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
