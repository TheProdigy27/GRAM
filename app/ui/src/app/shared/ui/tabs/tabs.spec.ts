import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { chooseOption, press, text } from '../../../../testing/dom';
import { type TabItem, Tabs } from './tabs';

@Component({
  imports: [Tabs],
  template: `
    <g-tabs
      #tabs
      data-testid="sections"
      label="Sections du classement"
      idPrefix="rank"
      [tabs]="items"
      [(selected)]="section"
    />
    <div
      role="tabpanel"
      [id]="tabs.panelId(section())"
      [attr.aria-labelledby]="tabs.tabId(section())"
    >
      {{ section() }}
    </div>
  `,
})
class Host {
  readonly items: TabItem[] = [
    { value: 'runnable', label: 'Exécutables', count: 1165 },
    { value: 'slow', label: 'Lents', count: 12 },
    { value: 'tooBig', label: 'Trop gros', count: '3' },
  ];
  readonly section = signal('runnable');
}

describe('Tabs', () => {
  async function render() {
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    const tabs = [...el.querySelectorAll<HTMLButtonElement>('[role="tab"]')];
    return { fixture, el, tabs, host: fixture.componentInstance };
  }

  it('liste d’onglets nommée, compteurs, panneau relié', async () => {
    const { el, tabs } = await render();
    const list = el.querySelector('g-tabs')!;
    expect(list.getAttribute('role')).toBe('tablist');
    expect(list.getAttribute('aria-label')).toBe('Sections du classement');
    expect(tabs.map(text)).toEqual(['Exécutables 1 165', 'Lents 12', 'Trop gros 3']);
    expect(tabs.map((t) => t.getAttribute('aria-selected'))).toEqual(['true', 'false', 'false']);
    expect(tabs.map((t) => t.tabIndex)).toEqual([0, -1, -1]);
    const panel = el.querySelector('[role="tabpanel"]')!;
    expect(tabs[0].id).toBe('rank-tab-runnable');
    expect(tabs[0].getAttribute('aria-controls')).toBe(panel.id);
    expect(panel.getAttribute('aria-labelledby')).toBe(tabs[0].id);
  });

  it('clic sur un onglet', async () => {
    const { fixture, tabs, host } = await render();
    await chooseOption(fixture, 'sections', 'Lents');
    expect(host.section()).toBe('slow');
    expect(tabs[1].getAttribute('aria-selected')).toBe('true');
    expect(tabs.map((t) => t.tabIndex)).toEqual([-1, 0, -1]);
  });

  it('← → en boucle, Début et Fin : sélection et focus suivent', async () => {
    const { fixture, tabs, host } = await render();
    tabs[0].focus();
    await press(fixture, tabs[0], 'ArrowLeft');
    expect(host.section()).toBe('tooBig');
    expect(document.activeElement).toBe(tabs[2]);
    await press(fixture, tabs[2], 'ArrowRight');
    expect(host.section()).toBe('runnable');
    await press(fixture, tabs[0], 'End');
    expect(host.section()).toBe('tooBig');
    await press(fixture, tabs[2], 'Home');
    expect(host.section()).toBe('runnable');
    expect(document.activeElement).toBe(tabs[0]);
  });
});
