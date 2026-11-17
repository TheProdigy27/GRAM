import { Component, inject } from '@angular/core';
import { ShortcutsService } from '../../core/shortcuts.service';
import { GDialog } from '../../shared/ui/dialog/dialog';
import {
  type ShortcutGroup,
  ShortcutGroups,
} from '../../shared/ui/shortcut-groups/shortcut-groups';

/**
 * Table « Raccourcis » de l'application. `mod` met une touche avec Ctrl (⌘ sur Mac) :
 * « Ctrl F », « ⌘F ».
 */
export function shortcutGroups(mod: (key: string) => string): ShortcutGroup[] {
  return [
    {
      title: 'Partout',
      rows: [
        { keys: [mod('F')], label: 'Chercher un modèle ou une commande' },
        { keys: ['?'], label: 'Afficher cette aide' },
        { keys: ['Échap'], label: 'Fermer la fiche, le dialogue ou la palette' },
        { keys: ['R'], label: 'Ré-analyser le matériel' },
        { keys: ['E'], label: 'Exporter : menu, puis M (Markdown) ou C (CSV)' },
        { keys: ['I'], label: 'Modèles installés dans Ollama' },
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
  imports: [GDialog, ShortcutGroups],
  template: `
    <g-dialog heading="Raccourcis clavier">
      <g-shortcut-groups [groups]="groups" />
      <p class="note">
        Les touches seules ne marchent pas pendant une saisie ni sous un dialogue ; {{ modF }} et
        Échap, si.
      </p>
    </g-dialog>
  `,
  styles: `
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
  protected readonly modF = this.shortcuts.modShortcut('F');
}
