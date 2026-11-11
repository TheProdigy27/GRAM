import { TestBed } from '@angular/core/testing';
import { TestbedHarnessEnvironment } from '@angular/cdk/testing/testbed';
import { MatDialogRef } from '@angular/material/dialog';
import { MatInputHarness } from '@angular/material/input/testing';
import { MatSelectHarness } from '@angular/material/select/testing';
import { MatTabGroupHarness } from '@angular/material/tabs/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import { type Handler, argsOf, mockBackend } from '../../../testing/backend';
import { SETTINGS } from '../../../testing/fixtures';
import type { Settings } from '../../core/generated/Settings';
import { SettingsStore } from '../../state/settings.store';
import { SettingsDialog, bytesToGb, parseNumber, toPercent } from './settings-dialog';

async function open(handlers: Record<string, Handler> = {}) {
  const calls = mockBackend(handlers);
  const closed: unknown[] = [];
  TestBed.configureTestingModule({
    imports: [SettingsDialog],
    providers: [{ provide: MatDialogRef, useValue: { close: (v: unknown) => closed.push(v) } }],
  });
  const store = TestBed.inject(SettingsStore);
  await store.load();
  const fixture = TestBed.createComponent(SettingsDialog);
  await fixture.whenStable();
  const loader = TestbedHarnessEnvironment.loader(fixture);
  const el = fixture.nativeElement as HTMLElement;
  const click = async (testId: string) => {
    (el.querySelector(`[data-testid="${testId}"]`) as HTMLElement).click();
    await fixture.whenStable();
  };
  const tab = async (label: string) => {
    await (await loader.getHarness(MatTabGroupHarness)).selectTab({ label });
    await fixture.whenStable();
  };
  const input = (testId: string) =>
    loader.getHarness(MatInputHarness.with({ selector: `[data-testid="${testId}"]` }));
  const saved = () => argsOf(calls, 'save_settings').at(-1)?.['settings'] as Settings | undefined;
  return { calls, closed, store, fixture, el, click, tab, input, loader, saved };
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

describe('SettingsDialog', () => {
  afterEach(() => clearMocks());

  it('conversions des champs', () => {
    expect(toPercent(0.1)).toBe(10);
    expect(toPercent(0.10000000149011612)).toBe(10);
    expect(bytesToGb(2 ** 29)).toBe(0.5);
    expect(parseNumber('0,55')).toBe(0.55);
    expect(parseNumber(' 12 ')).toBe(12);
    expect(parseNumber('')).toBeNull();
    expect(parseNumber('abc')).toBeNull();
  });

  it('modifie une copie et l’enregistre ; le classement est recalculé', async () => {
    const { store, click, input, saved, closed, loader, fixture } = await open();
    expect(await (await input('set-vram-margin')).getValue()).toBe('10');
    const version = store.version();

    await (await input('set-vram-margin')).setValue('15');
    expect(await (await input('set-gpu-efficiency')).getValue()).toBe('0.81');
    await (await input('set-ram-efficiency')).setValue('0.5');
    const kv = await loader.getHarness(
      MatSelectHarness.with({ selector: '[data-testid="set-kv-cache"]' }),
    );
    await kv.clickOptions({ text: /^q8_0/ });
    await fixture.whenStable();
    // Rien n'est enregistré avant « Enregistrer ».
    expect(saved()).toBeUndefined();

    await click('settings-save');
    expect(saved()!.estimate.vramMargin).toBeCloseTo(0.15);
    expect(saved()!.estimate.kvCache).toBe('q8_0');
    expect(saved()!.estimate.ramShare).toBe(0.75);
    // Les deux efficacités sont indépendantes.
    expect(saved()!.estimate.ramEfficiency).toBe(0.5);
    expect(saved()!.estimate.gpuEfficiency).toBe(0.81);
    expect(store.settings()!.estimate.kvCache).toBe('q8_0');
    expect(store.version()).toBe(version + 1);
    expect(closed).toEqual([true]);
  });

  it('cas d’usage, catalogue, puis retour aux valeurs par défaut', async () => {
    const custom: Settings = {
      ...SETTINGS,
      estimate: { ...SETTINGS.estimate, vramMargin: 0.2 },
      catalogTtlHours: 72,
    };
    const { tab, el, fixture, click, input, saved } = await open({ get_settings: () => custom });

    await tab('Cas d’usage');
    const row = el.querySelector('[data-testid="use-case-reasoning"]')!;
    const slow = row.querySelector('[data-testid="slow"]') as HTMLInputElement;
    expect(slow.value).toBe('8');
    slow.value = '12';
    slow.dispatchEvent(new Event('input'));
    const ctx = row.querySelector('select') as HTMLSelectElement;
    expect(ctx.value).toBe('16384');
    ctx.value = '32768';
    ctx.dispatchEvent(new Event('change'));
    await fixture.whenStable();

    await tab('Catalogue');
    expect(await (await input('set-ttl')).getValue()).toBe('72');
    await click('settings-save');
    expect(saved()!.useCases.reasoning).toEqual({
      ...SETTINGS.useCases.reasoning,
      slowTokS: 12,
      ctx: 32768,
    });

    // « Valeurs par défaut » remplit la copie avec celles du backend (§12).
    await click('settings-reset');
    await click('settings-save');
    expect(saved()).toEqual(SETTINGS);
  });

  it('le token est masqué, jamais réaffiché, et son état seul est montré', async () => {
    let configured = false;
    const { tab, el, input, click, calls } = await open({
      has_hf_token: () => configured,
      set_hf_token: () => {
        configured = true;
        return null;
      },
    });
    await tab('Hugging Face');
    expect(text(el.querySelector('[data-testid="token-status"]'))).toBe('Token : non configuré');

    const field = await input('token-input');
    expect(await field.getType()).toBe('password');
    await field.setValue('hf_secret123');
    await click('token-save');

    expect(argsOf(calls, 'set_hf_token')).toEqual([{ token: 'hf_secret123' }]);
    expect(text(el.querySelector('[data-testid="token-status"]'))).toBe('Token : configuré');
    expect(await field.getValue()).toBe('');
    expect(el.textContent).not.toContain('hf_secret123');

    await click('token-clear');
    expect(argsOf(calls, 'clear_hf_token')).toHaveLength(1);
    expect(text(el.querySelector('[data-testid="token-status"]'))).toBe('Token : non configuré');
  });

  it('affiche la taille du cache et le vide', async () => {
    const { tab, el, click, calls } = await open();
    await tab('Catalogue');
    expect(text(el.querySelector('[data-testid="cache-summary"]'))).toBe('312 fichiers · 256 Mo');
    await click('clear-cache');
    expect(argsOf(calls, 'clear_cache')).toHaveLength(1);
    expect(text(el.querySelector('[data-testid="cache-summary"]'))).toBe('vide');
  });

  it('une erreur d’enregistrement reste affichée dans le dialogue', async () => {
    const { input, click, el, closed } = await open({
      save_settings: () => {
        throw 'Paramètres non enregistrés dans C:\\x\\settings.json : accès refusé';
      },
    });
    await (await input('set-gpu-efficiency')).setValue('0,7');
    await click('settings-save');
    expect(text(el.querySelector('[role="alert"]'))).toContain('accès refusé');
    expect(closed).toEqual([]);
  });
});
