import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { chooseMenuItem, click, menuItems, openMenu, press, text } from '../../../../testing/dom';
import { MENU } from './menu';

@Component({
  imports: [MENU],
  template: `
    <button type="button" data-testid="export" [cdkMenuTriggerFor]="menu">Exporter</button>
    <ng-template #menu>
      <g-menu aria-label="Exporter">
        <button gMenuItem icon="file" shortcut="M" (triggered)="chosen.push('markdown')">
          Markdown (.md)
        </button>
        <button gMenuItem icon="table" shortcut="C" (triggered)="chosen.push('csv')">
          CSV pour Excel
        </button>
        <button gMenuItem [disabled]="true" (triggered)="chosen.push('pdf')">PDF</button>
      </g-menu>
    </ng-template>
  `,
})
class Host {
  readonly chosen: string[] = [];
}

const menu = () => document.querySelector<HTMLElement>('[role="menu"]');

describe('Menu', () => {
  async function render() {
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('button')!;
    return { fixture, trigger, host: fixture.componentInstance };
  }

  it('s’ouvre au clic : role="menu", éléments, icônes et touches directes', async () => {
    const { fixture, trigger } = await render();
    expect(trigger.getAttribute('aria-haspopup')).toBe('menu');
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    const items = await openMenu(fixture, 'export');
    expect(trigger.getAttribute('aria-expanded')).toBe('true');
    expect(menu()?.getAttribute('aria-label')).toBe('Exporter');
    expect(items.map(text)).toEqual(['Markdown (.md) M', 'CSV pour Excel C', 'PDF']);
    expect(items[0].querySelector('g-icon')?.getAttribute('data-icon')).toBe('file');
    expect(items[0].getAttribute('aria-keyshortcuts')).toBe('M');
    expect(items[2].getAttribute('aria-disabled')).toBe('true');
  });

  it('un clic déclenche l’élément et ferme le menu', async () => {
    const { fixture, host } = await render();
    await chooseMenuItem(fixture, 'export', 'CSV');
    expect(host.chosen).toEqual(['csv']);
    expect(menu()).toBeNull();
  });

  it('clavier : flèches, Entrée, touche directe, Échap rend le focus', async () => {
    const { fixture, trigger, host } = await render();
    trigger.focus();
    await press(fixture, trigger, 'ArrowDown');
    let items = menuItems();
    expect(document.activeElement).toBe(items[0]);
    await press(fixture, items[0], 'ArrowDown');
    expect(document.activeElement).toBe(items[1]);
    await press(fixture, items[1], 'Enter');
    expect(host.chosen).toEqual(['csv']);
    expect(menu()).toBeNull();

    await openMenu(fixture, trigger);
    await press(fixture, menu()!, 'm');
    expect(host.chosen).toEqual(['csv', 'markdown']);
    expect(menu()).toBeNull();

    await click(fixture, trigger);
    items = menuItems();
    items[0].focus();
    await press(fixture, items[0], 'Escape');
    expect(menu()).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it('un élément désactivé ne se déclenche pas', async () => {
    const { fixture, host } = await render();
    const items = await openMenu(fixture, 'export');
    await click(fixture, items[2]);
    expect(host.chosen).toEqual([]);
  });
});
