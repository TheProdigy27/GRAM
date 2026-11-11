import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { TestbedHarnessEnvironment } from '@angular/cdk/testing/testbed';
import { MatSortHeaderHarness } from '@angular/material/sort/testing';
import { GIB, makeItem } from '../../../testing/fixtures';
import type { RankItem } from '../../core/generated/RankItem';
import { RankingTable } from './ranking-table';

@Component({
  imports: [RankingTable],
  template: `<app-ranking-table
    label="Test"
    [items]="items()"
    [selected]="selected()"
    (openModel)="opened.push($event)"
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
  readonly selected = signal<string | null>('org/Vision-GGUF');
  readonly opened: string[] = [];
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

describe('RankingTable', () => {
  async function render() {
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    return { fixture, el: fixture.nativeElement as HTMLElement };
  }

  const names = (el: HTMLElement) =>
    [...el.querySelectorAll('[data-testid="model-name"]')].map(text);

  it('affiche les colonnes du classement et le badge de mode coloré', async () => {
    const { el } = await render();
    const rows = el.querySelectorAll('tr.mat-mdc-row');
    expect(rows).toHaveLength(3);
    const cells = [...rows[1].querySelectorAll('td')].map(text);
    expect(cells[0]).toBe('2');
    expect(cells.slice(2)).toEqual(['Q4_K_M', '14,0 Go', 'Hybride 30/40', '~7,4', '40k', '70']);
    expect(rows[1].querySelector('.mode-badge')?.classList).toContain('mode-hybrid');
    expect(rows[0].querySelector('.mode-badge')?.classList).toContain('mode-gpu');
    expect(rows[1].querySelector('.approx')).not.toBeNull();
    expect(rows[2].querySelectorAll('.flag')).toHaveLength(2); // vision + protégé
    // Déjà installé dans Ollama : seulement la première ligne.
    expect(rows[0].querySelector('[data-testid="installed"]')).not.toBeNull();
    expect(el.querySelectorAll('[data-testid="installed"]')).toHaveLength(1);
    expect(rows[2].classList).toContain('selected');
  });

  it('trie au clic sur un en-tête, en gardant le rang du score', async () => {
    const { fixture, el } = await render();
    const loader = TestbedHarnessEnvironment.loader(fixture);
    const tokS = await loader.getHarness(MatSortHeaderHarness.with({ label: 'tok/s' }));

    await tokS.click(); // croissant
    expect(names(el)).toEqual(['Hybride', 'Vision', 'Rapide']);
    await tokS.click(); // décroissant
    expect(names(el)).toEqual(['Rapide', 'Vision', 'Hybride']);
    const ranks = [...el.querySelectorAll('td.col-rank')].map(text);
    expect(ranks).toEqual(['1', '3', '2']);
  });

  it('ouvre la fiche au clic ou avec Entrée', async () => {
    const { fixture, el } = await render();
    const rows = el.querySelectorAll<HTMLElement>('tr.mat-mdc-row');
    rows[0].click();
    rows[1].dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    expect(fixture.componentInstance.opened).toEqual(['org/Rapide-GGUF', 'org/Hybride-GGUF']);
  });
});
