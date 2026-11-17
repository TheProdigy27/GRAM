import { NgTemplateOutlet } from '@angular/common';
import {
  Component,
  ElementRef,
  Injector,
  PendingTasks,
  afterNextRender,
  computed,
  inject,
  signal,
} from '@angular/core';
import type { HarnessEntry } from '../../core/generated/HarnessEntry';
import type { OllamaModel } from '../../core/generated/OllamaModel';
import { ctxK, fileSize } from '../../shared/format';
import { type HarnessRow, type OtherRow, modelCount, sameOllamaName } from '../../shared/installed';
import { Button } from '../../shared/ui/button/button';
import { DialogService, GDialog } from '../../shared/ui/dialog/dialog';
import { EmptyState } from '../../shared/ui/empty-state/empty-state';
import { Icon } from '../../shared/ui/icon/icon';
import { Skeleton } from '../../shared/ui/skeleton/skeleton';
import { HardwareStore } from '../../state/hardware.store';
import { HarnessStore } from '../../state/harness.store';
import type { RemoveChoice } from '../model-detail/harness-remove-dialog';

/** Ce que montre le dialogue. */
type ListState = 'loading' | 'absent' | 'stopped' | 'empty' | 'list';

/**
 * « Modèles installés » : les modèles d'Ollama, relus à l'ouverture et par « Actualiser ».
 * D'abord ceux ajoutés à GRAM Harness (nom lisible, modèle dérivé et contexte), avec
 * « Retirer » et son dialogue habituel (la base peut partir avec, si GRAM l'a téléchargée) ;
 * puis les autres, avec « Supprimer » après confirmation. Une suppression à la fois ; la ligne
 * disparaît dès qu'Ollama a répondu, et le focus passe au bouton de la ligne suivante.
 */
@Component({
  selector: 'app-installed-dialog',
  imports: [Button, EmptyState, GDialog, Icon, NgTemplateOutlet, Skeleton],
  templateUrl: './installed-dialog.html',
  styleUrl: './installed-dialog.scss',
})
export class InstalledDialog {
  protected readonly store = inject(HarnessStore);
  private readonly hardware = inject(HardwareStore);
  private readonly dialogs = inject(DialogService);
  private readonly pendingTasks = inject(PendingTasks);
  private readonly injector = inject(Injector);
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef);

  protected readonly refreshing = signal(false);
  protected readonly installed = this.store.installed;

  protected readonly state = computed<ListState>(() => {
    const r = this.hardware.runtimes();
    if (!r) return 'loading';
    if (r.ollama.running) return this.installed().count > 0 ? 'list' : 'empty';
    return r.ollama.installed ? 'stopped' : 'absent';
  });

  /** « 4 modèles · 12,7 Go ». */
  protected readonly summary = computed(() => {
    const i = this.installed();
    return `${modelCount(i.count)} · ${fileSize(i.size)}`;
  });

  constructor() {
    // Des modèles ont pu être ajoutés ou supprimés hors de GRAM (`ollama pull`, `ollama rm`).
    void this.refresh();
  }

  protected size(bytes: number): string {
    return fileSize(bytes);
  }

  protected ctx(tokens: number): string {
    return ctxK(tokens);
  }

  /** Une autre suppression est en cours : les autres boutons attendent. */
  protected busyElsewhere(name: string): boolean {
    const removing = this.store.removing();
    return removing !== null && !sameOllamaName(removing, name);
  }

  protected removingThis(name: string): boolean {
    const removing = this.store.removing();
    return removing !== null && sameOllamaName(removing, name);
  }

  /** Le modèle dérivé est en train d'être refait (« Régler le contexte ») : pas de retrait. */
  protected installing(row: HarnessRow): boolean {
    const install = this.store.install();
    return !!install && sameOllamaName(install.ollamaName, row.entry.model.ollamaName);
  }

  protected async refresh(): Promise<void> {
    if (this.refreshing()) return;
    this.refreshing.set(true);
    try {
      await this.hardware.refreshRuntimes();
    } finally {
      this.refreshing.set(false);
    }
  }

  /** « Retirer » : le dialogue de la fiche, qui propose la base quand c'est permis. */
  protected remove(row: HarnessRow, event: Event): void {
    const origin = event.currentTarget as HTMLElement;
    const index = this.actionIndex(origin);
    this.pendingTasks.run(async () => {
      const { HarnessRemoveDialog } = await import('../model-detail/harness-remove-dialog');
      const ref = this.dialogs.open<RemoveChoice, HarnessEntry>(HarnessRemoveDialog, {
        data: row.entry,
        width: '480px',
        restoreFocus: origin,
      });
      ref.closed.subscribe((choice) => {
        if (!choice) return;
        void this.store
          .remove(row.entry.model, choice.removeBase)
          .then((r) => r && this.focusAfterRemoval(index));
      });
    });
  }

  /** « Supprimer » : confirmation, puis suppression d'Ollama. */
  protected delete(row: OtherRow, event: Event): void {
    const origin = event.currentTarget as HTMLElement;
    const index = this.actionIndex(origin);
    this.pendingTasks.run(async () => {
      const { OllamaDeleteDialog } = await import('./ollama-delete-dialog');
      const ref = this.dialogs.open<boolean, OllamaModel>(OllamaDeleteDialog, {
        data: row.model,
        width: '480px',
        restoreFocus: origin,
      });
      ref.closed.subscribe((confirmed) => {
        if (!confirmed) return;
        void this.store.deleteModel(row.model).then((ok) => ok && this.focusAfterRemoval(index));
      });
    });
  }

  private actions(): HTMLElement[] {
    return [...this.host.nativeElement.querySelectorAll<HTMLElement>('[data-row-action]')];
  }

  private actionIndex(button: HTMLElement): number {
    return Math.max(0, this.actions().indexOf(button));
  }

  /**
   * La ligne supprimée a emporté le bouton qui avait le focus : il passe au bouton de la ligne
   * qui a pris sa place (ou de la dernière), sinon au titre du dialogue.
   */
  private focusAfterRemoval(index: number): void {
    afterNextRender(
      () => {
        const active = document.activeElement;
        if (active && active !== document.body) return;
        const actions = this.actions().filter((b) => !b.hasAttribute('disabled'));
        const next = actions[Math.min(index, actions.length - 1)];
        const title = this.host.nativeElement.querySelector<HTMLElement>('.g-dialog__title');
        const target = next ?? title;
        if (target === title && title) title.tabIndex = -1;
        target?.focus();
      },
      { injector: this.injector },
    );
  }
}
