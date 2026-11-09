import { TestBed } from '@angular/core/testing';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { App } from './app';
import type { PingResponse } from './core/generated/PingResponse';

const PONG: PingResponse = { message: 'pong', appVersion: '0.1.0', coreVersion: '0.1.0' };

describe('App', () => {
  beforeEach(async () => {
    await TestBed.configureTestingModule({ imports: [App] }).compileComponents();
  });

  afterEach(() => clearMocks());

  it('affiche la réponse de ping', async () => {
    mockIPC((cmd) => (cmd === 'ping' ? PONG : undefined));
    const fixture = TestBed.createComponent(App);
    await fixture.whenStable();

    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('.brand')?.textContent).toBe('GRAM');
    expect(el.querySelector('[data-testid="ping-message"]')?.textContent).toBe('pong');
    expect(el.querySelector('.versions')?.textContent).toContain('gram-core 0.1.0');
  });

  it("affiche une erreur si l'appel échoue", async () => {
    mockIPC(() => {
      throw new Error('IPC indisponible');
    });
    const fixture = TestBed.createComponent(App);
    await fixture.whenStable();

    const alert = (fixture.nativeElement as HTMLElement).querySelector('[role="alert"]');
    expect(alert?.textContent).toContain('IPC indisponible');
  });
});
