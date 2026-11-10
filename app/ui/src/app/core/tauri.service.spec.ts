import { TestBed } from '@angular/core/testing';
import { emit } from '@tauri-apps/api/event';
import { clearMocks } from '@tauri-apps/api/mocks';
import { argsOf, mockBackend } from '../../testing/backend';
import type { SyncProgress } from './generated/SyncProgress';
import { TauriService } from './tauri.service';

describe('TauriService', () => {
  afterEach(() => clearMocks());

  it('appelle les commandes avec les arguments en camelCase attendus par Rust', async () => {
    const calls = mockBackend();
    const service = TestBed.inject(TauriService);

    expect((await service.ping()).message).toBe('pong');
    await service.rank({ useCase: 'code', ctx: 16384, showTooBig: false, limit: 25 });
    await service.syncCatalog(true);
    await service.getModelDetail({ repoId: 'Qwen/Qwen3-8B-GGUF', useCase: 'general' });

    expect(argsOf(calls, 'rank')).toEqual([
      { request: { useCase: 'code', ctx: 16384, showTooBig: false, limit: 25 } },
    ]);
    expect(argsOf(calls, 'sync_catalog')).toEqual([{ force: true }]);
    expect(argsOf(calls, 'get_model_detail')[0]).toEqual({
      request: { repoId: 'Qwen/Qwen3-8B-GGUF', useCase: 'general' },
    });
  });

  it('rejette avec une Error portant le message français du backend', async () => {
    mockBackend({
      rank: () => {
        throw 'aucun catalogue disponible : ni connexion, ni cache, ni catalogue embarqué';
      },
    });
    const service = TestBed.inject(TauriService);
    await expect(service.rank({ useCase: 'general', showTooBig: false })).rejects.toThrow(
      /^aucun catalogue disponible/,
    );
  });

  it('copie dans le presse-papiers et ouvre les pages via les plugins', async () => {
    const calls = mockBackend();
    const service = TestBed.inject(TauriService);
    await service.copyText('ollama run hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M');
    await service.openUrl('https://huggingface.co/Qwen/Qwen3-8B-GGUF');
    expect(argsOf(calls, 'plugin:clipboard-manager|write_text')[0]['text']).toBe(
      'ollama run hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M',
    );
    expect(argsOf(calls, 'plugin:opener|open_url')[0]['url']).toBe(
      'https://huggingface.co/Qwen/Qwen3-8B-GGUF',
    );
  });

  it('transmet la charge utile des événements', async () => {
    mockBackend();
    const service = TestBed.inject(TauriService);
    const received: SyncProgress[] = [];
    const unlisten = await service.listen('sync-progress', (p) => received.push(p));

    await emit('sync-progress', { phase: 'details', done: 3, total: 150 });
    expect(received).toEqual([{ phase: 'details', done: 3, total: 150 }]);

    unlisten();
    await emit('sync-progress', { phase: 'details', done: 4, total: 150 });
    expect(received).toHaveLength(1);
  });
});
