import { Component, DestroyRef, PendingTasks, inject, signal } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog } from '@angular/material/dialog';
import { MatIconModule } from '@angular/material/icon';
import { MatMenuModule } from '@angular/material/menu';
import { MatSidenavModule } from '@angular/material/sidenav';
import { MatSnackBar } from '@angular/material/snack-bar';
import { MatToolbarModule } from '@angular/material/toolbar';
import { MatTooltipModule } from '@angular/material/tooltip';
import type { ExportFormat } from './core/generated/ExportFormat';
import { TauriService, errorMessage } from './core/tauri.service';
import { FiltersPanel } from './features/filters-panel/filters-panel';
import { HardwareBanner } from './features/hardware-banner/hardware-banner';
import { ModelDetailPanel } from './features/model-detail/model-detail';
import { RankingView } from './features/ranking/ranking-view';
import { StatusBar } from './features/status-bar/status-bar';
import { CatalogStore } from './state/catalog.store';
import { DetailStore } from './state/detail.store';
import { HardwareStore } from './state/hardware.store';
import { RankingStore } from './state/ranking.store';
import { SettingsStore } from './state/settings.store';
import { UPDATE_CHECK_DELAY_MS, UpdateStore } from './state/update.store';

/**
 * Coquille de l'application. Au lancement : matériel, paramètres et premier classement en
 * parallèle (catalogue du cache, sinon embarqué), puis synchronisation du catalogue en arrière-
 * plan (rien n'est téléchargé si le cache a moins de 24 h). Quelques secondes plus tard, GRAM
 * cherche une mise à jour dans les releases GitHub, sans rien afficher s'il n'y en a pas.
 */
@Component({
  selector: 'app-root',
  imports: [
    MatButtonModule,
    MatIconModule,
    MatMenuModule,
    MatSidenavModule,
    MatToolbarModule,
    MatTooltipModule,
    FiltersPanel,
    HardwareBanner,
    ModelDetailPanel,
    RankingView,
    StatusBar,
  ],
  templateUrl: './app.html',
  styleUrl: './app.scss',
})
export class App {
  protected readonly hardware = inject(HardwareStore);
  protected readonly catalog = inject(CatalogStore);
  protected readonly detail = inject(DetailStore);
  protected readonly ranking = inject(RankingStore);
  protected readonly updates = inject(UpdateStore);
  private readonly tauri = inject(TauriService);
  private readonly dialog = inject(MatDialog);
  private readonly snackBar = inject(MatSnackBar);
  private readonly pendingTasks = inject(PendingTasks);

  protected readonly exporting = signal(false);

  constructor() {
    // Le classement part dès la création du store (injecté ci-dessus), sans attendre le reste.
    void this.hardware.load();
    void inject(SettingsStore).load();
    void this.catalog.sync(false);

    const timer = setTimeout(() => void this.checkUpdate(), inject(UPDATE_CHECK_DELAY_MS));
    inject(DestroyRef).onDestroy(() => clearTimeout(timer));
  }

  /** Vérification automatique : une mise à jour trouvée est signalée, un échec ignoré. */
  private async checkUpdate(): Promise<void> {
    const update = await this.updates.check(true);
    if (!update) return;
    this.snackBar
      .open(`GRAM ${update.version} est disponible.`, 'Voir', { duration: 10000 })
      .onAction()
      .subscribe(() => this.openUpdate());
  }

  protected rescan(): void {
    this.hardware.rescan().catch((e: unknown) => {
      this.snackBar.open(`Analyse impossible : ${errorMessage(e)}`, 'OK');
    });
  }

  protected refreshCatalog(): void {
    void this.catalog.sync(true);
  }

  /**
   * Le dialogue (et les onglets Material) est chargé à la première ouverture ; le chargement
   * compte comme une tâche en cours (l'application n'est pas « stable » avant l'ouverture).
   */
  protected openSettings(): void {
    this.pendingTasks.run(async () => {
      const { SettingsDialog } = await import('./features/settings/settings-dialog');
      this.dialog.open(SettingsDialog, { width: '820px', maxWidth: '94vw', autoFocus: false });
    });
  }

  /** Dialogue « Mise à jour », chargé à la première ouverture comme celui des paramètres. */
  protected openUpdate(): void {
    this.pendingTasks.run(async () => {
      const { UpdateDialog } = await import('./features/update/update-dialog');
      this.dialog.open(UpdateDialog, { width: '560px', maxWidth: '94vw', autoFocus: false });
    });
  }

  /**
   * Exporte le classement affiché (filtres compris, toutes les lignes). Le dialogue
   * « Enregistrer sous » est ouvert par le backend.
   */
  protected async exportRanking(format: ExportFormat): Promise<void> {
    if (this.exporting()) return;
    this.exporting.set(true);
    try {
      const path = await this.tauri.exportRanking(format);
      if (path) this.snackBar.open(`Classement exporté : ${path}`, 'OK', { duration: 6000 });
    } catch (e) {
      this.snackBar.open(`Export impossible : ${errorMessage(e)}`, 'OK');
    } finally {
      this.exporting.set(false);
    }
  }
}
