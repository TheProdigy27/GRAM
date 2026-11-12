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
    [grouped]="grouped()"
    [docked]="docked()"
    [search]="search()"
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
  readonly grouped = signal(false);
  readonly docked = signal(false);
  readonly search = signal<string | null>(null);
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

  it('fiche ouverte : #, Modèle avec quant · taille, Mode sans couches, tok/s, score seul', async () => {
    const { fixture, el, host, rows } = await render();
    host.docked.set(true);
    await fixture.whenStable();
    expect(el.querySelector('app-ranking-table')?.hasAttribute('data-docked')).toBe(true);
    expect(text(byTestId('model-quant-size', rows()[0]))).toBe('Q4_K_M · 4,7 Go');
    expect(cell(rows()[1], 'mode')).toBe('Hybride');
    expect(rows()[0].querySelector('.spark')).toBeNull();
    expect(text(byTestId('score', rows()[0]))).toBe('90');
    // Les lignes restent navigables et ouvrent la fiche.
    await press(fixture, rows()[1], 'Enter');
    expect(host.opened).toEqual(['org/Hybride-GGUF']);
  });

  it('focusModel() : focus sur la ligne d’un modèle affiché', async () => {
    const { host, rows } = await render();
    expect(host.table().focusModel('org/Vision-GGUF')).toBe(true);
    expect(document.activeElement).toBe(rows()[2]);
    expect(host.table().focusModel('org/Absent-GGUF')).toBe(false);
  });

  it('empreinte sur l’échelle mémoire commune : part en VRAM puis part en RAM', async () => {
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

  describe('variantes regroupées', () => {
    const QWEN = 'org/Qwen3-8B-GGUF';

    /** Qwen3-8B, une variante mieux classée que lui et quatre autres plus bas. */
    function family(): RankItem[] {
      return [
        makeItem('Qwen3-8B-abliterated', { rank: 1, score: 91, family: QWEN, tokS: 60 }),
        makeItem('Gemma', { rank: 2, score: 88, tokS: 40 }),
        makeItem('Qwen3-8B', { rank: 3, score: 86, repoId: QWEN, tokS: 95 }),
        ...[4, 5, 6, 7].map((rank) =>
          makeItem(`Qwen3-8B-V${rank}`, { rank, score: 90 - rank, family: QWEN, tokS: rank }),
        ),
      ];
    }

    async function renderGrouped() {
      const r = await render();
      r.host.items.set(family());
      r.host.grouped.set(true);
      r.host.selected.set(null);
      await r.fixture.whenStable();
      const row = (name: string) => r.rows().find((x) => text(byTestId('model-name', x)) === name)!;
      const more = () => r.el.querySelector<HTMLElement>('[data-testid="more-variants"]');
      return { ...r, row, more };
    }

    it('grille arborescente : la famille à la place du modèle officiel, repliée', async () => {
      const { el, rows, row } = await renderGrouped();
      expect(el.querySelector('[role="treegrid"]')).not.toBeNull();
      expect(names(el)).toEqual(['Gemma', 'Qwen3-8B']);
      expect(rows().map((r) => cell(r, 'rank'))).toEqual(['2', '3']);

      const qwen = row('Qwen3-8B');
      expect(qwen.getAttribute('aria-level')).toBe('1');
      expect(qwen.getAttribute('aria-expanded')).toBe('false');
      expect(text(byTestId('variant-count', qwen))).toBe('5 variantes');
      const toggle = byTestId('family-toggle', qwen);
      expect(toggle.getAttribute('aria-label')).toBe('Déplier les variantes de Qwen3-8B');
      expect(toggle.getAttribute('aria-expanded')).toBe('false');

      // Un modèle sans variante : pas de chevron, nom aligné sur ceux des familles.
      const gemma = row('Gemma');
      expect(gemma.hasAttribute('aria-expanded')).toBe(false);
      expect(gemma.querySelector('.lead')).not.toBeNull();
    });

    it('chevron : 3 variantes, « Voir les 2 autres », « mieux noté que l’officiel », sans ouvrir la fiche', async () => {
      const { fixture, el, host, rows, row, more } = await renderGrouped();
      await click(fixture, byTestId('family-toggle', row('Qwen3-8B')));
      expect(host.opened).toEqual([]);
      expect(host.active()).toBe(QWEN);
      expect(document.activeElement).toBe(row('Qwen3-8B'));
      expect(row('Qwen3-8B').getAttribute('aria-expanded')).toBe('true');
      expect(names(el)).toEqual([
        'Gemma',
        'Qwen3-8B',
        'Qwen3-8B-abliterated',
        'Qwen3-8B-V4',
        'Qwen3-8B-V5',
      ]);
      const child = row('Qwen3-8B-abliterated');
      expect(child.getAttribute('aria-level')).toBe('2');
      expect(cell(child, 'rank')).toBe('');
      expect(text(byTestId('better-than-official', child))).toBe('▲ mieux noté que l’officiel');
      expect(allByTestId('better-than-official', el)).toHaveLength(1);
      expect(text(more())).toBe('Voir les 2 autres variantes');
      expect(rows()).toHaveLength(5);

      await click(fixture, more()!);
      expect(more()).toBeNull();
      expect(names(el)).toHaveLength(7);
      expect(document.activeElement).toBe(row('Qwen3-8B-V6'));

      // Replier puis rouvrir : de nouveau 3 variantes.
      await click(fixture, byTestId('family-toggle', row('Qwen3-8B')));
      expect(names(el)).toEqual(['Gemma', 'Qwen3-8B']);
      await click(fixture, byTestId('family-toggle', row('Qwen3-8B')));
      expect(names(el)).toHaveLength(5);
      expect(host.opened).toEqual([]);
    });

    it('clavier : → déplie puis descend, ← remonte puis replie, Entrée sur « Voir les autres »', async () => {
      const { fixture, el, row, more } = await renderGrouped();
      const qwen = () => row('Qwen3-8B');
      qwen().focus();

      let event = await press(fixture, qwen(), 'ArrowRight');
      expect(event.defaultPrevented).toBe(true);
      expect(qwen().getAttribute('aria-expanded')).toBe('true');
      expect(document.activeElement).toBe(qwen());

      await press(fixture, qwen(), 'ArrowRight');
      expect(document.activeElement).toBe(row('Qwen3-8B-abliterated'));
      await press(fixture, row('Qwen3-8B-abliterated'), 'ArrowLeft');
      expect(document.activeElement).toBe(qwen());

      // La ligne « Voir les N autres » se parcourt comme les autres.
      await press(fixture, qwen(), 'End');
      expect(document.activeElement).toBe(more());
      expect(more()!.tabIndex).toBe(0);
      await press(fixture, more()!, 'Enter');
      expect(document.activeElement).toBe(row('Qwen3-8B-V6'));

      await press(fixture, qwen(), 'ArrowLeft');
      expect(qwen().getAttribute('aria-expanded')).toBe('false');
      expect(names(el)).toEqual(['Gemma', 'Qwen3-8B']);
      // Rien à replier ni à déplier : la touche passe.
      event = await press(fixture, qwen(), 'ArrowLeft');
      expect(event.defaultPrevented).toBe(false);
      event = await press(fixture, row('Gemma'), 'ArrowRight');
      expect(event.defaultPrevented).toBe(false);
    });

    it('expand() depuis l’extérieur agit sur la ligne active, sinon la première', async () => {
      const { fixture, host, row } = await renderGrouped();
      expect(host.table().expand(true)).toBe(false); // Gemma n'a pas de variante
      host.active.set(QWEN);
      await fixture.whenStable();
      expect(host.table().expand(true)).toBe(true);
      await fixture.whenStable();
      expect(row('Qwen3-8B').getAttribute('aria-expanded')).toBe('true');
      expect(document.activeElement).toBe(row('Qwen3-8B'));
      expect(host.table().expand(false)).toBe(true);
      await fixture.whenStable();
      expect(row('Qwen3-8B').getAttribute('aria-expanded')).toBe('false');
    });

    it('recherche : les familles sont dépliées avec toutes leurs variantes trouvées', async () => {
      const { fixture, el, host, row, more } = await renderGrouped();
      host.search.set('qwen');
      await fixture.whenStable();
      expect(names(el)).toHaveLength(7);
      expect(more()).toBeNull();

      await click(fixture, byTestId('family-toggle', row('Qwen3-8B')));
      expect(names(el)).toEqual(['Gemma', 'Qwen3-8B']);

      // Nouvelle recherche : les familles se déplient de nouveau ; sans recherche, repliées.
      host.search.set('qwen3');
      await fixture.whenStable();
      expect(names(el)).toHaveLength(7);
      host.search.set(null);
      await fixture.whenStable();
      expect(names(el)).toEqual(['Gemma', 'Qwen3-8B']);
    });

    it('tri : les familles d’après leur ligne de famille', async () => {
      const { fixture, el } = await renderGrouped();
      await click(fixture, 'sort-tokS');
      expect(names(el)).toEqual(['Qwen3-8B', 'Gemma']);
    });
  });
});
