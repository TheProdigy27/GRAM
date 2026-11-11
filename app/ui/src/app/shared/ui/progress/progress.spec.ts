import { TestBed } from '@angular/core/testing';
import { Progress } from './progress';

describe('Progress', () => {
  async function render(inputs: Record<string, unknown>) {
    const fixture = TestBed.createComponent(Progress);
    for (const [k, v] of Object.entries(inputs)) fixture.componentRef.setInput(k, v);
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    return { fixture, el, bar: el.querySelector<HTMLElement>('.g-progress__bar')! };
  }

  it('déterminée : role="progressbar", valeur et largeur', async () => {
    const { el, bar } = await render({ value: 96, max: 150, label: 'Synchronisation' });
    expect(el.getAttribute('role')).toBe('progressbar');
    expect(el.getAttribute('aria-label')).toBe('Synchronisation');
    expect(el.getAttribute('aria-valuemin')).toBe('0');
    expect(el.getAttribute('aria-valuemax')).toBe('150');
    expect(el.getAttribute('aria-valuenow')).toBe('96');
    expect(bar.style.width).toBe('64%');
    expect(bar.classList).not.toContain('g-progress__bar--indeterminate');
  });

  it('bornée entre 0 et le maximum ; texte de valeur', async () => {
    const { el, bar } = await render({ value: 140, valueText: 'presque fini' });
    expect(el.getAttribute('aria-valuenow')).toBe('100');
    expect(el.getAttribute('aria-valuetext')).toBe('presque fini');
    expect(bar.style.width).toBe('100%');
  });

  it('indéterminée : pas de valeur, animation', async () => {
    const { el, bar } = await render({ value: null, size: 'sm' });
    expect(el.hasAttribute('aria-valuenow')).toBe(false);
    expect(el.dataset['size']).toBe('sm');
    expect(bar.classList).toContain('g-progress__bar--indeterminate');
    expect(bar.style.width).toBe('');
  });
});
