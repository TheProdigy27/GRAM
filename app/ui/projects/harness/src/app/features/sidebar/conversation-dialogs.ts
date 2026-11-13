import { DIALOG_DATA, DialogRef } from '@angular/cdk/dialog';
import { Component, ElementRef, afterNextRender, inject, signal, viewChild } from '@angular/core';
import { Button } from '@gram/ui/button/button';
import { GDialog } from '@gram/ui/dialog/dialog';
import { FIELD } from '@gram/ui/field/field';
import type { ConversationSummary } from '@gram/types/ConversationSummary';

/** Longueur maximale d'un titre (comme le backend). */
export const TITLE_MAX = 120;

/** « Renommer » : renvoie le nouveau titre, ou rien si l'utilisateur annule. */
@Component({
  selector: 'app-rename-dialog',
  imports: [Button, GDialog, FIELD],
  template: `
    <g-dialog heading="Renommer la conversation" width="460px">
      <g-field label="Titre">
        <input
          #field
          gInput
          data-testid="rename-input"
          [attr.maxlength]="max"
          [value]="title()"
          (input)="title.set($any($event.target).value)"
          (keydown.enter)="confirm()"
        />
      </g-field>
      <div gDialogActions>
        <button gButton data-testid="rename-cancel" (click)="ref.close()">Annuler</button>
        <button
          gButton
          variant="primary"
          data-testid="rename-confirm"
          [disabled]="!title().trim()"
          (click)="confirm()"
        >
          Renommer
        </button>
      </div>
    </g-dialog>
  `,
})
export class RenameDialog {
  protected readonly ref = inject<DialogRef<string>>(DialogRef);
  protected readonly title = signal(inject<ConversationSummary>(DIALOG_DATA).title);
  protected readonly max = TITLE_MAX;
  private readonly field = viewChild.required<ElementRef<HTMLInputElement>>('field');

  constructor() {
    // Titre sélectionné : il se remplace en tapant.
    afterNextRender(() => this.field().nativeElement.select());
  }

  protected confirm(): void {
    const title = this.title().trim();
    if (title) this.ref.close(title);
  }
}

/** « Supprimer » : renvoie `true` si la suppression est confirmée. */
@Component({
  selector: 'app-delete-dialog',
  imports: [Button, GDialog],
  template: `
    <g-dialog heading="Supprimer la conversation" width="460px">
      <p class="text">
        « {{ conversation.title }} » et tous ses messages seront supprimés définitivement.
      </p>
      <div gDialogActions>
        <button gButton data-testid="delete-cancel" (click)="ref.close()">Annuler</button>
        <button gButton variant="danger" data-testid="delete-confirm" (click)="ref.close(true)">
          Supprimer
        </button>
      </div>
    </g-dialog>
  `,
  styles: `
    .text {
      margin: 0;
      line-height: 1.5;
      overflow-wrap: anywhere;
    }
  `,
})
export class DeleteDialog {
  protected readonly ref = inject<DialogRef<boolean>>(DialogRef);
  protected readonly conversation = inject<ConversationSummary>(DIALOG_DATA);
}
