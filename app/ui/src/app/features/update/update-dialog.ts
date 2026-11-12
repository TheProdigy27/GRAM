import { DialogRef } from '@angular/cdk/dialog';
import { Component, computed, effect, inject } from '@angular/core';
import { TauriService, errorMessage } from '../../core/tauri.service';
import { fileSize } from '../../shared/format';
import { Button } from '../../shared/ui/button/button';
import { Callout } from '../../shared/ui/callout/callout';
import { GDialog } from '../../shared/ui/dialog/dialog';
import { Icon } from '../../shared/ui/icon/icon';
import { Progress } from '../../shared/ui/progress/progress';
import { Spinner } from '../../shared/ui/spinner/spinner';
import { ToastService } from '../../shared/ui/toast/toast';
import { RELEASES_URL, UpdateStore, noteBlocks } from '../../state/update.store';

const longDate = new Intl.DateTimeFormat('fr-FR', { dateStyle: 'long' });

/** Date RFC 3339 → « 11 novembre 2026 » ; `null` si absente ou illisible. */
export function releaseDate(iso: string | null): string | null {
  if (!iso) return null;
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? null : longDate.format(date);
}

/**
 * Mise à jour de GRAM : vérification à l'ouverture (sauf si une mise à jour est déjà connue),
 * notes de la nouvelle version, puis téléchargement et installation. Le dialogue ne se ferme pas
 * pendant l'installation (ni Échap, ni voile, ni ✕) ; l'application redémarre à la fin.
 */
@Component({
  selector: 'app-update-dialog',
  imports: [Button, Callout, GDialog, Icon, Progress, Spinner],
  templateUrl: './update-dialog.html',
  styleUrl: './update-dialog.scss',
})
export class UpdateDialog {
  protected readonly store = inject(UpdateStore);
  private readonly tauri = inject(TauriService);
  protected readonly dialogRef = inject(DialogRef);
  private readonly toast = inject(ToastService);

  protected readonly installing = computed(() => this.store.status() === 'installing');
  protected readonly notes = computed(() => noteBlocks(this.store.available()?.notes ?? ''));
  protected readonly date = computed(() => releaseDate(this.store.available()?.date ?? null));
  /** « 3 Mo sur 9 Mo », ou « 3 Mo » si la taille est inconnue. */
  protected readonly received = computed(() => {
    const p = this.store.progress();
    if (!p) return null;
    return p.total ? `${fileSize(p.downloaded)} sur ${fileSize(p.total)}` : fileSize(p.downloaded);
  });
  /** Avancement affiché : indéterminé tant que la taille est inconnue, puis à l'installation. */
  protected readonly progressValue = computed(() =>
    this.store.downloaded() ? null : this.store.percent(),
  );

  constructor() {
    const status = this.store.status();
    if (status !== 'available' && status !== 'installing') void this.store.check();
    effect(() => {
      this.dialogRef.disableClose = this.installing();
    });
  }

  protected close(): void {
    this.dialogRef.close();
  }

  protected install(): void {
    void this.store.install();
  }

  protected retry(): void {
    if (this.store.available()) void this.store.install();
    else void this.store.check();
  }

  protected openReleases(): void {
    this.tauri.openUrl(RELEASES_URL).catch((e: unknown) => {
      this.toast.show(`Ouverture impossible : ${errorMessage(e)}`, { kind: 'error' });
    });
  }
}
