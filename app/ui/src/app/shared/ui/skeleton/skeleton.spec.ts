import { TestBed } from '@angular/core/testing';
import { Skeleton } from './skeleton';

describe('Skeleton', () => {
  it('bloc masqué aux lecteurs d’écran, aux dimensions demandées', async () => {
    const fixture = TestBed.createComponent(Skeleton);
    fixture.componentRef.setInput('width', '70%');
    fixture.componentRef.setInput('height', '12px');
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    expect(el.getAttribute('aria-hidden')).toBe('true');
    expect(el.style.width).toBe('70%');
    expect(el.style.height).toBe('12px');
    expect(el.style.borderRadius).toBe('4px');
  });
});
