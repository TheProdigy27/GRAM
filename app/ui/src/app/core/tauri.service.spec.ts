import { TestBed } from '@angular/core/testing';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { TauriService } from './tauri.service';

describe('TauriService', () => {
  afterEach(() => clearMocks());

  it('appelle la commande « ping »', async () => {
    const calls: string[] = [];
    mockIPC((cmd) => {
      calls.push(cmd);
      return { message: 'pong', appVersion: '0.1.0', coreVersion: '0.1.0' };
    });

    const service = TestBed.inject(TauriService);
    const res = await service.ping();

    expect(calls).toEqual(['ping']);
    expect(res.message).toBe('pong');
  });
});
