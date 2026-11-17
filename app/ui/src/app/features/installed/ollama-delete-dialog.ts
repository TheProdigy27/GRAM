import { DIALOG_DATA, DialogRef } from '@angular/cdk/dialog';
import { Component, inject } from '@angular/core';
import type { OllamaModel } from '../../core/generated/OllamaModel';
import { fileSize } from '../../shared/format';
import { Button } from '../../shared/ui/button/button';
import { GDialog } from '../../shared/ui/dialog/dialog';

/**
 * Confirmation de « Supprimer » pour un modèle installé sans GRAM Harness : il quitte Ollama,
 * donc toutes les applications qui s'en servent. Répond `true` si la suppression est confirmée.
 */
@Component({
  selector: 'app-ollama-delete-dialog',
  imports: [Button, GDialog],
  template: `
    <g-dialog heading="Supprimer d’Ollama">
      <div class="content">
        <p>
          Supprime <span class="mono">{{ model.name }}</span> d’Ollama et libère jusqu’à {{ size }}.
        </p>
        <p class="muted">
          GRAM Harness et les autres applications qui passent par Ollama ne le proposeront plus : il
          faudra le télécharger de nouveau pour s’en servir.
        </p>
      </div>
      <div gDialogActions>
        <button gButton data-testid="ollama-delete-cancel" (click)="ref.close()">Annuler</button>
        <button
          gButton
          variant="danger"
          data-testid="ollama-delete-confirm"
          (click)="ref.close(true)"
        >
          Supprimer
        </button>
      </div>
    </g-dialog>
  `,
  styles: `
    .content {
      display: flex;
      flex-direction: column;
      gap: 10px;
    }
    p {
      margin: 0;
      line-height: 1.5;
    }
    .muted {
      font-size: 12.5px;
      color: var(--g-muted);
    }
    .mono {
      overflow-wrap: anywhere;
    }
  `,
})
export class OllamaDeleteDialog {
  protected readonly ref = inject<DialogRef<boolean>>(DialogRef);
  protected readonly model = inject<OllamaModel>(DIALOG_DATA);
  protected readonly size = fileSize(this.model.size);
}
