import { TestBed } from '@angular/core/testing';
import { Spinner } from './spinner';

describe('Spinner', () => {
  it('purement visuel, à la taille demandée', async () => {
    const fixture = TestBed.createComponent(Spinner);
    fixture.componentRef.setInput('size', 14);
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    expect(el.getAttribute('aria-hidden')).toBe('true');
    expect(el.style.width).toBe('14px');
    expect(el.style.height).toBe('14px');
  });
});
