import { Injectable, Injector, PendingTasks, inject, signal } from '@angular/core';
import type { ExportFormat } from './core/generated/ExportFormat';
import { TauriService, errorMessage } from './core/tauri.service';
import { UpdateLauncher } from './features/update/update-launcher';
import { DialogService } from './shared/ui/dialog/dialog';
import { ToastService } from './shared/ui/toast/toast';
import { CatalogStore } from './state/catalog.store';
import { HardwareStore } from './state/hardware.store';
import { RankingStore } from './state/ranking.store';

/**
 * Actions de la coque, partagées par l'en-tête, la bande « Votre machine », les raccourcis et la
 * palette Ctrl F. Les dialogues sont chargés à leur première ouverture ; le chargement compte
 * comme une tâche en cours (l'application n'est pas « stable » avant l'ouverture).
 */
@Injectable({ providedIn: 'root' })
export class AppCommands {
  private readonly tauri = inject(TauriService);
  private readonly dialogs = inject(DialogService);
  private readonly toast = inject(ToastService);
  private readonly pendingTasks = inject(PendingTasks);
  private readonly updateLauncher = inject(UpdateLauncher);
  private readonly catalog = inject(CatalogStore);
  private readonly hardware = inject(HardwareStore);
  private readonly ranking = inject(RankingStore);

  private readonly injector = inject(Injector);

  readonly exporting = signal(false);
  private paletteOpen = false;

  /** Le classement est là et aucun export n'est en cours. */
  canExport(): boolean {
    return !this.exporting() && !!this.ranking.response();
  }

  /**
   * Exporte le classement affiché (filtres compris, toutes les lignes). Le dialogue
   * « Enregistrer sous » est ouvert par le backend.
   */
  async exportRanking(format: ExportFormat): Promise<void> {
    if (this.exporting()) return;
    this.exporting.set(true);
    try {
      const path = await this.tauri.exportRanking(format);
      if (path) {
        this.toast.show(`Classement exporté : ${path}`, { kind: 'success', duration: 6000 });
      }
    } catch (e) {
      this.toast.show(`Export impossible : ${errorMessage(e)}`, { kind: 'error' });
    } finally {
      this.exporting.set(false);
    }
  }

  /** « Actualiser les modèles » : synchronisation forcée du catalogue. */
  refreshCatalog(): void {
    void this.catalog.sync(true);
  }

  /** « Ré-analyser » et touche R : nouvelle détection du matériel et des runtimes. */
  rescan(): void {
    this.hardware.rescan().catch((e: unknown) => {
      this.toast.show(`Analyse impossible : ${errorMessage(e)}`, { kind: 'error' });
    });
  }

  openSettings(): void {
    this.pendingTasks.run(async () => {
      const { SettingsDialog } = await import('./features/settings/settings-dialog');
      this.dialogs.open(SettingsDialog, { width: '1040px' });
    });
  }

  /** « Corriger… » : corrections du matériel détecté et simulation. */
  openCorrections(): void {
    this.pendingTasks.run(async () => {
      const { CorrectionsDialog } = await import('./features/hardware-banner/corrections-dialog');
      this.dialogs.open(CorrectionsDialog, { width: '720px' });
    });
  }

  openUpdate(): void {
    this.updateLauncher.open();
  }

  /** Touche ? : aide des raccourcis. */
  openShortcuts(): void {
    this.pendingTasks.run(async () => {
      const { ShortcutsHelp } = await import('./features/shortcuts-help/shortcuts-help');
      this.dialogs.open(ShortcutsHelp, { width: '800px' });
    });
  }

  /**
   * Ctrl F et champ de l'en-tête : palette de recherche, en haut de la fenêtre. Le focus revient
   * à `origin` à la fermeture (WebKit ne donne pas le focus à un bouton cliqué), sinon là où il
   * était.
   */
  openPalette(origin?: HTMLElement): void {
    // Une seule palette, même si Ctrl F est répété pendant son chargement.
    if (this.paletteOpen) return;
    this.paletteOpen = true;
    this.pendingTasks.run(async () => {
      try {
        const { Palette, paletteConfig } = await import('./features/palette/palette');
        this.dialogs
          .open(Palette, paletteConfig(this.injector, origin))
          .closed.subscribe(() => (this.paletteOpen = false));
      } catch (e) {
        this.paletteOpen = false;
        throw e;
      }
    });
  }
}
