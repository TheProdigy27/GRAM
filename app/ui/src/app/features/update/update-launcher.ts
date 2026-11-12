import { Injectable, PendingTasks, inject } from '@angular/core';
import { DialogService } from '../../shared/ui/dialog/dialog';

/**
 * Ouvre le dialogue « Mise à jour » (bouton de l'en-tête, toast « GRAM X est disponible »,
 * section « Mises à jour » des paramètres). Le dialogue est chargé à la première ouverture ; le
 * chargement compte comme une tâche en cours (l'application n'est pas « stable » avant).
 */
@Injectable({ providedIn: 'root' })
export class UpdateLauncher {
  private readonly dialogs = inject(DialogService);
  private readonly pendingTasks = inject(PendingTasks);

  open(): void {
    this.pendingTasks.run(async () => {
      const { UpdateDialog } = await import('./update-dialog');
      this.dialogs.open(UpdateDialog, { width: '560px' });
    });
  }
}
