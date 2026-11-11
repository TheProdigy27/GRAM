import { CdkMenu, CdkMenuItem, CdkMenuTrigger } from '@angular/cdk/menu';
import { Component, contentChildren, inject, input } from '@angular/core';
import { Icon } from '../icon/icon';
import type { IconName } from '../icon/icons';

/**
 * Élément de menu : icône, libellé et touche directe (`shortcut`), sur `CdkMenuItem`.
 * `(triggered)` part au clic, à Entrée ou à la touche directe ; le menu se ferme ensuite.
 */
@Component({
  selector: 'button[gMenuItem]',
  imports: [Icon],
  hostDirectives: [
    {
      directive: CdkMenuItem,
      inputs: ['cdkMenuItemDisabled: disabled'],
      outputs: ['cdkMenuItemTriggered: triggered'],
    },
  ],
  template: `@if (icon(); as name) {
      <g-icon [name]="name" />
    }
    <span class="g-menu-item__label"><ng-content /></span>
    @if (shortcut()) {
      <kbd class="g-kbd" aria-hidden="true">{{ shortcut() }}</kbd>
    }`,
  host: {
    class: 'g-menu-item',
    type: 'button',
    '[attr.aria-keyshortcuts]': 'shortcut() || null',
  },
  styles: `
    :host {
      display: flex;
      align-items: center;
      gap: 8px;
      width: 100%;
      min-height: 34px;
      padding: 0 10px;
      border: 0;
      border-radius: 6px;
      background: transparent;
      color: var(--g-fg-2);
      font: inherit;
      font-size: 13px;
      text-align: left;
      cursor: pointer;
    }
    :host(:hover),
    :host(:focus) {
      background: var(--g-hover);
      color: var(--g-fg);
      outline: none;
    }
    :host(:focus-visible) {
      outline: 2px solid var(--g-accent);
      outline-offset: -2px;
    }
    :host([aria-disabled='true']) {
      color: var(--g-disabled-fg);
      background: transparent;
      cursor: not-allowed;
    }
    .g-menu-item__label {
      flex: 1;
    }
  `,
})
export class MenuItem {
  readonly icon = input<IconName>();
  /** Lettre qui déclenche l'élément quand le menu est ouvert (« M » pour Markdown). */
  readonly shortcut = input('');
  readonly item = inject(CdkMenuItem, { self: true });
}

/**
 * Panneau de menu sur `CdkMenu` (flèches, Début / Fin, recherche par lettre, Échap) :
 *
 * ```html
 * <button gButton [cdkMenuTriggerFor]="exportMenu">Exporter</button>
 * <ng-template #exportMenu>
 *   <g-menu aria-label="Exporter">
 *     <button gMenuItem icon="file" shortcut="M" (triggered)="export('markdown')">Markdown</button>
 *   </g-menu>
 * </ng-template>
 * ```
 */
@Component({
  selector: 'g-menu',
  hostDirectives: [CdkMenu],
  template: '<ng-content />',
  host: { class: 'g-menu', '(keydown)': 'onKeydown($event)' },
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      min-width: 200px;
      padding: 4px;
      border-radius: 10px;
      border: 1px solid var(--g-line);
      background: var(--g-surface);
      box-shadow: var(--g-shadow-pop);
      font-family: var(--g-font-sans);
      outline: none;
    }
  `,
})
export class Menu {
  private readonly items = contentChildren(MenuItem, { descendants: true });

  /** Touche directe d'un élément : le déclenche (et ferme le menu). */
  protected onKeydown(event: KeyboardEvent): void {
    if (event.key.length !== 1 || event.ctrlKey || event.altKey || event.metaKey) return;
    const key = event.key.toLowerCase();
    const match = this.items().find((i) => i.shortcut().toLowerCase() === key && !i.item.disabled);
    if (!match) return;
    event.preventDefault();
    event.stopPropagation();
    match.item.trigger();
  }
}

/** À importer pour un menu : déclencheur CDK, panneau et éléments. */
export const MENU = [CdkMenuTrigger, Menu, MenuItem] as const;
