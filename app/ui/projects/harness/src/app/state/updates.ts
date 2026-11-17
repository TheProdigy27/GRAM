import { Injectable, PendingTasks, inject } from '@angular/core';
import { DialogService } from '@gram/ui/dialog/dialog';
import { ToastService } from '@gram/ui/toast/toast';
import { UPDATED_APP, UPDATE_CHECK_DELAY_MS, UpdateStore } from '@gram/updates';

/** Durée du toast « GRAM Harness X est disponible » (comme dans GRAM). */
export const UPDATE_TOAST_MS = 10000;

/**
 * Mises à jour de GRAM Harness, avec le magasin et le dialogue de GRAM (`@gram/updates`,
 * `@gram/update-dialog`) : le backend a les mêmes commandes (crate `gram-update`), qui lisent
 * `latest-harness.json`. Vérification automatique peu après le lancement (un échec est ignoré),
 * signalée par un toast ; dialogue chargé à la demande.
 */
@Injectable({ providedIn: 'root' })
export class Updates {
  readonly store = inject(UpdateStore);
  private readonly app = inject(UPDATED_APP);
  private readonly delay = inject(UPDATE_CHECK_DELAY_MS);
  private readonly dialogs = inject(DialogService);
  private readonly pendingTasks = inject(PendingTasks);
  private readonly toast = inject(ToastService);

  /** Lance la vérification automatique ; renvoie de quoi l'annuler (destruction de la coque). */
  start(): () => void {
    const timer = setTimeout(() => void this.checkAtLaunch(), this.delay);
    return () => clearTimeout(timer);
  }

  /** Dialogue « Mise à jour de GRAM Harness » (chargé à la première ouverture). */
  open(): void {
    this.pendingTasks.run(async () => {
      const { UpdateDialog } = await import('@gram/update-dialog');
      this.dialogs.open(UpdateDialog, { width: '560px' });
    });
  }

  private async checkAtLaunch(): Promise<void> {
    const update = await this.store.check(true);
    if (!update) return;
    this.toast
      .show(`${this.app.name} ${update.version} est disponible.`, {
        action: 'Voir',
        duration: UPDATE_TOAST_MS,
      })
      .onAction()
      .subscribe(() => this.open());
  }
}
