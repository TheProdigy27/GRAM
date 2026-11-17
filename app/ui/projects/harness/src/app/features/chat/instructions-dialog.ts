import { DIALOG_DATA, DialogRef } from '@angular/cdk/dialog';
import { Component, inject, signal } from '@angular/core';
import { Button } from '@gram/ui/button/button';
import { GDialog } from '@gram/ui/dialog/dialog';
import { FIELD } from '@gram/ui/field/field';
import { INSTRUCTIONS_MAX } from '../../shared/conversations';

export interface InstructionsData {
  instructions: string;
  /** Nouvelle conversation, pas encore enregistrée. */
  draft: boolean;
}

/** « Instructions » de la conversation : renvoie le texte (vide : aucune), ou rien si annulé. */
@Component({
  selector: 'app-instructions-dialog',
  imports: [Button, GDialog, FIELD],
  template: `
    <g-dialog heading="Instructions de la conversation" width="560px">
      <p class="intro">
        Envoyées au modèle avant la conversation, à chaque message : rôle, ton, langue, format des
        réponses…
        @if (data.draft) {
          Celles des paramètres sont reprises par chaque nouvelle conversation.
        }
      </p>
      <g-field label="Instructions" clearable clearLabel="Effacer les instructions">
        <textarea
          gInput
          class="text"
          rows="8"
          data-testid="instructions-input"
          placeholder="Par exemple : réponds en français, en quelques phrases."
          [attr.maxlength]="max"
          [value]="text()"
          (input)="text.set($any($event.target).value)"
        ></textarea>
      </g-field>
      <div gDialogActions>
        <button gButton data-testid="instructions-cancel" (click)="ref.close()">Annuler</button>
        <button
          gButton
          variant="primary"
          data-testid="instructions-save"
          (click)="ref.close(text())"
        >
          Enregistrer
        </button>
      </div>
    </g-dialog>
  `,
  styles: `
    .intro {
      margin: 0 0 14px;
      line-height: 1.5;
      color: var(--g-fg-2);
    }
    .text {
      min-height: 140px;
      resize: vertical;
    }
  `,
})
export class InstructionsDialog {
  protected readonly ref = inject<DialogRef<string>>(DialogRef);
  protected readonly data = inject<InstructionsData>(DIALOG_DATA);
  protected readonly text = signal(this.data.instructions);
  protected readonly max = INSTRUCTIONS_MAX;
}
