import { Dialog as CdkDialog, type DialogConfig, DialogRef } from '@angular/cdk/dialog';
import type { ComponentType } from '@angular/cdk/portal';
import {
  Component,
  Injectable,
  ViewEncapsulation,
  booleanAttribute,
  inject,
  input,
} from '@angular/core';
import { Button } from '../button/button';
import { Icon } from '../icon/icon';

let nextId = 0;

/**
 * Ouvre un dialogue CDK avec les réglages de GRAM : voile, largeur bornée à la fenêtre, focus sur
 * le titre puis rendu à l'élément d'origine, Échap ferme, titre relié par `aria-labelledby`.
 */
@Injectable({ providedIn: 'root' })
export class DialogService {
  private readonly dialog = inject(CdkDialog);

  open<R = unknown, D = unknown, C = unknown>(
    component: ComponentType<C>,
    config: DialogConfig<D, DialogRef<R, C>> = {},
  ): DialogRef<R, C> {
    const id = config.id ?? `g-dialog-${nextId++}`;
    return this.dialog.open<R, D, C>(component, {
      ariaLabelledBy: `${id}-title`,
      ariaModal: true,
      autoFocus: 'first-heading',
      backdropClass: 'g-backdrop',
      maxWidth: 'calc(100vw - 32px)',
      maxHeight: 'calc(100vh - 32px)',
      ...config,
      id,
    });
  }
}

/**
 * Coque d'un dialogue : titre, en-tête complémentaire (`[gDialogHeader]`), contenu qui défile,
 * pied d'actions (`[gDialogActions]`, sur toute la largeur : un élément `flex: 1` y sépare les
 * actions de gauche de celles de droite) et bouton Fermer.
 *
 * ```html
 * <g-dialog heading="Paramètres" width="1040px">
 *   …contenu…
 *   <div gDialogActions><button gButton (click)="ref.close()">Annuler</button></div>
 * </g-dialog>
 * ```
 */
@Component({
  selector: 'g-dialog',
  imports: [Button, Icon],
  template: `
    <header class="g-dialog__header">
      <h2 class="g-dialog__title" [id]="titleId">{{ heading() }}</h2>
      <ng-content select="[gDialogHeader]" />
      @if (closable()) {
        <button
          gButton
          variant="ghost"
          icon
          class="g-dialog__close"
          aria-label="Fermer"
          data-testid="dialog-close"
          (click)="close()"
        >
          <g-icon name="close" [size]="18" />
        </button>
      }
    </header>
    <div class="g-dialog__body"><ng-content /></div>
    <footer class="g-dialog__actions"><ng-content select="[gDialogActions]" /></footer>
  `,
  host: {
    class: 'g-dialog',
    '[class.g-dialog--flush]': 'flush()',
    '[style.width]': 'width()',
  },
  // Sans encapsulation pour espacer les actions projetées. Tout est préfixé.
  encapsulation: ViewEncapsulation.None,
  styles: `
    .g-dialog {
      display: flex;
      flex-direction: column;
      max-width: 100%;
      max-height: inherit;
      border-radius: var(--g-radius-dialog);
      background: var(--g-surface);
      color: var(--g-fg);
      box-shadow: var(--g-shadow-dialog);
      font-family: var(--g-font-sans);
      font-size: 13px;
      overflow: hidden;
    }
    .g-dialog__header {
      display: flex;
      align-items: center;
      gap: 12px;
      padding: 14px 16px 14px 20px;
      border-bottom: 1px solid var(--g-line-2);
    }
    .g-dialog__title {
      flex: 1;
      margin: 0;
      font-size: 17px;
      font-weight: 700;
      outline: none;
    }
    // Place de la barre de défilement réservée, et retirée de la marge droite (styles.scss).
    .g-dialog__body {
      flex: 1 1 auto;
      min-height: 0;
      padding: 20px calc(20px - var(--g-scrollbar-size)) 20px 20px;
      overflow-x: auto;
      overflow-y: scroll;
    }
    // Sans marge, le contenu a ses propres zones qui défilent.
    .g-dialog--flush > .g-dialog__body {
      padding: 0;
      overflow-y: auto;
    }
    .g-dialog__actions {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      justify-content: flex-end;
      gap: 10px;
      padding: 14px 20px;
      border-top: 1px solid var(--g-line-2);
      background: var(--g-surface-2);
    }
    .g-dialog__actions:empty {
      display: none;
    }
    .g-dialog__actions > [gDialogActions] {
      flex: 1;
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      justify-content: flex-end;
      gap: 10px;
    }
  `,
})
export class GDialog {
  readonly heading = input.required<string>();
  /** Largeur souhaitée (« 520px ») ; le dialogue reste borné à la fenêtre. */
  readonly width = input<string | null>(null);
  readonly closable = input(true, { transform: booleanAttribute });
  /** Contenu sans marge intérieure (navigation latérale des paramètres). */
  readonly flush = input(false, { transform: booleanAttribute });

  private readonly ref = inject(DialogRef, { optional: true });
  protected readonly titleId = this.ref?.config.ariaLabelledBy ?? `g-dialog-${nextId++}-title`;

  close(): void {
    this.ref?.close();
  }
}
