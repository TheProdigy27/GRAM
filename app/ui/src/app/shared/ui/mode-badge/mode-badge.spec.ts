import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { text } from '../../../../testing/dom';
import type { RunMode } from '../../../core/generated/RunMode';
import { ModeBadge } from './mode-badge';

@Component({
  imports: [ModeBadge],
  template: `<app-mode-badge [mode]="mode()" [tooltip]="tooltip()" />`,
})
class Host {
  readonly mode = signal<RunMode>({ kind: 'gpu' });
  readonly tooltip = signal('');
}

describe('ModeBadge', () => {
  async function render() {
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    return {
      fixture,
      host: fixture.componentInstance,
      badge: () => el.querySelector('.mode-badge')!,
    };
  }

  it('repère coloré et libellé, classe du mode (API inchangée)', async () => {
    const { fixture, host, badge } = await render();
    expect(text(badge())).toBe('GPU');
    expect(badge().classList).toContain('mode-gpu');
    expect(badge().querySelector('.mode-badge__mark')).not.toBeNull();
    expect(badge().hasAttribute('aria-describedby')).toBe(false);
    host.mode.set({ kind: 'cpu' });
    await fixture.whenStable();
    expect(text(badge())).toBe('CPU');
    expect(badge().classList).toContain('mode-cpu');
    expect(badge().classList).not.toContain('mode-gpu');
  });

  it('hybride : couches en VRAM affichées et détaillées en infobulle', async () => {
    const { fixture, host, badge } = await render();
    host.mode.set({ kind: 'hybrid', gpuLayers: 62, layers: 65 });
    await fixture.whenStable();
    expect(text(badge())).toBe('Hybride 62/65');
    expect(badge().classList).toContain('mode-hybrid');
    const description = document.getElementById(badge().getAttribute('aria-describedby')!);
    expect(description?.textContent).toBe('62 couches sur 65 en VRAM');
    host.tooltip.set('Tient en partie en VRAM');
    await fixture.whenStable();
    const custom = document.getElementById(badge().getAttribute('aria-describedby')!);
    expect(custom?.textContent).toBe('Tient en partie en VRAM');
  });

  it('trop gros : croix et libellé', async () => {
    const { fixture, host, badge } = await render();
    host.mode.set({ kind: 'tooBig' });
    await fixture.whenStable();
    expect(text(badge())).toBe('Trop gros');
    expect(badge().classList).toContain('mode-tooBig');
    expect(badge().querySelector('g-icon')?.getAttribute('data-icon')).toBe('close');
  });
});
