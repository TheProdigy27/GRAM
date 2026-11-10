import { TestBed } from '@angular/core/testing';
import { TestbedHarnessEnvironment } from '@angular/cdk/testing/testbed';
import { MatDialogRef } from '@angular/material/dialog';
import { MatInputHarness } from '@angular/material/input/testing';
import { MatSelectHarness } from '@angular/material/select/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import { type Handler, argsOf, mockBackend } from '../../../testing/backend';
import { GIB, HARDWARE, HARDWARE_OPTIONS } from '../../../testing/fixtures';
import { HardwareStore } from '../../state/hardware.store';
import { CorrectionsDialog, NO_OVERRIDES, groupGpus, parsePositive } from './corrections-dialog';

async function open(handlers: Record<string, Handler> = {}) {
  const calls = mockBackend(handlers);
  const closed: unknown[] = [];
  TestBed.configureTestingModule({
    imports: [CorrectionsDialog],
    providers: [{ provide: MatDialogRef, useValue: { close: (v: unknown) => closed.push(v) } }],
  });
  const hardware = TestBed.inject(HardwareStore);
  await hardware.load();
  const fixture = TestBed.createComponent(CorrectionsDialog);
  await fixture.whenStable();
  const loader = TestbedHarnessEnvironment.loader(fixture);
  const el = fixture.nativeElement as HTMLElement;
  const select = (testId: string) =>
    loader.getHarness(MatSelectHarness.with({ selector: `[data-testid="${testId}"]` }));
  const click = async (testId: string) => {
    (el.querySelector(`[data-testid="${testId}"]`) as HTMLElement).click();
    await fixture.whenStable();
  };
  return { calls, closed, hardware, fixture, loader, el, select, click };
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

  it('applique corrections et simulation ; le classement est recalculé', async () => {
    const { calls, closed, hardware, select, click, fixture } = await open();
    expect(argsOf(calls, 'hardware_options')).toHaveLength(1);
    const version = hardware.version();

    // Deux GPU détectés : le choix du GPU retenu est proposé.
    const gpuIndex = await select('fix-gpu-index');
    expect(await gpuIndex.getValueText()).toBe('Automatique (le plus de VRAM)');
    expect(await (await select('fix-channels')).getValueText()).toBe('Détecté (2)');

    await (await select('fix-channels')).clickOptions({ text: '4 canaux' });
    const sim = await select('sim-gpu');
    await sim.open();
    expect((await sim.getOptionGroups()).length).toBe(3);
    await sim.clickOptions({ text: /RTX 4090 24 Go/ });
    await (await select('sim-ram')).clickOptions({ text: '128 Go' });
    await fixture.whenStable();

    await click('fix-apply');
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

  it('bande passante corrigée, puis retour aux valeurs détectées', async () => {
    const corrected = {
      ...HARDWARE,
      overrides: { ...NO_OVERRIDES, ramChannels: 1, gpuBandwidthGbs: 900 },
    };
    const { calls, loader, click, fixture } = await open({ get_hardware: () => corrected });

    const bw = await loader.getHarness(
      MatInputHarness.with({ selector: '[data-testid="fix-bandwidth"]' }),
    );
    expect(await bw.getValue()).toBe('900');
    await bw.setValue('');
    await click('fix-reset');
    await fixture.whenStable();
    await click('fix-apply');
    expect(argsOf(calls, 'set_hardware_overrides')).toEqual([{ overrides: NO_OVERRIDES }]);
  });
});
