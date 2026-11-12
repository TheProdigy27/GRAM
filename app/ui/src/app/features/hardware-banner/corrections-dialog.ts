import { DialogRef } from '@angular/cdk/dialog';
import {
  Component,
  ElementRef,
  Injector,
  afterNextRender,
  computed,
  inject,
  signal,
} from '@angular/core';
import type { GpuInfo } from '../../core/generated/GpuInfo';
import type { HardwareOptions } from '../../core/generated/HardwareOptions';
import type { HardwareProfile } from '../../core/generated/HardwareProfile';
import type { Overrides } from '../../core/generated/Overrides';
import type { SimulationGpu } from '../../core/generated/SimulationGpu';
import type { Source } from '../../core/generated/Source';
import { TauriService, errorMessage } from '../../core/tauri.service';
import {
  bandwidth,
  decimal,
  gbToBytes,
  parseNumber,
  roundSize,
  sameNumber,
} from '../../shared/format';
import { SOURCE_LABELS } from '../../shared/labels';
import { Button } from '../../shared/ui/button/button';
import { Callout } from '../../shared/ui/callout/callout';
import { GDialog } from '../../shared/ui/dialog/dialog';
import { FIELD } from '../../shared/ui/field/field';
import { Icon } from '../../shared/ui/icon/icon';
import { Progress } from '../../shared/ui/progress/progress';
import { Select, type SelectOption } from '../../shared/ui/select/select';
import { Tag, type TagTone } from '../../shared/ui/tag/tag';
import { ToggleGroup, type ToggleOption } from '../../shared/ui/toggle-group/toggle-group';
import { Tooltip } from '../../shared/ui/tooltip/tooltip';
import { HardwareStore } from '../../state/hardware.store';
import { cpuSpecs, ramChannels, ramSizeType, shortCpuName, shortGpuName } from './machine-cards';

/** Aucune correction, aucune simulation : valeurs détectées. */
export const NO_OVERRIDES: Overrides = {
  ramChannels: null,
  gpuBandwidthGbs: null,
  gpuIntegrated: null,
  gpuIndex: null,
  simulatedGpu: null,
  simulatedRamTotal: null,
};

/** Canaux mémoire proposés (64 bits chacun ; jusqu'à 16 pour une puce Apple Ultra). */
export const RAM_CHANNELS = [1, 2, 3, 4, 6, 8, 12, 16] as const;

/** Quantités de RAM proposées pour la simulation, en Go. */
export const SIMULATED_RAM_GB = [8, 16, 24, 32, 48, 64, 96, 128, 192, 256] as const;

/** GPU simulables regroupés par constructeur (d'après le début du libellé). */
export function groupGpus(gpus: SimulationGpu[]): { vendor: string; gpus: SimulationGpu[] }[] {
  const groups: { vendor: string; gpus: SimulationGpu[] }[] = [];
  for (const g of gpus) {
    const vendor = /^(NVIDIA|AMD|Intel)\b/.exec(g.label)?.[1] ?? 'Autres';
    let group = groups.find((x) => x.vendor === vendor);
    if (!group) groups.push((group = { vendor, gpus: [] }));
    group.gpus.push(g);
  }
  return groups;
}

/** Nombre décimal saisi (virgule acceptée) ; vide, invalide ou négatif : `null`. */
export function parsePositive(value: string): number | null {
  const x = parseNumber(value);
  return x !== null && x > 0 ? x : null;
}

export type DetectedKey = 'gpu' | 'gpuType' | 'bandwidth' | 'ram' | 'channels' | 'cpu';

/** Une valeur du matériel, sur une ligne du dialogue. */
export interface DetectedRow {
  key: DetectedKey;
  label: string;
  /** Valeur (en gras), puis ses précisions. */
  value: string;
  detail: string;
  /** Origine, en étiquette : détectée, table de référence, estimée, corrigée. */
  source: Source;
  /** Valeur détectée, rappelée quand une correction la remplace. */
  detected: string | null;
  /** Estimée et non corrigée : surlignée, « à confirmer ». */
  toConfirm: boolean;
  /** Bouton crayon : la valeur se corrige. */
  editable: boolean;
}

/** Les GPU détectés : celui retenu par défaut, puis les autres. */
export function detectedGpus(d: HardwareProfile): GpuInfo[] {
  return [...(d.gpu ? [d.gpu] : []), ...d.otherGpus];
}

/** GPU qui serait retenu avec ces corrections. */
export function chosenGpu(d: HardwareProfile, o: Overrides): GpuInfo | null {
  const all = detectedGpus(d);
  const index = o.gpuIndex;
  return (index !== null ? all.find((g) => g.index === index) : undefined) ?? all[0] ?? null;
}

const channels = (n: number) => `${n} ${n > 1 ? 'canaux' : 'canal'}`;

/**
 * Valeurs détectées (`d`, sans corrections) et leur origine, avec les corrections `o` en cours :
 * une valeur corrigée prend l'origine « corrigée » et rappelle la valeur détectée.
 */
export function detectedRows(d: HardwareProfile, o: Overrides): DetectedRow[] {
  const rows: DetectedRow[] = [];
  const gpus = detectedGpus(d);
  const gpu = chosenGpu(d, o);
  if (gpu) {
    rows.push({
      key: 'gpu',
      label: 'Carte graphique',
      value: shortGpuName(gpu.name),
      detail: gpu.unifiedMemory
        ? `mémoire unifiée, ${roundSize(gpu.vramTotal)} pour le GPU`
        : `${roundSize(gpu.vramTotal)} de VRAM`,
      source: o.gpuIndex !== null ? 'manual' : 'detected',
      detected: o.gpuIndex !== null && d.gpu ? shortGpuName(d.gpu.name) : null,
      toConfirm: false,
      editable: gpus.length > 1,
    });
    if (!gpu.unifiedMemory) {
      const integrated = o.gpuIntegrated ?? gpu.integrated;
      rows.push({
        key: 'gpuType',
        label: 'Type de GPU',
        value: integrated ? 'Intégré' : 'Dédié',
        detail: integrated
          ? 'compté comme du CPU : il partage la RAM'
          : 'les couches du modèle vont dans sa VRAM',
        source: o.gpuIntegrated !== null ? 'manual' : 'detected',
        detected: o.gpuIntegrated !== null ? (gpu.integrated ? 'intégré' : 'dédié') : null,
        toConfirm: false,
        editable: true,
      });
    }
    const bw = o.gpuBandwidthGbs ?? gpu.bandwidthGbs;
    const detectedBw = gpu.bandwidthGbs ? bandwidth(gpu.bandwidthGbs) : 'inconnue';
    rows.push({
      key: 'bandwidth',
      label: 'Bande passante GPU',
      value: bw ? bandwidth(bw) : 'Inconnue',
      detail: bw ? '' : 'fiche constructeur du GPU',
      source: o.gpuBandwidthGbs !== null ? 'manual' : gpu.bandwidthSource,
      detected: o.gpuBandwidthGbs !== null ? detectedBw : null,
      toConfirm: o.gpuBandwidthGbs === null && gpu.bandwidthSource === 'estimated',
      editable: true,
    });
  } else {
    rows.push({
      key: 'gpu',
      label: 'Carte graphique',
      value: 'Aucun GPU',
      detail: 'calcul sur le CPU',
      source: 'detected',
      detected: null,
      toConfirm: false,
      editable: false,
    });
  }

  const r = d.ram;
  const modules =
    r.modules > 0
      ? `${r.modules} barrette${r.modules > 1 ? 's' : ''}` +
        (r.slots ? ` sur ${r.slots} emplacements` : '')
      : '';
  rows.push({
    key: 'ram',
    label: 'Mémoire vive',
    value: ramSizeType(r),
    detail: modules,
    source: 'detected',
    detected: null,
    toConfirm: false,
    editable: false,
  });
  const ch = o.ramChannels ?? r.channels;
  rows.push({
    key: 'channels',
    label: 'Canaux mémoire',
    value: channels(ch),
    detail: o.ramChannels === null && r.bandwidthGbs ? bandwidth(r.bandwidthGbs) : '',
    source: o.ramChannels !== null ? 'manual' : r.channelsSource,
    detected: o.ramChannels !== null ? ramChannels(r) : null,
    toConfirm: o.ramChannels === null && r.channelsSource === 'estimated',
    editable: true,
  });

  const c = d.cpu;
  rows.push({
    key: 'cpu',
    label: 'Processeur',
    value: shortCpuName(c.name),
    detail: cpuSpecs(d),
    source: 'detected',
    detected: null,
    toConfirm: false,
    editable: false,
  });
  return rows;
}

/** Teinte de l'étiquette d'origine : mesurée, table, estimée (attention), corrigée. */
export function sourceTone(s: Source): TagTone {
  switch (s) {
    case 'detected':
    case 'nvml':
      return 'ok';
    case 'estimated':
      return 'warn';
    case 'manual':
      return 'accent';
    default:
      return 'neutral';
  }
}

const EDIT_LABELS: Record<DetectedKey, string> = {
  gpu: 'Choisir la carte graphique retenue',
  gpuType: 'Corriger le type de GPU',
  bandwidth: 'Corriger la bande passante du GPU',
  ram: '',
  channels: 'Corriger le nombre de canaux',
  cpu: '',
};

type IntegratedChoice = 'auto' | 'yes' | 'no';

/**
 * Dialogue « Corriger… » (§6.6) : chaque valeur détectée sur une ligne avec son origine et un
 * bouton crayon (corrections enregistrées d'un lancement à l'autre), puis la simulation d'un
 * autre GPU ou d'une autre quantité de RAM, jusqu'à la fermeture de l'application. Le profil qui
 * en résulte est calculé en Rust.
 */
@Component({
  selector: 'app-corrections-dialog',
  imports: [Button, Callout, FIELD, GDialog, Icon, Progress, Select, Tag, ToggleGroup, Tooltip],
  templateUrl: './corrections-dialog.html',
  styleUrl: './corrections-dialog.scss',
})
export class CorrectionsDialog {
  private readonly tauri = inject(TauriService);
  private readonly hardware = inject(HardwareStore);
  private readonly dialogRef = inject(DialogRef);
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
  private readonly injector = inject(Injector);

  protected readonly sourceLabels = SOURCE_LABELS;
  protected readonly sourceTone = sourceTone;
  protected readonly editLabels = EDIT_LABELS;
  protected readonly sameNumber = sameNumber;

  protected readonly options = signal<HardwareOptions | null>(null);
  protected readonly draft = signal<Overrides>({
    ...NO_OVERRIDES,
    ...this.hardware.profile()?.overrides,
  });
  protected readonly busy = signal(false);
  protected readonly error = signal<string | null>(null);
  /** Ligne dont le correcteur est ouvert (une à la fois). */
  protected readonly editing = signal<DetectedKey | null>(null);

  protected readonly detected = computed(() => this.options()?.detected ?? null);
  protected readonly rows = computed(() => {
    const d = this.detected();
    return d ? detectedRows(d, this.draft()) : [];
  });
  protected readonly chosen = computed(() => {
    const d = this.detected();
    return d ? chosenGpu(d, this.draft()) : null;
  });

  // ── Correcteurs ───────────────────────────────────────────────────────────────────────────

  protected readonly gpuOptions = computed<SelectOption<number>[]>(() => {
    const d = this.detected();
    return [
      { value: -1, label: 'Automatique (le plus de VRAM)' },
      ...(d ? detectedGpus(d) : []).map((g) => ({
        value: g.index,
        label: g.name,
        hint: roundSize(g.vramTotal),
      })),
    ];
  });

  protected readonly integratedOptions = computed<ToggleOption<IntegratedChoice>[]>(() => [
    { value: 'auto', label: `Détecté (${this.chosen()?.integrated ? 'intégré' : 'dédié'})` },
    { value: 'yes', label: 'Intégré', tooltip: 'Compté comme du CPU : il partage la RAM' },
    { value: 'no', label: 'Dédié', tooltip: 'Les couches du modèle vont dans sa VRAM' },
  ]);

  protected readonly integratedValue = computed<IntegratedChoice>(() => {
    const v = this.draft().gpuIntegrated;
    return v === null ? 'auto' : v ? 'yes' : 'no';
  });

  protected readonly channelOptions = computed<SelectOption<number>[]>(() => {
    const r = this.detected()?.ram;
    return [
      { value: 0, label: r ? `Détecté (${r.channels})` : 'Détecté' },
      ...RAM_CHANNELS.map((c) => ({ value: c, label: channels(c) })),
    ];
  });

  protected readonly channelsHint = computed(() => {
    const r = this.detected()?.ram;
    if (!r) return '';
    const detected = `Détecté : ${r.channels} (valeur ${SOURCE_LABELS[r.channelsSource]})`;
    if (r.modules === 0) return detected;
    return `${detected} · ${r.modules} barrette${r.modules > 1 ? 's' : ''}`;
  });

  protected readonly bandwidthText = computed(() => {
    const v = this.draft().gpuBandwidthGbs;
    return v === null ? '' : decimal(v, 1);
  });

  // ── Simulation ────────────────────────────────────────────────────────────────────────────

  protected readonly simGpuOptions = computed<SelectOption<string>[]>(() => [
    { value: '', label: 'Aucun (GPU réel)' },
    ...groupGpus(this.options()?.gpus ?? []).flatMap((group) =>
      group.gpus.map((g) => ({
        value: g.label,
        label: g.label,
        hint: bandwidth(g.bandwidthGbs),
        group: group.vendor,
      })),
    ),
  ]);

  protected readonly simRamOptions: SelectOption<number>[] = [
    { value: 0, label: 'Aucune (RAM réelle)' },
    ...SIMULATED_RAM_GB.map((gb) => ({ value: gbToBytes(gb), label: `${gb} Go` })),
  ];

  protected readonly simulatingGpu = computed(() => this.draft().simulatedGpu !== null);

  protected readonly changed = computed(() => {
    const current = { ...NO_OVERRIDES, ...this.hardware.profile()?.overrides };
    return JSON.stringify(current) !== JSON.stringify(this.draft());
  });

  constructor() {
    this.tauri
      .hardwareOptions()
      .then((o) => this.options.set(o))
      .catch((e: unknown) => this.error.set(errorMessage(e)));
  }

  protected patch(patch: Partial<Overrides>): void {
    this.draft.update((d) => ({ ...d, ...patch }));
  }

  /** Bouton crayon : ouvre le correcteur de la ligne (focus sur son contrôle) ou le referme. */
  protected toggleEditor(key: DetectedKey): void {
    if (this.editing() === key) {
      this.editing.set(null);
      return;
    }
    this.editing.set(key);
    afterNextRender(
      () => {
        const editor = this.host.querySelector<HTMLElement>(`#fix-editor-${key}`);
        const control =
          editor?.querySelector<HTMLElement>('[aria-pressed="true"]') ??
          editor?.querySelector<HTMLElement>('input, button');
        control?.focus();
      },
      { injector: this.injector },
    );
  }

  /** La valeur détectée revient pour cette ligne (appliqué avec « Appliquer »). */
  protected resetRow(key: DetectedKey): void {
    switch (key) {
      case 'gpu':
        return this.patch({ gpuIndex: null });
      case 'gpuType':
        return this.patch({ gpuIntegrated: null });
      case 'bandwidth':
        return this.patch({ gpuBandwidthGbs: null });
      case 'channels':
        return this.patch({ ramChannels: null });
    }
  }

  protected onIntegrated(choice: IntegratedChoice | null): void {
    this.patch({ gpuIntegrated: choice === null || choice === 'auto' ? null : choice === 'yes' });
  }

  protected onBandwidth(value: string): void {
    this.patch({ gpuBandwidthGbs: parsePositive(value) });
  }

  /** Revient aux valeurs détectées, sans simulation (appliqué avec « Appliquer »). */
  protected resetAll(): void {
    this.draft.set({ ...NO_OVERRIDES });
    this.editing.set(null);
  }

  protected close(): void {
    this.dialogRef.close();
  }

  protected async apply(): Promise<void> {
    this.busy.set(true);
    this.error.set(null);
    try {
      await this.hardware.setOverrides(this.draft());
      this.dialogRef.close(true);
    } catch (e) {
      this.error.set(errorMessage(e));
    } finally {
      this.busy.set(false);
    }
  }
}
