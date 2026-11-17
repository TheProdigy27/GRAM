import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { allByTestId, click, press, queryTestId, text } from '../../../testing/dom';
import { BUDGET, GIB, makeDetail, makeEstimate } from '../../../testing/fixtures';
import type { VariantDetail } from '../../core/generated/VariantDetail';
import { QuantChart, axisIndices, axisRooms, quantChart } from './quant-chart';

/** Fiche à 4 quantifications : deux en VRAM, une hybride (au-dessus de la taille max), une CPU. */
function variants(): VariantDetail[] {
  const [q4, q8] = makeDetail().variants;
  const hybrid: VariantDetail = {
    ...q8,
    variant: { ...q8.variant, label: 'Q8_K_XL', sizeBytes: 15 * GIB },
    estimate: makeEstimate({
      weights: 15 * GIB,
      mode: { kind: 'hybrid', gpuLayers: 30, layers: 36 },
      tokS: 21,
    }),
    withinSize: false,
  };
  const cpu: VariantDetail = {
    ...q8,
    variant: { ...q8.variant, label: 'BF16', sizeBytes: 16 * GIB },
    estimate: makeEstimate({ weights: 16 * GIB, mode: { kind: 'cpu' }, tokS: 4.2 }),
  };
  return [q4, q8, hybrid, cpu];
}

const sorted = (s: Set<number>) => [...s].sort((a, b) => a - b);

describe('axisIndices', () => {
  it('toutes les quantifications jusqu’à 7, sinon 5 répartis autour de la meilleure', () => {
    expect([...axisIndices(3, 1)]).toEqual([0, 1, 2]);
    expect(sorted(axisIndices(28, 11))).toEqual([0, 6, 11, 19, 27]);
    expect(sorted(axisIndices(28, null))).toEqual([0, 7, 14, 20, 27]);
  });

  it('pas de nom collé à celui de la meilleure : le bout trop proche n’est pas nommé', () => {
    // Avant : 0, 8, 13, 24, 32 (13 à la place de 16), deux noms à 5 colonnes l’un de l’autre.
    expect(sorted(axisIndices(33, 13))).toEqual([0, 7, 13, 23, 32]);
    expect(sorted(axisIndices(33, 2))).toEqual([2, 10, 17, 25, 32]);
    expect(sorted(axisIndices(33, 31))).toEqual([0, 8, 16, 23, 31]);
  });
});

describe('axisRooms', () => {
  it('chaque nom va jusqu’à mi-chemin de ses voisins ; les bouts, calés au bord', () => {
    const labelled = axisIndices(28, null);
    const rooms = axisRooms(labelled, 28);
    expect(sorted(labelled).map((i) => rooms[i])).toEqual([4, 7, 6, 6, 4]);
    expect(rooms.filter((r) => r > 0)).toHaveLength(5);
    // Toutes nommées : une colonne chacune.
    expect(axisRooms(axisIndices(4, 1), 4)).toEqual([1, 1, 1, 1]);
  });
});

describe('quantChart', () => {
  it('hauteurs sur un axe rond, repère au passage de GPU à hybride, légende', () => {
    const m = quantChart(variants(), 1, BUDGET);
    // 95 tok/s au plus : axe 0 → 100.
    expect(m.ticks.map((t) => t.label)).toEqual(['0', '25', '50', '75', '100']);
    expect(m.bars.map((b) => Math.round(b.height))).toEqual([95, 62, 21, 4]);
    expect(m.bars.map((b) => b.mode)).toEqual(['gpu', 'gpu', 'hybrid', 'cpu']);
    expect(m.limit).toEqual({ pct: 50, label: 'Limite VRAM · 14,3 Go', side: 'right' });
    expect(m.modes).toEqual(['gpu', 'hybrid', 'cpu']);
    expect(m.bars.map((b) => b.dimmed)).toEqual([false, false, true, false]);
    expect(m.bars.map((b) => b.axisLabel)).toEqual(['Q4_K_M', '★ Q8_0', 'Q8_K_XL', 'BF16']);
    expect(m.bars[2].tooltip).toBe(
      'Q8_K_XL\n15,0 Go · Hybride (30/36 couches en VRAM)\n~21 tok/s · contexte max 40k · score 86\n' +
        'Au-dessus de la taille maximale demandée',
    );
    expect(m.summary).toBe(
      'Vitesse estimée en tokens par seconde pour 4 quantifications, de Q4_K_M à BF16. ' +
        'Au-delà de Q8_0, le modèle déborde de la VRAM : ~62 puis ~21 tok/s.',
    );
  });

  it('nom de fichier exotique : affiché sans le nom du modèle', () => {
    const vs = variants();
    vs[2] = { ...vs[2], variant: { ...vs[2].variant, label: 'Qwen3-8B-Swift-1.5-IQ2' } };
    const m = quantChart(vs, 1, BUDGET);
    expect(m.bars[2].label).toBe('Qwen3-8B-Swift-1.5-IQ2');
    expect(m.bars[2].name).toBe('Swift-1.5-IQ2');
    expect(m.bars[2].axisLabel).toBe('Swift-1.5-IQ2');
    expect(m.bars[2].tooltip.split('\n')[0]).toBe('Swift-1.5-IQ2');
  });

  it('mémoire unifiée : limite du GPU ; tout sur le CPU : pas de repère', () => {
    const mac = { ...BUDGET, vram: 10.7 * GIB, ram: 12 * GIB, unified: true };
    expect(quantChart(variants(), 1, mac).limit?.label).toBe('Limite du GPU · 10,7 Go');
    const cpuOnly = variants().map((v) => ({
      ...v,
      estimate: { ...v.estimate, mode: { kind: 'cpu' as const } },
    }));
    expect(quantChart(cpuOnly, null, BUDGET).limit).toBeNull();
  });
});

@Component({
  imports: [QuantChart],
  template: `<app-quant-chart
    [variants]="variants"
    [best]="1"
    [budget]="budget"
    [selected]="selected()"
    (selectedChange)="selected.set($event)"
  />`,
})
class Host {
  readonly variants = variants();
  readonly budget = BUDGET;
  readonly selected = signal<string | null>('Q8_0');
}

describe('QuantChart', () => {
  async function render() {
    const fixture = TestBed.createComponent(Host);
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    return {
      fixture,
      el,
      host: fixture.componentInstance,
      bars: () => allByTestId('quant-bar', el),
    };
  }

  it('liste à choix unique : la choisie cerclée avec son nom et sa vitesse', async () => {
    const { el, bars } = await render();
    expect(el.querySelector('[role="listbox"]')?.getAttribute('aria-label')).toBe(
      'Quantifications',
    );
    expect(bars().map((b) => b.getAttribute('aria-selected'))).toEqual([
      'false',
      'true',
      'false',
      'false',
    ]);
    expect(text(el.querySelector('.col__tag'))).toBe('Q8_0 ~62 tok/s');
    expect([...el.querySelectorAll('.legend__item')].map(text)).toEqual([
      'GPU, tient en VRAM',
      'Hybride, déborde en RAM',
      'CPU',
    ]);
    expect(text(queryTestId('vram-limit', el))).toBe('Limite VRAM · 14,3 Go');
  });

  it('clic, ← → Début Fin choisissent ; le focus suit', async () => {
    const { fixture, host, bars } = await render();
    await click(fixture, bars()[2]);
    expect(host.selected()).toBe('Q8_K_XL');

    await press(fixture, bars()[2], 'ArrowLeft');
    expect(host.selected()).toBe('Q8_0');
    expect(document.activeElement).toBe(bars()[1]);
    await press(fixture, bars()[1], 'End');
    expect(host.selected()).toBe('BF16');
    await press(fixture, bars()[3], 'ArrowRight');
    expect(host.selected()).toBe('BF16');
    await press(fixture, bars()[3], 'Home');
    expect(host.selected()).toBe('Q4_K_M');
    expect(bars().map((b) => b.tabIndex)).toEqual([0, -1, -1, -1]);
  });
});
