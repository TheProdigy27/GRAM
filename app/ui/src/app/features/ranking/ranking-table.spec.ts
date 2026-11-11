import { Component, signal, viewChild } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { allByTestId, byTestId, click, press, text } from '../../../testing/dom';
import { BUDGET, GIB, makeItem } from '../../../testing/fixtures';
import type { RankItem } from '../../core/generated/RankItem';
import { RankingTable } from './ranking-table';
import { NO_SORT, type SortState } from './sort';

@Component({
  imports: [RankingTable],
  template: `<app-ranking-table
    label="Test"
    [items]="items()"
    [budget]="budget"
    [selected]="selected()"
    [active]="active()"
    [(sort)]="sort"
    (openModel)="opened.push($event)"
    (activeChange)="active.set($event)"
  />`,
})
class Host {
  readonly items = signal<RankItem[]>([
    makeItem('Rapide', { rank: 1, score: 90, tokS: 95, installed: ['Q4_K_M'] }),
    makeItem('Hybride', {
      rank: 2,
      score: 70,
      tokS: 7.4,
      size: 14 * GIB,
      mode: { kind: 'hybrid', gpuLayers: 30, layers: 40 },
      approximate: true,
    }),
    makeItem('Vision', { rank: 3, score: 80, tokS: 40, vision: true, gated: true }),
  ]);
  readonly budget = BUDGET;
  readonly selected = signal<string | null>('org/Vision-GGUF');
  readonly active = signal<string | null>(null);
  readonly sort = signal<SortState>(NO_SORT);
  readonly opened: string[] = [];
  readonly table = viewChild.required(RankingTable);
}

describe('RankingTable', () => {
  async function render() {
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    return {
      fixture,
      el,
      host: fixture.componentInstance,
      rows: () => allByTestId('rank-row', el),
    };
  }

  const names = (el: HTMLElement) => allByTestId('model-name', el).map(text);
  const cell = (row: Element, col: string) => text(row.querySelector(`[data-col="${col}"]`));

  it('grille ARIA : colonnes du classement, badge de mode coloré, repères', async () => {
    const { el, rows } = await render();
    expect(el.querySelector('[role="grid"]')?.getAttribute('aria-label')).toBe('Test');
    const headers = [...el.querySelectorAll('[role="columnheader"]')].map(text);
    expect(headers).toEqual([
      '#',
      'Modèle',
      'Quant',
      'Taille · 0–62 Go',
      'Mode',
      'tok/s',
      'Ctx max',
      'Score',
    ]);

    const [rapide, hybride, vision] = rows();
    expect(rows()).toHaveLength(3);
    expect(cell(hybride, 'rank')).toBe('2');
    expect(cell(hybride, 'quant')).toBe('Q4_K_M');
    expect(cell(hybride, 'mode')).toBe('Hybride 30/40');
    expect(cell(hybride, 'tokS')).toBe('~7,4');
    expect(cell(hybride, 'ctxMax')).toBe('40k');
    expect(text(byTestId('score', hybride))).toBe('70');
    expect(hybride.querySelector('.mode-badge')?.classList).toContain('mode-hybrid');
    expect(rapide.querySelector('.mode-badge')?.classList).toContain('mode-gpu');

    // Sous-ligne : paramètres et dépôt (la quantification n'y apparaît qu'en fenêtre étroite).
    expect(text(vision.querySelector('.model__sub'))).toBe('Q4_K_M · 8,2 Md · org/Vision-GGUF');
    expect(byTestId('flag-approx', hybride).getAttribute('aria-label')).toBe(
      'Estimation approximative',
    );
    expect(byTestId('flag-vision', vision).getAttribute('aria-label')).toBe('Vision');
    expect(byTestId('flag-gated', vision).getAttribute('aria-label')).toBe('Dépôt protégé');
    // Déjà installé dans Ollama : seulement la première ligne.
    expect(allByTestId('installed', el)).toHaveLength(1);
    expect(text(byTestId('installed', rapide))).toBe('Installé');

    expect(vision.getAttribute('aria-selected')).toBe('true');
    expect(rapide.getAttribute('aria-selected')).toBe('false');
  });

  it('empreinte sur l’échelle de la carte mémoire : part en VRAM puis part en RAM', async () => {
    const { rows } = await render();
    const [rapide, hybride] = rows();

    // 4,7 Go de poids + 1,1 Go de cache + 0,5 Go de surcoût, sur 14,3 + 47,3 Go.
    expect(cell(rapide, 'size')).toBe('4,7 Go, 6,3 Go en VRAM');
    const gpu = rapide.querySelector<HTMLElement>('.seg--gpu')!;
    expect(parseFloat(gpu.style.width)).toBeCloseTo((6.3 / 61.6) * 100, 1);
    expect(rapide.querySelector('.seg--ram')).toBeNull();
    const zone = rapide.querySelector<HTMLElement>('.zone')!;
    expect(parseFloat(zone.style.width)).toBeCloseTo((14.3 / 61.6) * 100, 1);

    expect(text(hybride.querySelector('.g-sr-only'))).toBe(', 13,9 Go en VRAM + 1,7 Go en RAM');
    const ram = hybride.querySelector<HTMLElement>('.seg--ram')!;
    // La part en RAM commence au début de la zone RAM.
    expect(parseFloat(ram.style.left)).toBeCloseTo((14.3 / 61.6) * 100, 1);
  });

  it('trie au clic sur un en-tête : sens naturel, inverse, puis ordre du classement', async () => {
    const { fixture, el } = await render();
    const header = () => byTestId('sort-tokS', el).closest('[role="columnheader"]')!;

    await click(fixture, 'sort-tokS'); // le plus rapide d'abord
    expect(names(el)).toEqual(['Rapide', 'Vision', 'Hybride']);
    expect(header().getAttribute('aria-sort')).toBe('descending');
    expect(allByTestId('rank-row', el).map((r) => cell(r, 'rank'))).toEqual(['1', '3', '2']);

    await click(fixture, 'sort-tokS');
    expect(names(el)).toEqual(['Hybride', 'Vision', 'Rapide']);
    expect(header().getAttribute('aria-sort')).toBe('ascending');

    await click(fixture, 'sort-tokS');
    expect(names(el)).toEqual(['Rapide', 'Hybride', 'Vision']);
    expect(header().hasAttribute('aria-sort')).toBe(false);
    expect(fixture.componentInstance.sort()).toEqual(NO_SORT);

    await click(fixture, 'sort-model');
    expect(names(el)).toEqual(['Hybride', 'Rapide', 'Vision']);
  });

  it('ouvre la fiche au clic ou avec Entrée', async () => {
    const { fixture, host, rows } = await render();
    await click(fixture, rows()[0]);
    await press(fixture, rows()[1], 'Enter');
    expect(host.opened).toEqual(['org/Rapide-GGUF', 'org/Hybride-GGUF']);
  });

  it('clavier : une seule ligne tabulable, ↑ ↓ Début Fin la déplacent et la rendent active', async () => {
    const { fixture, host, rows } = await render();
    const tabbable = () => rows().map((r) => r.tabIndex);
    expect(tabbable()).toEqual([0, -1, -1]);

    rows()[0].focus();
    await fixture.whenStable();
    expect(host.active()).toBe('org/Rapide-GGUF');

    await press(fixture, rows()[0], 'ArrowDown');
    expect(document.activeElement).toBe(rows()[1]);
    expect(host.active()).toBe('org/Hybride-GGUF');
    expect(tabbable()).toEqual([-1, 0, -1]);

    await press(fixture, rows()[1], 'End');
    expect(document.activeElement).toBe(rows()[2]);
    await press(fixture, rows()[2], 'ArrowDown'); // reste sur la dernière
    expect(document.activeElement).toBe(rows()[2]);
    await press(fixture, rows()[2], 'Home');
    expect(document.activeElement).toBe(rows()[0]);
    await press(fixture, rows()[0], 'ArrowUp');
    expect(document.activeElement).toBe(rows()[0]);
    expect(host.opened).toEqual([]);
  });

  it('move() depuis l’extérieur : la ligne après l’active, la première sans ligne active', async () => {
    const { fixture, host, rows } = await render();
    expect(host.table().move(1)).toBe(true);
    await fixture.whenStable();
    expect(document.activeElement).toBe(rows()[0]);

    (document.activeElement as HTMLElement).blur();
    host.table().move(1);
    await fixture.whenStable();
    expect(document.activeElement).toBe(rows()[1]);

    host.items.set([]);
    await fixture.whenStable();
    expect(host.table().move(1)).toBe(false);
  });
});
