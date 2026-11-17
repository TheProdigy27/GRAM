import { Component, inject } from '@angular/core';
import { ShortcutsService } from '@gram/shortcuts';
import { GDialog } from '@gram/ui/dialog/dialog';
import { type ShortcutGroup, ShortcutGroups } from '@gram/ui/shortcut-groups/shortcut-groups';

/** Raccourcis de GRAM Harness. `mod` met une touche avec Ctrl (⌘ sur Mac). */
export function shortcutGroups(mod: (key: string) => string): ShortcutGroup[] {
  return [
    {
      title: 'Partout',
      rows: [
        { keys: [mod('N')], label: 'Nouvelle conversation' },
        { keys: [mod(',')], label: 'Paramètres' },
        { keys: ['Échap'], label: 'Arrêter la réponse ; fermer le dialogue ou le menu' },
        { keys: ['?'], label: 'Afficher cette aide' },
        { keys: ['R'], label: 'Actualiser la liste des modèles' },
      ],
    },
    {
      title: 'Zone de saisie',
      rows: [
        { keys: ['Entrée'], label: 'Envoyer le message' },
        { keys: ['Maj', 'Entrée'], joiner: '+', label: 'Aller à la ligne' },
      ],
    },
  ];
}

/** Dialogue « Raccourcis clavier » (touche ?), comme dans GRAM. */
@Component({
  selector: 'app-shortcuts-help',
  imports: [GDialog, ShortcutGroups],
  template: `
    <g-dialog heading="Raccourcis clavier">
      <g-shortcut-groups [groups]="groups" />
      <p class="note">
        Les touches seules ne marchent pas pendant une saisie ni sous un dialogue ; {{ modN }},
        {{ modComma }} et Échap, si.
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
  protected readonly modN = this.shortcuts.modShortcut('N');
  protected readonly modComma = this.shortcuts.modShortcut(',');
}
