import {
  Component,
  DestroyRef,
  booleanAttribute,
  computed,
  inject,
  input,
  linkedSignal,
} from '@angular/core';
import { AppCommands } from '../../app-commands';
import type { HardwareProfile } from '../../core/generated/HardwareProfile';
import { ShortcutsService } from '../../core/shortcuts.service';
import { errorMessage } from '../../core/tauri.service';
import { roundSize } from '../../shared/format';
import { runtimeRows } from '../../shared/runtimes';
import { Button } from '../../shared/ui/button/button';
import { Callout } from '../../shared/ui/callout/callout';
import { Icon } from '../../shared/ui/icon/icon';
import { Skeleton } from '../../shared/ui/skeleton/skeleton';
import { Tag } from '../../shared/ui/tag/tag';
import { ToastService } from '../../shared/ui/toast/toast';
import { Tooltip } from '../../shared/ui/tooltip/tooltip';
import { HardwareStore } from '../../state/hardware.store';
import { RankingStore } from '../../state/ranking.store';
import { machineCards, machinePills, shortGpuName, usableSummary } from './machine-cards';

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
 * Bande « Votre machine » (maquette A1 · Atelier) : mémoire utilisable par un modèle, puis une
 * carte par composant (GPU et RAM avec leur barre « Go utilisables », CPU et disques, runtimes),
 * avertissements, « Ré-analyser » (touche R) et « Corriger ou simuler… ».
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
  private readonly commands = inject(AppCommands);
  private readonly toast = inject(ToastService);

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

  /** Mémoire utilisable par un modèle, calculée avec le classement. */
  private readonly budget = computed(() => this.ranking.response()?.budget ?? null);
  /** Budget attendu (classement en cours) : squelette à la place des barres. */
  protected readonly budgetPending = computed(() => !this.budget() && !this.ranking.error());

  protected readonly summary = computed(() => {
    const b = this.budget();
    return b ? usableSummary(b) : null;
  });

  protected readonly cards = computed(() => {
    const p = this.profile();
    return p ? machineCards(p, this.budget()) : [];
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
    const b = this.budget();
    return p && b ? machinePills(b, p) : null;
  });

  /** Runtime choisi pour le classement et son état (bande resserrée). */
  protected readonly runtime = computed(() => {
    const id = this.ranking.effectiveRuntime();
    return this.runtimes()?.find((r) => r.value === id) ?? null;
  });

  /** Bouton « Ré-analyser » et touche R : nouvelle détection du matériel et des runtimes. */
  protected rescan(): void {
    this.commands.rescan();
  }

  protected openCorrections(): void {
    this.commands.openCorrections();
  }

  protected stopSimulation(): void {
    this.hardware.stopSimulation().catch((e: unknown) => {
      this.toast.show(`Simulation non arrêtée : ${errorMessage(e)}`, { kind: 'error' });
    });
  }
}
