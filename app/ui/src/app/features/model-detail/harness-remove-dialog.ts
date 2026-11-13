import { DIALOG_DATA, DialogRef } from '@angular/cdk/dialog';
import { Component, computed, inject, signal } from '@angular/core';
import type { HarnessEntry } from '../../core/generated/HarnessEntry';
import { fileSize } from '../../shared/format';
import { Button } from '../../shared/ui/button/button';
import { Checkbox } from '../../shared/ui/checkbox/checkbox';
import { GDialog } from '../../shared/ui/dialog/dialog';

/** Réponse du dialogue quand le retrait est confirmé. */
export interface RemoveChoice {
  removeBase: boolean;
}

/**
 * Confirmation de « Retirer » : le modèle dérivé est supprimé d'Ollama. La suppression de la
 * base n'est proposée (décochée) que si GRAM l'a téléchargée et qu'aucun autre modèle ajouté ne
 * l'utilise ; sinon, le dialogue dit pourquoi elle reste.
 */
@Component({
  selector: 'app-harness-remove-dialog',
  imports: [Button, Checkbox, GDialog],
  template: `
    <g-dialog heading="Retirer de GRAM Harness">
      <div class="content">
        <p>
          Supprime <span class="mono">{{ model.ollamaName }}</span> d’Ollama : GRAM Harness ne le
          proposera plus.
        </p>
        @if (entry.baseRemovable) {
          <g-checkbox
            data-testid="harness-remove-base"
            [checked]="removeBase()"
            (checkedChange)="removeBase.set($event)"
            >Supprimer aussi le modèle d’Ollama ({{ size }})</g-checkbox
          >
          <p class="muted">
            <span class="mono">{{ model.baseName }}</span
            >, téléchargé par GRAM. Sans lui, un nouvel ajout le téléchargera à nouveau.
          </p>
        } @else {
          <p class="muted" data-testid="harness-base-kept">{{ kept() }}</p>
        }
      </div>
      <div gDialogActions>
        <button gButton data-testid="harness-remove-cancel" (click)="ref.close()">Annuler</button>
        <button
          gButton
          variant="danger"
          data-testid="harness-remove-confirm"
          (click)="ref.close({ removeBase: removeBase() })"
        >
          Retirer
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
export class HarnessRemoveDialog {
  protected readonly ref = inject<DialogRef<RemoveChoice>>(DialogRef);
  protected readonly entry = inject<HarnessEntry>(DIALOG_DATA);
  protected readonly model = this.entry.model;
  protected readonly size = fileSize(this.model.size);
  protected readonly removeBase = signal(false);

  /** Pourquoi la base reste dans Ollama. */
  protected readonly kept = computed(() => {
    const base = this.model.baseName;
    return this.model.baseInstalledByGram
      ? `${base} reste dans Ollama : un autre modèle ajouté à GRAM Harness l’utilise.`
      : `${base} reste dans Ollama : il y était avant l’ajout.`;
  });
}
