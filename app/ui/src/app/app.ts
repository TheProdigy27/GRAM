import { Component, inject } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';
import { MatSidenavModule } from '@angular/material/sidenav';
import { MatSnackBar } from '@angular/material/snack-bar';
import { MatToolbarModule } from '@angular/material/toolbar';
import { MatTooltipModule } from '@angular/material/tooltip';
import { errorMessage } from './core/tauri.service';
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

/**
 * Coquille de l'application. Au lancement : matériel, paramètres et premier classement en
 * parallèle (catalogue du cache, sinon embarqué), puis synchronisation du catalogue en arrière-
 * plan (rien n'est téléchargé si le cache a moins de 24 h).
 */
@Component({
  selector: 'app-root',
  imports: [
    MatButtonModule,
    MatIconModule,
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
  private readonly snackBar = inject(MatSnackBar);

  constructor() {
    // Le classement part dès la création du store, sans attendre le reste.
    inject(RankingStore);
    void this.hardware.load();
    void inject(SettingsStore).load();
    void this.catalog.sync(false);
  }

  protected rescan(): void {
    this.hardware.rescan().catch((e: unknown) => {
      this.snackBar.open(`Analyse impossible : ${errorMessage(e)}`, 'OK');
    });
  }

  protected refreshCatalog(): void {
    void this.catalog.sync(true);
  }
}
