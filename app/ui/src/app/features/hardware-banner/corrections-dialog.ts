import { Component, computed, inject, signal } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatDialogModule, MatDialogRef } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatSelectModule } from '@angular/material/select';
import type { GpuInfo } from '../../core/generated/GpuInfo';
import type { HardwareOptions } from '../../core/generated/HardwareOptions';
import type { Overrides } from '../../core/generated/Overrides';
import type { SimulationGpu } from '../../core/generated/SimulationGpu';
import { TauriService, errorMessage } from '../../core/tauri.service';
import { bandwidth, gbToBytes, roundSize } from '../../shared/format';
import { SOURCE_LABELS } from '../../shared/labels';
import { HardwareStore } from '../../state/hardware.store';

/** Aucune correction, aucune simulation : valeurs détectées. */
export const NO_OVERRIDES: Overrides = {
  ramChannels: null,
  gpuBandwidthGbs: null,
  gpuIntegrated: null,
  gpuIndex: null,
  simulatedGpu: null,
  simulatedRamTotal: null,
};

/** Canaux mémoire proposés (64 bits chacun). */
export const RAM_CHANNELS = [1, 2, 3, 4, 6, 8] as const;

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

/** Nombre décimal saisi (virgule acceptée) ; vide ou invalide : `null`. */
export function parsePositive(value: string): number | null {
  const x = Number(value.trim().replace(',', '.'));
  return value.trim() !== '' && Number.isFinite(x) && x > 0 ? x : null;
}

/**
 * Dialogue « Corriger… » (§6.6) : corrections des valeurs détectées, enregistrées d'un
 * lancement à l'autre, et simulation d'un autre GPU ou d'une autre quantité de RAM, jusqu'à la
 * fermeture de l'application. Le profil qui en résulte est calculé en Rust.
 */
@Component({
  selector: 'app-corrections-dialog',
  imports: [
    MatButtonModule,
    MatDialogModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatProgressBarModule,
    MatSelectModule,
  ],
  templateUrl: './corrections-dialog.html',
  styleUrl: './corrections-dialog.scss',
})
export class CorrectionsDialog {
  private readonly tauri = inject(TauriService);
  private readonly hardware = inject(HardwareStore);
  private readonly dialogRef = inject(MatDialogRef<CorrectionsDialog>);

  protected readonly ramChannels = RAM_CHANNELS;
  protected readonly simulatedRam = SIMULATED_RAM_GB.map((gb) => ({
    gb,
    bytes: gbToBytes(gb),
  }));
  protected readonly bandwidth = bandwidth;
  protected readonly roundSize = roundSize;

  protected readonly options = signal<HardwareOptions | null>(null);
  protected readonly draft = signal<Overrides>({
    ...NO_OVERRIDES,
    ...this.hardware.profile()?.overrides,
  });
  protected readonly busy = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly detected = computed(() => this.options()?.detected ?? null);
  protected readonly gpuGroups = computed(() => groupGpus(this.options()?.gpus ?? []));

  /** Tous les GPU détectés, pour choisir le GPU retenu (seulement s'il y en a plusieurs). */
  protected readonly gpus = computed<GpuInfo[]>(() => {
    const d = this.detected();
    return d ? [...(d.gpu ? [d.gpu] : []), ...d.otherGpus] : [];
  });

  /** GPU détecté qui serait retenu avec le choix courant (pour les rappels « détecté : … »). */
  protected readonly chosenGpu = computed(() => {
    const index = this.draft().gpuIndex;
    const all = this.gpus();
    return (index !== null ? all.find((g) => g.index === index) : undefined) ?? all[0] ?? null;
  });

  protected readonly channelsHint = computed(() => {
    const r = this.detected()?.ram;
    if (!r) return '';
    const modules = `${r.modules} barrette${r.modules > 1 ? 's' : ''}`;
    return `Détecté : ${r.channels} (valeur ${SOURCE_LABELS[r.channelsSource]}) · ${modules}`;
  });

  protected readonly bandwidthHint = computed(() => {
    const g = this.chosenGpu();
    if (!g?.bandwidthGbs) return '';
    return `Détectée : ${bandwidth(g.bandwidthGbs)} (${SOURCE_LABELS[g.bandwidthSource]})`;
  });

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

  protected onBandwidth(value: string): void {
    this.patch({ gpuBandwidthGbs: parsePositive(value) });
  }

  /** Revient aux valeurs détectées (appliqué avec « Appliquer »). */
  protected resetAll(): void {
    this.draft.set({ ...NO_OVERRIDES });
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
