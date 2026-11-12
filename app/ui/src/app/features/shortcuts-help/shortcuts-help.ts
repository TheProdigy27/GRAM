import { Component, inject } from '@angular/core';
import { ShortcutsService } from '../../core/shortcuts.service';
import { GDialog } from '../../shared/ui/dialog/dialog';

/** Une ligne de l'aide : touches (reliées par `joiner`, « à »…) et ce qu'elles font. */
export interface ShortcutRow {
  keys: readonly string[];
  joiner?: string;
  label: string;
}

export interface ShortcutGroup {
  title: string;
  rows: readonly ShortcutRow[];
}

/**
 * Table « Raccourcis » de l'application. `mod` met une touche avec Ctrl (⌘ sur Mac) :
 * « Ctrl K », « ⌘K ».
 */
export function shortcutGroups(mod: (key: string) => string): ShortcutGroup[] {
  return [
    {
      title: 'Partout',
      rows: [
        { keys: [mod('K')], label: 'Chercher un modèle ou une commande' },
        { keys: ['?'], label: 'Afficher cette aide' },
        { keys: ['Échap'], label: 'Fermer la fiche, le dialogue ou la palette' },
        { keys: ['R'], label: 'Ré-analyser le matériel' },
        { keys: ['E'], label: 'Exporter : menu, puis M (Markdown) ou C (CSV)' },
      ],
    },
    {
      title: 'Classement',
      rows: [
        {
          keys: ['1', '5'],
          joiner: 'à',
          label: 'Usage : polyvalent, code, raisonnement, chat rapide, vision',
        },
        { keys: ['↑', '↓'], label: 'Ligne précédente ou suivante' },
        { keys: ['Entrée'], label: 'Ouvrir la fiche de la ligne' },
        { keys: ['←', '→'], label: 'Replier ou déplier une famille de variantes' },
      ],
    },
    {
      title: 'Palette',
      rows: [
        { keys: ['↑', '↓'], label: 'Choisir un résultat' },
        { keys: ['Entrée'], label: 'Ouvrir le modèle ou lancer la commande' },
        { keys: [mod('Entrée')], label: 'Filtrer le classement sur la saisie' },
      ],
    },
    {
      title: 'Fiche',
      rows: [
        {
          keys: ['←', '→'],
          label: 'Quantification précédente ou suivante (graphique des vitesses)',
        },
        {
          keys: ['Début', 'Fin'],
          joiner: 'ou',
          label: 'Plus petite ou plus grosse quantification',
        },
      ],
    },
  ];
}

/** Dialogue « Raccourcis clavier » (touche ?, palette). */
@Component({
  selector: 'app-shortcuts-help',
  imports: [GDialog],
  template: `
    <g-dialog heading="Raccourcis clavier">
      <div class="groups">
        @for (g of groups; track g.title) {
          <section class="group" [attr.aria-labelledby]="'shortcuts-' + $index">
            <h3 class="group__title" [id]="'shortcuts-' + $index">{{ g.title }}</h3>
            <dl class="rows">
              @for (r of g.rows; track r.label) {
                <div class="row" data-testid="shortcut-row">
                  <dt class="keys">
                    @for (k of r.keys; track $index; let last = $last) {
                      <kbd class="g-kbd">{{ k }}</kbd>
                      @if (r.joiner && !last) {
                        <span class="joiner"> {{ r.joiner }} </span>
                      }
                    }
                  </dt>
                  <dd class="label">{{ r.label }}</dd>
                </div>
              }
            </dl>
          </section>
        }
      </div>
      <p class="note">
        Les touches seules ne marchent pas pendant une saisie ni sous un dialogue ; {{ modK }} et
        Échap, si.
      </p>
    </g-dialog>
  `,
  styles: `
    .groups {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
      gap: 20px 28px;
    }
    .group__title {
      margin: 0 0 6px;
      font-size: 11px;
      font-weight: 600;
      letter-spacing: 0.06em;
      text-transform: uppercase;
      color: var(--g-muted);
    }
    .rows {
      margin: 0;
    }
    .row {
      display: flex;
      align-items: baseline;
      gap: 12px;
      padding: 6px 0;
      border-bottom: 1px solid var(--g-line-2);
    }
    .keys {
      flex: none;
      display: flex;
      align-items: center;
      gap: 4px;
      width: 96px;
    }
    .joiner {
      font-size: 12px;
      color: var(--g-muted);
    }
    .label {
      margin: 0;
      color: var(--g-fg-2);
    }
    .note {
      margin: 16px 0 0;
      font-size: 11.5px;
      color: var(--g-muted);
    }
  `,
})
export class ShortcutsHelp {
  private readonly shortcuts = inject(ShortcutsService);
  protected readonly groups = shortcutGroups((key) => this.shortcuts.modShortcut(key));
  protected readonly modK = this.shortcuts.modShortcut('K');
}
