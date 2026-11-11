import { ApplicationRef } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { ToastService } from './toast';

describe('ToastService', () => {
  const stable = () => TestBed.inject(ApplicationRef).whenStable();
  const region = () => document.querySelector<HTMLElement>('g-toast-region')!;
  const toast = () => region().querySelector<HTMLElement>('.g-toast');

  afterEach(() => vi.useRealTimers());

  it('région polie présente dès le départ, message et action', async () => {
    const service = TestBed.inject(ToastService);
    await stable();
    expect(region().getAttribute('role')).toBe('status');
    expect(region().getAttribute('aria-live')).toBe('polite');
    expect(toast()).toBeNull();

    const ref = service.show('Commande copiée', { action: 'Annuler', kind: 'success' });
    const actions: unknown[] = [];
    const dismissals: unknown[] = [];
    ref.onAction().subscribe(() => actions.push(true));
    ref.afterDismissed().subscribe((d) => dismissals.push(d));
    await stable();
    expect(toast()?.textContent).toContain('Commande copiée');
    expect(toast()?.dataset['kind']).toBe('success');
    const action = region().querySelector<HTMLButtonElement>('[data-testid="toast-action"]')!;
    expect(action.textContent?.trim()).toBe('Annuler');

    action.click();
    await stable();
    expect(actions).toHaveLength(1);
    expect(dismissals).toEqual([{ dismissedByAction: true }]);
    expect(toast()).toBeNull();
  });

  it('disparaît après 4 s, minuteur suspendu au survol', async () => {
    const service = TestBed.inject(ToastService);
    vi.useFakeTimers();
    const ref = service.show('Classement exporté');
    const dismissals: unknown[] = [];
    ref.afterDismissed().subscribe((d) => dismissals.push(d));
    vi.advanceTimersByTime(3000);
    service.pause();
    vi.advanceTimersByTime(5000);
    expect(service.current()).toBe(ref);
    service.resume();
    vi.advanceTimersByTime(999);
    expect(service.current()).toBe(ref);
    vi.advanceTimersByTime(1);
    expect(service.current()).toBeNull();
    expect(dismissals).toEqual([{ dismissedByAction: false }]);
  });

  it('un nouveau message remplace l’ancien ; durée 0 : reste affiché', async () => {
    const service = TestBed.inject(ToastService);
    vi.useFakeTimers();
    const first = service.show('Premier');
    let firstDismissed = false;
    first.afterDismissed().subscribe(() => (firstDismissed = true));
    const second = service.show('Analyse impossible', { kind: 'error', duration: 0 });
    expect(firstDismissed).toBe(true);
    expect(service.current()).toBe(second);
    vi.advanceTimersByTime(60_000);
    expect(service.current()).toBe(second);
    vi.useRealTimers();
    await stable();
    expect(toast()?.textContent).toContain('Analyse impossible');
    second.dismiss();
    await stable();
    expect(toast()).toBeNull();
  });
});
