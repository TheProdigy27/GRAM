import { CdkMenuTrigger } from '@angular/cdk/menu';
import {
  Component,
  DestroyRef,
  ElementRef,
  Injector,
  PendingTasks,
  afterNextRender,
  inject,
  linkedSignal,
  signal,
  viewChild,
} from '@angular/core';
import { MatDialog } from '@angular/material/dialog';
import { MatSnackBar } from '@angular/material/snack-bar';
import type { ExportFormat } from './core/generated/ExportFormat';
import { ShortcutsService } from './core/shortcuts.service';
import { TauriService, errorMessage } from './core/tauri.service';
import { FiltersPanel } from './features/filters-panel/filters-panel';
import { HardwareBanner } from './features/hardware-banner/hardware-banner';
import { ModelDetailPanel } from './features/model-detail/model-detail';
import { RankingView } from './features/ranking/ranking-view';
import { StatusBar } from './features/status-bar/status-bar';
import { Button } from './shared/ui/button/button';
import { FIELD } from './shared/ui/field/field';
import { Icon } from './shared/ui/icon/icon';
import { MENU } from './shared/ui/menu/menu';
import { Tooltip } from './shared/ui/tooltip/tooltip';
import { CatalogStore } from './state/catalog.store';
import { DetailStore } from './state/detail.store';
import { HardwareStore } from './state/hardware.store';
import { RankingStore } from './state/ranking.store';
import { SettingsStore } from './state/settings.store';
import { UPDATE_CHECK_DELAY_MS, UpdateStore } from './state/update.store';

/**
 * Coque de l'application : en-tête (recherche, export, mise à jour, paramètres), bande
 * « Votre machine », classement et fiche, barre d'état. La fiche s'ancre à droite du classement,
 * qui reste cliquable : la bande machine passe en une ligne de pastilles, les filtres se replient
 * en un bouton « Filtres » qui les ouvre par-dessus le classement. Au lancement : matériel, paramètres et
 * premier classement en parallèle (catalogue du cache, sinon embarqué), puis synchronisation du
 * catalogue en arrière-plan (rien n'est téléchargé si le cache a moins de 24 h). Quelques
 * secondes plus tard, GRAM cherche une mise à jour dans les releases GitHub, sans rien afficher
 * s'il n'y en a pas.
 */
@Component({
  selector: 'app-root',
  imports: [
    Button,
    FIELD,
    Icon,
    MENU,
    Tooltip,
    FiltersPanel,
    HardwareBanner,
    ModelDetailPanel,
    RankingView,
    StatusBar,
  ],
  templateUrl: './app.html',
  styleUrl: './app.scss',
  host: { '(document:pointerdown)': 'onDocumentPointerDown($event)' },
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
  private readonly shortcuts = inject(ShortcutsService);
  private readonly injector = inject(Injector);

  private readonly searchInput = viewChild.required<ElementRef<HTMLInputElement>>('search');
  private readonly exportTrigger = viewChild.required(CdkMenuTrigger);
  private readonly rankingView = viewChild.required(RankingView);
  private readonly filters = viewChild.required<FiltersPanel, ElementRef<HTMLElement>>(
    FiltersPanel,
    {
      read: ElementRef,
    },
  );
  private readonly pane = viewChild<ElementRef<HTMLElement>>('pane');

  protected readonly exporting = signal(false);
  /** Filtres ouverts par-dessus le classement (fiche ouverte) ; refermés avec la fiche. */
  protected readonly filtersOpen = linkedSignal({
    source: this.detail.isOpen,
    computation: () => false,
  });
  /** « Control+K » (« Meta+K » sur Mac) pour `aria-keyshortcuts`, « Ctrl K » à l'écran. */
  protected readonly searchShortcut = this.shortcuts.isMac ? 'Meta+K' : 'Control+K';
  protected readonly searchShortcutLabel = this.shortcuts.modShortcut('K');

  constructor() {
    // Le classement part dès la création du store (injecté ci-dessus), sans attendre le reste.
    void this.hardware.load();
    void inject(SettingsStore).load();
    void this.catalog.sync(false);

    const timer = setTimeout(() => void this.checkUpdate(), inject(UPDATE_CHECK_DELAY_MS));
    const destroyRef = inject(DestroyRef);
    destroyRef.onDestroy(() => clearTimeout(timer));

    destroyRef.onDestroy(
      this.shortcuts.register([
        // Ctrl F reste accepté : c'était le raccourci de la recherche.
        { key: 'k', mod: true, inInputs: true, run: () => this.focusSearch() },
        { key: 'f', mod: true, inInputs: true, run: () => this.focusSearch() },
        { key: 'e', run: () => this.openExportMenu() },
        { key: 'Escape', inInputs: true, run: () => this.onEscape() },
      ]),
    );
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

  /** Ctrl K (⌘K) : curseur dans la recherche, texte sélectionné. */
  private focusSearch(): void {
    const input = this.searchInput().nativeElement;
    input.focus();
    input.select();
  }

  /** Échap dans la recherche : la vide d'abord ; vide, Échap ferme la fiche. */
  protected clearSearch(event: Event): void {
    if (this.ranking.search() === '') return;
    event.preventDefault();
    this.ranking.setSearch('');
  }

  /** Touche E : ouvre le menu Exporter, focus sur son premier élément. */
  private openExportMenu(): boolean {
    if (this.exporting() || !this.ranking.response()) return false;
    const trigger = this.exportTrigger();
    trigger.open();
    trigger.getMenu()?.focusFirstItem('keyboard');
    return true;
  }

  /** Échap : referme les filtres ouverts par-dessus le classement, sinon la fiche. */
  private onEscape(): boolean {
    if (this.filtersOpen()) {
      this.closeFilters(true);
      return true;
    }
    return this.closeDetail();
  }

  /**
   * Ferme la fiche. Le focus revient sur la ligne du modèle dans le classement : toujours avec
   * le bouton ✕ (`always`), avec Échap seulement si le focus était dans la fiche ou nulle part
   * (Échap dans la recherche la laisse au champ).
   */
  protected closeDetail(always = false): boolean {
    const repoId = this.detail.repoId();
    if (repoId === null) return false;
    const active = document.activeElement;
    const fromPane =
      !active || active === document.body || !!this.pane()?.nativeElement.contains(active);
    this.detail.close();
    if (always || fromPane) {
      afterNextRender(() => this.rankingView().focusModel(repoId), { injector: this.injector });
    }
    return true;
  }

  // ── Filtres repliés (fiche ouverte) ───────────────────────────────────────────────────────

  /** Bouton « Filtres » : ouvre les filtres par-dessus le classement, focus sur le premier. */
  protected toggleFilters(): void {
    if (this.filtersOpen()) {
      this.closeFilters(false);
      return;
    }
    this.filtersOpen.set(true);
    afterNextRender(
      () => {
        const host = this.filters().nativeElement;
        const first =
          host.querySelector<HTMLElement>('[aria-pressed="true"]') ??
          host.querySelector<HTMLElement>('button, input');
        first?.focus();
      },
      { injector: this.injector },
    );
  }

  private closeFilters(focusToggle: boolean): void {
    this.filtersOpen.set(false);
    if (focusToggle) this.rankingView().focusFiltersToggle();
  }

  /** Un clic hors des filtres ouverts (et hors de leur bouton) les referme. */
  protected onDocumentPointerDown(event: Event): void {
    if (!this.filtersOpen() || this.insideFilters(event.target)) return;
    this.filtersOpen.set(false);
  }

  /** Le focus quitte les filtres ouverts (Tab) : ils se referment. */
  protected onFiltersFocusOut(event: FocusEvent): void {
    if (!this.filtersOpen() || !event.relatedTarget) return;
    if (!this.insideFilters(event.relatedTarget)) this.filtersOpen.set(false);
  }

  private insideFilters(target: EventTarget | null): boolean {
    if (!(target instanceof Node)) return false;
    const host = this.filters().nativeElement;
    const toggle = target instanceof Element && target.closest('[data-testid="filters-toggle"]');
    // Les superpositions (infobulles) sont hors de l'arbre des filtres.
    const overlay = target instanceof Element && target.closest('.cdk-overlay-container');
    return host.contains(target) || !!toggle || !!overlay;
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
