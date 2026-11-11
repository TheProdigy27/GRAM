import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Icon } from './icon';
import { ICONS, type IconName } from './icons';

@Component({
  imports: [Icon],
  template: `<g-icon [name]="name()" [size]="size()" [label]="label()" />`,
})
class Host {
  readonly name = signal<IconName>('search');
  readonly size = signal(16);
  readonly label = signal('');
}

describe('Icon', () => {
  async function render() {
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    return { fixture, host: el.querySelector('g-icon')!, svg: el.querySelector('svg')! };
  }

  it('rend les tracés SVG du registre, décoratifs par défaut', async () => {
    const { host, svg } = await render();
    const paths = svg.querySelectorAll('path');
    expect(paths).toHaveLength(ICONS.search.length);
    expect(paths[0].namespaceURI).toBe('http://www.w3.org/2000/svg');
    expect(paths[1].getAttribute('d')).toBe('m20 20-3.5-3.5');
    expect(svg.getAttribute('viewBox')).toBe('0 0 24 24');
    expect(svg.getAttribute('width')).toBe('16');
    expect(svg.getAttribute('stroke-width')).toBe('1.8');
    expect(host.getAttribute('aria-hidden')).toBe('true');
    expect(host.getAttribute('role')).toBeNull();
  });

  it('change de tracé et de taille ; avec un libellé, devient une image nommée', async () => {
    const { fixture, host, svg } = await render();
    fixture.componentInstance.name.set('close');
    fixture.componentInstance.size.set(18);
    fixture.componentInstance.label.set('Fermer');
    await fixture.whenStable();
    expect(svg.querySelectorAll('path')).toHaveLength(1);
    expect(svg.getAttribute('height')).toBe('18');
    expect(host.getAttribute('role')).toBe('img');
    expect(host.getAttribute('aria-label')).toBe('Fermer');
    expect(host.getAttribute('aria-hidden')).toBeNull();
  });

  it('registre : une trentaine d’icônes aux tracés valides', () => {
    const entries = Object.entries(ICONS);
    expect(entries.length).toBeGreaterThanOrEqual(35);
    for (const [name, paths] of entries) {
      expect(paths.length, name).toBeGreaterThan(0);
      for (const d of paths) expect(d, name).toMatch(/^[Mm][-\d. a-zA-Z]+$/);
    }
  });
});
