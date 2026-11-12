import { DialogRef } from '@angular/cdk/dialog';
import { TestBed } from '@angular/core/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import { type Handler, argsOf, mockBackend } from '../../../testing/backend';
import { byTestId, chooseOption, click, queryTestId, text, typeInto } from '../../../testing/dom';
import { GIB, HARDWARE, HARDWARE_OPTIONS, MAC_HARDWARE } from '../../../testing/fixtures';
import { HardwareStore } from '../../state/hardware.store';
import {
  CorrectionsDialog,
  NO_OVERRIDES,
  detectedRows,
  groupGpus,
  parsePositive,
} from './corrections-dialog';

async function open(handlers: Record<string, Handler> = {}) {
  const calls = mockBackend(handlers);
  const closed: unknown[] = [];
  TestBed.configureTestingModule({
    imports: [CorrectionsDialog],
    providers: [
      { provide: DialogRef, useValue: { config: {}, close: (v: unknown) => closed.push(v) } },
    ],
  });
  const hardware = TestBed.inject(HardwareStore);
  await hardware.load();
  const fixture = TestBed.createComponent(CorrectionsDialog);
  await fixture.whenStable();
  const el = fixture.nativeElement as HTMLElement;
  const row = (key: string) =>
    el.querySelector<HTMLElement>(`[data-testid="fix-row"][data-key="${key}"]`)!;
  const origin = (key: string) => text(byTestId('fix-origin', row(key)));
  return { calls, closed, hardware, fixture, el, row, origin };
}

describe('CorrectionsDialog', () => {
  afterEach(() => clearMocks());

  it('regroupe les GPU simulables et lit les nombres saisis', () => {
    expect(groupGpus(HARDWARE_OPTIONS.gpus).map((g) => [g.vendor, g.gpus.length])).toEqual([
      ['NVIDIA', 2],
      ['AMD', 1],
      ['Intel', 1],
    ]);
    expect(parsePositive('960')).toBe(960);
    expect(parsePositive('448,5')).toBe(448.5);
    expect(parsePositive('')).toBeNull();
    expect(parsePositive('-3')).toBeNull();
  });

  it('une ligne par valeur, avec son origine ; une correction rappelle la valeur détectée', () => {
    const rows = detectedRows(HARDWARE, NO_OVERRIDES);
    expect(rows.map((r) => r.key)).toEqual([
      'gpu',
      'gpuType',
      'bandwidth',
      'ram',
      'channels',
      'cpu',
    ]);
    const byKey = Object.fromEntries(rows.map((r) => [r.key, r]));
    expect(byKey['gpu']).toMatchObject({ value: 'RTX 5080', source: 'detected', editable: false });
    expect(byKey['bandwidth']).toMatchObject({ value: '≈960 Go/s', source: 'table' });
    expect(byKey['ram']).toMatchObject({
      value: '64 Go DDR5-6000',
      detail: '2 barrettes sur 4 emplacements',
      editable: false,
    });
    expect(byKey['channels']).toMatchObject({
      value: '2 canaux',
      source: 'estimated',
      toConfirm: true,
      detected: null,
    });
    expect(byKey['cpu'].detail).toBe('8 cœurs / 16 threads · AVX-512');

    const corrected = detectedRows(HARDWARE, { ...NO_OVERRIDES, ramChannels: 4 });
    expect(corrected.find((r) => r.key === 'channels')).toMatchObject({
      value: '4 canaux',
      source: 'manual',
      detected: '2 canaux',
      toConfirm: false,
    });

    // Mémoire unifiée : pas de « type de GPU » à corriger.
    const mac = detectedRows(MAC_HARDWARE, NO_OVERRIDES).map((r) => r.key);
    expect(mac).toEqual(['gpu', 'bandwidth', 'ram', 'channels', 'cpu']);
  });

  it('affiche les valeurs détectées ; les estimées sont surlignées « à confirmer »', async () => {
    const { calls, row, origin } = await open();
    expect(argsOf(calls, 'hardware_options')).toHaveLength(1);
    expect(origin('bandwidth')).toBe('table de référence');
    expect(origin('channels')).toBe('estimée');
    expect(row('channels').classList).toContain('kv__row--warn');
    expect(text(row('channels'))).toContain('à confirmer');
    expect(row('bandwidth').classList).not.toContain('kv__row--warn');
    // Deux GPU détectés : la carte retenue se choisit ; le processeur ne se corrige pas.
    expect(queryTestId('edit-gpu', row('gpu'))).not.toBeNull();
    expect(queryTestId('edit-cpu', row('cpu'))).toBeNull();
    expect(byTestId('fix-apply').hasAttribute('disabled')).toBe(true);
  });

  it('applique corrections et simulation ; le classement est recalculé', async () => {
    const { calls, closed, hardware, fixture, row, origin } = await open();
    const version = hardware.version();

    // Le crayon ouvre le correcteur de la ligne, focus sur son contrôle.
    const edit = byTestId('edit-gpu', row('gpu'));
    await click(fixture, edit);
    expect(edit.getAttribute('aria-expanded')).toBe('true');
    const gpuIndex = byTestId('fix-gpu-index', row('gpu'));
    expect(text(gpuIndex.querySelector('button'))).toBe('Automatique (le plus de VRAM)');
    expect(gpuIndex.contains(document.activeElement)).toBe(true);

    await click(fixture, byTestId('edit-channels', row('channels')));
    expect(queryTestId('fix-gpu-index')).toBeNull(); // un correcteur à la fois
    await chooseOption(fixture, 'fix-channels', '4 canaux');
    expect(origin('channels')).toBe('corrigée');
    expect(text(row('channels'))).toContain('détectée : 2 canaux');
    expect(row('channels').classList).not.toContain('kv__row--warn');

    // GPU simulés regroupés par constructeur.
    const sim = byTestId('sim-gpu').querySelector<HTMLElement>('[aria-haspopup="listbox"]')!;
    await click(fixture, sim);
    const list = document.getElementById(sim.getAttribute('aria-controls')!)!;
    expect(list.querySelectorAll('[role="group"]')).toHaveLength(3);
    await chooseOption(fixture, 'sim-gpu', 'NVIDIA GeForce RTX 4090 24 Go');
    expect(text(byTestId('sim-gpu').parentElement)).toContain('ne s’appliquent pas au GPU simulé');
    await chooseOption(fixture, 'sim-ram', '128 Go');

    await click(fixture, 'fix-apply');
    expect(argsOf(calls, 'set_hardware_overrides')).toEqual([
      {
        overrides: {
          ...NO_OVERRIDES,
          ramChannels: 4,
          simulatedGpu: 'NVIDIA GeForce RTX 4090 24 Go',
          simulatedRamTotal: 128 * GIB,
        },
      },
    ]);
    expect(hardware.simulation()).toBe(true);
    expect(hardware.version()).toBe(version + 1);
    expect(closed).toEqual([true]);
  });

  it('bande passante corrigée, « Valeur détectée » d’une ligne, puis de toutes', async () => {
    const corrected = {
      ...HARDWARE,
      overrides: { ...NO_OVERRIDES, ramChannels: 1, gpuBandwidthGbs: 900 },
    };
    const { calls, fixture, row, origin } = await open({ get_hardware: () => corrected });
    expect(origin('bandwidth')).toBe('corrigée');
    expect(text(row('bandwidth'))).toContain('≈900 Go/s');
    expect(text(row('bandwidth'))).toContain('détectée : ≈960 Go/s');

    await click(fixture, byTestId('edit-bandwidth', row('bandwidth')));
    const input = byTestId<HTMLInputElement>('fix-bandwidth', row('bandwidth'));
    expect(input.value).toBe('900');
    await typeInto(fixture, input, '');
    expect(origin('bandwidth')).toBe('table de référence');

    await click(fixture, byTestId('edit-channels', row('channels')));
    await click(fixture, byTestId('fix-row-reset', row('channels')));
    expect(origin('channels')).toBe('estimée');
    await click(fixture, 'fix-apply');
    expect(argsOf(calls, 'set_hardware_overrides')).toEqual([{ overrides: NO_OVERRIDES }]);
  });

  it('« Valeurs détectées » efface corrections et simulation ; une erreur reste affichée', async () => {
    const simulated = {
      ...HARDWARE,
      overrides: { ...NO_OVERRIDES, gpuIntegrated: true, simulatedRamTotal: 32 * GIB },
    };
    const { calls, closed, fixture, origin } = await open({
      get_hardware: () => simulated,
      set_hardware_overrides: () => {
        throw 'corrections non enregistrées : accès refusé';
      },
    });
    expect(origin('gpuType')).toBe('corrigée');
    await click(fixture, 'fix-reset');
    expect(origin('gpuType')).toBe('détectée');
    await click(fixture, 'fix-apply');
    expect(argsOf(calls, 'set_hardware_overrides')).toEqual([{ overrides: NO_OVERRIDES }]);
    expect(text(document.querySelector('[role="alert"]'))).toContain('accès refusé');
    expect(closed).toEqual([]);
  });
});
