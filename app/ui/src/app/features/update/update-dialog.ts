import { Component, computed, effect, inject } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatDialogModule, MatDialogRef } from '@angular/material/dialog';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatProgressSpinnerModule } from '@angular/material/progress-spinner';
import { MatSnackBar } from '@angular/material/snack-bar';
import { TauriService, errorMessage } from '../../core/tauri.service';
import { fileSize } from '../../shared/format';
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
 * pendant l'installation ; l'application redémarre à la fin.
 */
@Component({
  selector: 'app-update-dialog',
  imports: [
    MatButtonModule,
    MatDialogModule,
    MatIconModule,
    MatProgressBarModule,
    MatProgressSpinnerModule,
  ],
  templateUrl: './update-dialog.html',
  styleUrl: './update-dialog.scss',
})
export class UpdateDialog {
  protected readonly store = inject(UpdateStore);
  private readonly tauri = inject(TauriService);
  private readonly dialogRef = inject(MatDialogRef<UpdateDialog>);
  private readonly snackBar = inject(MatSnackBar);

  protected readonly notes = computed(() => noteBlocks(this.store.available()?.notes ?? ''));
  protected readonly date = computed(() => releaseDate(this.store.available()?.date ?? null));
  /** « 3 Mo sur 9 Mo », ou « 3 Mo » si la taille est inconnue. */
  protected readonly received = computed(() => {
    const p = this.store.progress();
    if (!p) return null;
    return p.total ? `${fileSize(p.downloaded)} sur ${fileSize(p.total)}` : fileSize(p.downloaded);
  });

  constructor() {
    const status = this.store.status();
    if (status !== 'available' && status !== 'installing') void this.store.check();
    effect(() => {
      this.dialogRef.disableClose = this.store.status() === 'installing';
    });
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
      this.snackBar.open(`Ouverture impossible : ${errorMessage(e)}`, 'OK');
    });
  }
}
