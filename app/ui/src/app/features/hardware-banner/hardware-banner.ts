import {
  Component,
  DestroyRef,
  PendingTasks,
  booleanAttribute,
  computed,
  inject,
  input,
  linkedSignal,
} from '@angular/core';
import type { GpuInfo } from '../../core/generated/GpuInfo';
import type { HardwareProfile } from '../../core/generated/HardwareProfile';
import type { Source } from '../../core/generated/Source';
import { ShortcutsService } from '../../core/shortcuts.service';
import { errorMessage } from '../../core/tauri.service';
import { bandwidth, roundSize } from '../../shared/format';
import { SOURCE_LABELS } from '../../shared/labels';
import { runtimeRows } from '../../shared/runtimes';
import { Button } from '../../shared/ui/button/button';
import { Callout } from '../../shared/ui/callout/callout';
import { DialogService } from '../../shared/ui/dialog/dialog';
import { Icon } from '../../shared/ui/icon/icon';
import { Skeleton } from '../../shared/ui/skeleton/skeleton';
import { Tag } from '../../shared/ui/tag/tag';
import { ToastService } from '../../shared/ui/toast/toast';
import { Tooltip } from '../../shared/ui/tooltip/tooltip';
import { DetailStore } from '../../state/detail.store';
import { HardwareStore } from '../../state/hardware.store';
import { RankingStore } from '../../state/ranking.store';
import {
  type MemorySelection,
  cpuIsa,
  machinePills,
  memoryCard,
  ramChannels,
  ramSizeType,
  shortCpuName,
  shortGpuName,
} from './memory-card';

/** Une valeur du matériel, avec son origine quand elle n'est pas mesurée directement. */
export interface BannerPart {
  text: string;
  /** « estimée », « table de référence »… (absent : valeur détectée). */
  source?: string;
}

/** Sources affichées à côté de la valeur : celles qui ne sont pas une mesure directe. */
function sourceNote(s: Source): string | undefined {
  return s === 'detected' || s === 'nvml' ? undefined : SOURCE_LABELS[s];
}

export function cpuParts(p: HardwareProfile): BannerPart[] {
  const c = p.cpu;
  return [
    { text: c.name },
    { text: `${c.coresPhysical}c/${c.coresLogical}t` },
    { text: cpuIsa(p) },
  ];
}

export function ramParts(p: HardwareProfile): BannerPart[] {
  const r = p.ram;
  const parts: BannerPart[] = [
    { text: ramSizeType(r) },
    { text: ramChannels(r), source: sourceNote(r.channelsSource) },
  ];
  if (r.bandwidthGbs) parts.push({ text: bandwidth(r.bandwidthGbs) });
  return parts;
}

export function gpuParts(p: HardwareProfile): BannerPart[] {
  const g: GpuInfo | null = p.gpu;
  if (!g) return [{ text: 'Aucun GPU : calcul sur le CPU' }];
  const parts: BannerPart[] = [{ text: g.name }];
  if (g.unifiedMemory) {
    // Apple Silicon : le GPU calcule dans la RAM, jusqu'à la limite de Metal.
    parts.push({ text: `mémoire unifiée : ${roundSize(g.vramTotal)} pour le GPU` });
  } else if (g.integrated) {
    parts.push({ text: 'intégré : compté comme du CPU (mémoire partagée)' });
    return parts;
  } else {
    parts.push({ text: roundSize(g.vramTotal) });
  }
  if (g.bandwidthGbs) {
    parts.push({ text: bandwidth(g.bandwidthGbs), source: sourceNote(g.bandwidthSource) });
  }
  const b = p.backends;
  if (b.cuda) parts.push({ text: b.cudaDriverVersion ? `CUDA ${b.cudaDriverVersion}` : 'CUDA' });
  if (b.vulkan) parts.push({ text: 'Vulkan' });
  if (b.metal) parts.push({ text: 'Metal' });
  return parts;
}

export function diskParts(p: HardwareProfile): BannerPart[] {
  return p.disks.map((d) => ({
    text: `${d.mount.replace(/\\$/, '')} ${roundSize(d.available)} libres`,
  }));
}

const joinParts = (parts: BannerPart[]) =>
  parts.map((x) => (x.source ? `${x.text} (${x.source})` : x.text)).join(' · ');

/** Toutes les valeurs détectées et leur origine (infobulle « Détails de la machine »). */
export function machineDetails(p: HardwareProfile): string {
  const others = p.otherGpus.map((g) => g.name);
  const gpu =
    joinParts(gpuParts(p)) +
    (others.length > 0
      ? ` · non utilisé${others.length > 1 ? 's' : ''} : ${others.join(', ')} (seul le GPU qui a le plus de VRAM calcule)`
      : '');
  const lines = [
    `CPU : ${joinParts(cpuParts(p))}`,
    `RAM : ${joinParts(ramParts(p))}`,
    `GPU : ${gpu}`,
  ];
  if (p.disks.length > 0) lines.push(`Disques : ${joinParts(diskParts(p))}`);
  lines.push(
    '',
    'Mémoire utilisable par un modèle : VRAM moins la marge de sécurité et l’occupation actuelle, ' +
      'puis la part de la RAM réglée dans les paramètres. En mémoire unifiée (Mac Apple Silicon), ' +
      'la part du GPU est prise sur la RAM.',
  );
  return lines.join('\n');
}

/** Libellé de la simulation : « RTX 4090 24 Go », « RAM 32 Go », ou les deux. */
export function simulationLabel(p: HardwareProfile): string {
  const o = p.overrides;
  return [
    o.simulatedGpu && shortGpuName(o.simulatedGpu),
    o.simulatedRamTotal !== null && `RAM ${roundSize(o.simulatedRamTotal)}`,
  ]
    .filter(Boolean)
    .join(', ');
}

/**
 * Bande « Votre machine » : carte mémoire (VRAM puis RAM sur une même échelle, empreinte du
 * modèle sélectionné), CPU, runtimes, avertissements, « Ré-analyser » (touche R) et
 * « Corriger… » (corrections et simulation).
 *
 * `compact` (fiche ouverte) : une ligne de pastilles (VRAM, RAM, runtime choisi), la simulation
 * et les avertissements, et « Détails de la machine » qui rouvre la bande complète.
 */
@Component({
  selector: 'app-hardware-banner',
  imports: [Button, Callout, Icon, Skeleton, Tag, Tooltip],
  templateUrl: './hardware-banner.html',
  styleUrl: './hardware-banner.scss',
})
export class HardwareBanner {
  protected readonly hardware = inject(HardwareStore);
  protected readonly ranking = inject(RankingStore);
  private readonly detail = inject(DetailStore);
  private readonly dialogs = inject(DialogService);
  private readonly toast = inject(ToastService);
  private readonly pendingTasks = inject(PendingTasks);

  /** Fiche ouverte : bande resserrée en une ligne. */
  readonly compact = input(false, { transform: booleanAttribute });
  /** Bande complète rouverte malgré la fiche ; oublié à chaque ouverture ou fermeture. */
  protected readonly expanded = linkedSignal({ source: this.compact, computation: () => false });
  protected readonly isCompact = computed(() => this.compact() && !this.expanded());

  protected readonly profile = this.hardware.profile;
  protected readonly error = this.hardware.error;

  constructor() {
    const off = inject(ShortcutsService).register([{ key: 'r', run: () => this.rescan() }]);
    inject(DestroyRef).onDestroy(off);
  }

  /**
   * Modèle ouvert dans la fiche (quantification choisie), sinon ligne active du classement
   * (clavier), sinon le premier du classement.
   */
  private readonly selection = computed<MemorySelection | null>(() => {
    const d = this.detail.detail();
    const v = this.detail.selected();
    if (d && v) {
      const { memGpu, memCpu } = v.estimate;
      return { name: d.model.displayName, quant: v.variant.label, memGpu, memCpu, opened: true };
    }
    const id = this.detail.repoId() ?? this.ranking.active();
    const found = id ? this.ranking.findItem(id) : undefined;
    const item = found ?? this.ranking.response()?.items[0];
    if (!item) return null;
    const { memGpu, memCpu } = item.best.estimate;
    return {
      name: item.displayName,
      quant: item.best.variant.label,
      memGpu,
      memCpu,
      opened: !!found,
    };
  });

  protected readonly card = computed(() => {
    const p = this.profile();
    const b = this.ranking.response()?.budget;
    return p && b ? memoryCard(b, p, this.selection()) : null;
  });

  protected readonly cpu = computed(() => {
    const p = this.profile();
    if (!p) return null;
    return {
      name: shortCpuName(p.cpu.name),
      full: p.cpu.name,
      rest: `${p.cpu.coresPhysical}c/${p.cpu.coresLogical}t · ${cpuIsa(p)}`,
    };
  });

  protected readonly details = computed(() => {
    const p = this.profile();
    return p ? machineDetails(p) : '';
  });

  protected readonly simulation = computed(() => {
    const p = this.profile();
    if (!p || !this.hardware.simulation()) return null;
    const label = simulationLabel(p);
    return {
      label,
      tooltip: `Classement calculé pour un matériel simulé : ${label}. Les valeurs réelles sont gardées.`,
    };
  });

  protected readonly runtimes = computed(() => {
    const status = this.hardware.runtimes();
    return status ? runtimeRows(status) : null;
  });

  protected readonly warnings = computed(() => this.profile()?.warnings ?? []);

  /** VRAM et RAM utilisables, en pastilles (bande resserrée). */
  protected readonly pills = computed(() => {
    const p = this.profile();
    const b = this.ranking.response()?.budget;
    return p && b ? machinePills(b, p) : null;
  });

  /** Runtime choisi pour le classement et son état (bande resserrée). */
  protected readonly runtime = computed(() => {
    const id = this.ranking.effectiveRuntime();
    return this.runtimes()?.find((r) => r.value === id) ?? null;
  });

  /** Bouton « Ré-analyser » et touche R : nouvelle détection du matériel et des runtimes. */
  protected rescan(): void {
    this.hardware.rescan().catch((e: unknown) => {
      this.toast.show(`Analyse impossible : ${errorMessage(e)}`, { kind: 'error' });
    });
  }

  /** Le dialogue est chargé à la première ouverture. */
  protected openCorrections(): void {
    this.pendingTasks.run(async () => {
      const { CorrectionsDialog } = await import('./corrections-dialog');
      this.dialogs.open(CorrectionsDialog, { width: '720px' });
    });
  }

  protected stopSimulation(): void {
    this.hardware.stopSimulation().catch((e: unknown) => {
      this.toast.show(`Simulation non arrêtée : ${errorMessage(e)}`, { kind: 'error' });
    });
  }
}
