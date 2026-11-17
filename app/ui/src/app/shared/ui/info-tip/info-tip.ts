import { Component, input, viewChild } from '@angular/core';
import { Icon } from '../icon/icon';
import { Tooltip, type TooltipPosition } from '../tooltip/tooltip';

/** Un terme et son explication, en une à trois phrases. */
export interface InfoEntry {
  term: string;
  text: string;
}

/**
 * Petit « i » à côté d'un terme technique : `<g-info [entry]="{ term: 'Contexte', text: '…' }" />`.
 * L'explication s'affiche en infobulle au survol, au focus clavier et au clic. Le bouton est
 * nommé « Aide : Contexte » et l'explication lui est reliée par `aria-describedby`.
 *
 * À poser à côté d'un titre, pas dedans : le nom du bouton s'ajouterait à celui du titre.
 * Sans marge : l'espace autour de l'icône (2 px de remplissage) suffit après un mot.
 */
@Component({
  selector: 'g-info',
  imports: [Icon, Tooltip],
  template: `<button
    type="button"
    class="info"
    data-testid="info"
    [attr.aria-label]="'Aide : ' + entry().term"
    [gTooltip]="entry().text"
    [gTooltipPosition]="position()"
    (click)="tooltip().show()"
  >
    <g-icon name="info" [size]="size()" />
  </button>`,
  host: { class: 'g-info' },
  styles: `
    /* Marges négatives : le remplissage du bouton n'agrandit pas la ligne de texte. */
    :host {
      display: inline-flex;
      align-items: center;
      margin-block: -2px;
      vertical-align: middle;
      flex: none;
    }
    .info {
      display: inline-grid;
      place-items: center;
      padding: 2px;
      border: 0;
      border-radius: 50%;
      background: none;
      color: var(--g-muted);
      cursor: help;
    }
    .info:hover {
      color: var(--g-accent-ink);
    }
    .info:focus-visible {
      outline: 2px solid var(--g-accent);
      outline-offset: 0;
    }
  `,
})
export class InfoTip {
  readonly entry = input.required<InfoEntry>();
  readonly position = input<TooltipPosition>('above');
  readonly size = input(14);

  private readonly tooltip = viewChild.required(Tooltip);
}
