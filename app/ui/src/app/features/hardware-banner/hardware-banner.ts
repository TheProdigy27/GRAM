import { Component, PendingTasks, computed, inject } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog } from '@angular/material/dialog';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatSnackBar } from '@angular/material/snack-bar';
import { MatTooltipModule } from '@angular/material/tooltip';
import type { GpuInfo } from '../../core/generated/GpuInfo';
import type { HardwareProfile } from '../../core/generated/HardwareProfile';
import type { RuntimeStatus } from '../../core/generated/RuntimeStatus';
import type { Source } from '../../core/generated/Source';
import { errorMessage } from '../../core/tauri.service';
import { bandwidth, gb, roundSize } from '../../shared/format';
import { RAM_KIND_LABELS, RUNTIMES, SOURCE_LABELS } from '../../shared/labels';
import { HardwareStore } from '../../state/hardware.store';
import { RankingStore } from '../../state/ranking.store';

/** Une valeur du bandeau, avec son origine quand elle n'est pas mesurée directement. */
export interface BannerPart {
  text: string;
  /** « estimée », « table des GPU »… affiché en retrait (absent : valeur détectée). */
  source?: string;
}

/** Sources affichées à côté de la valeur : celles qui ne sont pas une mesure directe. */
function sourceNote(s: Source): string | undefined {
  return s === 'detected' || s === 'nvml' ? undefined : SOURCE_LABELS[s];
}

export function cpuParts(p: HardwareProfile): BannerPart[] {
  const c = p.cpu;
  const isa = c.avx512 ? 'AVX-512' : c.avx2 ? 'AVX2' : 'sans AVX2';
  return [{ text: c.name }, { text: `${c.coresPhysical}c/${c.coresLogical}t` }, { text: isa }];
}

export function ramParts(p: HardwareProfile): BannerPart[] {
  const r = p.ram;
  const kind = RAM_KIND_LABELS[r.kind];
  const type =
    kind && r.speedMts ? `${kind}-${r.speedMts}` : kind || (r.speedMts ? `${r.speedMts} MT/s` : '');
  const parts: BannerPart[] = [
    { text: [roundSize(r.installed ?? r.total), type].filter(Boolean).join(' ') },
  ];
  parts.push({
    text: `${r.channels} ${r.channels > 1 ? 'canaux' : 'canal'}`,
    source: sourceNote(r.channelsSource),
  });
  if (r.bandwidthGbs) parts.push({ text: bandwidth(r.bandwidthGbs) });
  return parts;
}

export function gpuParts(p: HardwareProfile): BannerPart[] {
  const g: GpuInfo | null = p.gpu;
  if (!g) return [{ text: 'Aucun GPU : calcul sur le CPU' }];
  const parts: BannerPart[] = [{ text: g.name }];
  if (g.integrated) {
    parts.push({ text: 'intégré : compté comme du CPU (mémoire partagée)' });
    return parts;
  }
  parts.push({ text: roundSize(g.vramTotal) });
  if (g.bandwidthGbs) {
    parts.push({ text: bandwidth(g.bandwidthGbs), source: sourceNote(g.bandwidthSource) });
  }
  const b = p.backends;
  if (b.cuda) parts.push({ text: b.cudaDriverVersion ? `CUDA ${b.cudaDriverVersion}` : 'CUDA' });
  if (b.vulkan) parts.push({ text: 'Vulkan' });
  return parts;
}

export function diskParts(p: HardwareProfile): BannerPart[] {
  return p.disks.map((d) => ({
    text: `${d.mount.replace(/\\$/, '')} ${roundSize(d.available)} libres`,
  }));
}

/**
 * Bandeau du haut : matériel détecté, origine des valeurs, runtimes, mémoire utilisable, et
 * accès aux corrections et à la simulation (« Corriger… »).
 */
@Component({
  selector: 'app-hardware-banner',
  imports: [MatButtonModule, MatIconModule, MatProgressBarModule, MatTooltipModule],
  templateUrl: './hardware-banner.html',
  styleUrl: './hardware-banner.scss',
})
export class HardwareBanner {
  private readonly hardware = inject(HardwareStore);
  private readonly ranking = inject(RankingStore);
  private readonly dialog = inject(MatDialog);
  private readonly snackBar = inject(MatSnackBar);
  private readonly pendingTasks = inject(PendingTasks);

  protected readonly profile = this.hardware.profile;
  protected readonly error = this.hardware.error;

  protected readonly rows = computed(() => {
    const p = this.profile();
    if (!p) return [];
    // Les GPU non retenus (décision : le plus gros seulement) sont signalés sur la ligne GPU.
    const others = p.otherGpus.map((g) => g.name);
    return [
      { key: 'CPU', parts: cpuParts(p) },
      { key: 'RAM', parts: ramParts(p) },
      {
        key: 'GPU',
        parts: gpuParts(p),
        note:
          others.length > 0 ? `+${others.length} non utilisé${others.length > 1 ? 's' : ''}` : '',
        noteTooltip: `Seul le GPU qui a le plus de VRAM est utilisé pour le calcul. Non utilisé : ${others.join(', ')}`,
      },
      { key: 'Disques', parts: diskParts(p) },
    ].map((row) => ({
      note: '',
      noteTooltip: '',
      ...row,
      // Texte complet, en infobulle native quand la ligne est tronquée.
      title: row.parts.map((x) => (x.source ? `${x.text} (${x.source})` : x.text)).join(' · '),
    }));
  });

  protected readonly simulated = this.hardware.simulation;

  /** Ce qui est simulé, en infobulle de l'étiquette « Simulation ». */
  protected readonly simulationTooltip = computed(() => {
    const o = this.profile()?.overrides;
    if (!o) return '';
    const parts = [
      o.simulatedGpu && `GPU ${o.simulatedGpu}`,
      o.simulatedRamTotal !== null && `RAM ${roundSize(o.simulatedRamTotal)}`,
    ].filter(Boolean);
    return `Classement calculé pour un matériel simulé : ${parts.join(', ')}.`;
  });

  protected readonly runtimes = computed(() => {
    const status = this.hardware.runtimes();
    if (!status) return null;
    return RUNTIMES.map(({ value, label }) => {
      const s: RuntimeStatus = status[value];
      const state = !s.installed ? 'non installé' : s.running ? 'en cours d’exécution' : 'installé';
      return {
        label,
        installed: s.installed,
        running: s.running,
        tooltip: [state, s.version && `version ${s.version}`, s.path].filter(Boolean).join(' · '),
      };
    });
  });

  /** Mémoire réellement utilisable pour un modèle (marges des paramètres appliquées). */
  protected readonly usable = computed(() => {
    const b = this.ranking.response()?.budget;
    if (!b) return null;
    return b.vram > 0 ? `VRAM ${gb(b.vram)} · RAM ${gb(b.ram)}` : `RAM ${gb(b.ram)}`;
  });

  protected readonly warnings = computed(() => (this.profile()?.warnings ?? []).join('\n'));

  /** Le dialogue est chargé à la première ouverture. */
  protected openCorrections(): void {
    this.pendingTasks.run(async () => {
      const { CorrectionsDialog } = await import('./corrections-dialog');
      this.dialog.open(CorrectionsDialog, { width: '720px', maxWidth: '92vw', autoFocus: false });
    });
  }

  protected stopSimulation(): void {
    this.hardware.stopSimulation().catch((e: unknown) => {
      this.snackBar.open(`Simulation non arrêtée : ${errorMessage(e)}`, 'OK');
    });
  }
}
