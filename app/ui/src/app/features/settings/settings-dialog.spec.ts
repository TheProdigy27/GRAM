import { DialogRef } from '@angular/cdk/dialog';
import { TestBed } from '@angular/core/testing';
import { clearMocks } from '@tauri-apps/api/mocks';
import { type Handler, argsOf, mockBackend } from '../../../testing/backend';
import {
  allByTestId,
  byTestId,
  chooseOption,
  click,
  press,
  queryTestId,
  text,
  toggleSwitch,
  typeInto,
} from '../../../testing/dom';
import { CALIBRATION, SETTINGS } from '../../../testing/fixtures';
import type { Settings } from '../../core/generated/Settings';
import { parseNumber } from '../../shared/format';
import { SettingsStore } from '../../state/settings.store';
import { SettingsDialog, bytesToGb, toPercent } from './settings-dialog';
import { calibratedPaths, countDifferences, searchRows, weightSegments } from './settings-model';

async function open(handlers: Record<string, Handler> = {}) {
  const calls = mockBackend(handlers);
  const closed: unknown[] = [];
  TestBed.configureTestingModule({
    imports: [SettingsDialog],
    providers: [
      { provide: DialogRef, useValue: { config: {}, close: (v: unknown) => closed.push(v) } },
    ],
  });
  const store = TestBed.inject(SettingsStore);
  await store.load();
  const fixture = TestBed.createComponent(SettingsDialog);
  // Calibration et version arrivent hors d'Angular.
  await new Promise((r) => setTimeout(r));
  await fixture.whenStable();
  const el = fixture.nativeElement as HTMLElement;
  const nav = (id: string) => click(fixture, `nav-${id}`);
  const row = (id: string) => el.querySelector<HTMLElement>(`[data-row="${id}"]`);
  const input = (testId: string) => byTestId<HTMLInputElement>(testId, el);
  const saved = () => argsOf(calls, 'save_settings').at(-1)?.['settings'] as Settings | undefined;
  return { calls, closed, store, fixture, el, nav, row, input, saved };
}

describe('SettingsDialog', () => {
  afterEach(() => clearMocks());

  it('conversions, répartition des poids, comparaison et recherche', () => {
    expect(toPercent(0.1)).toBe(10);
    expect(toPercent(0.10000000149011612)).toBe(10);
    expect(bytesToGb(2 ** 29)).toBe(0.5);
    expect(parseNumber('0,55')).toBe(0.55);

    const segments = weightSegments({ quality: 0.45, speed: 0.45, fit: 0.18, context: 0.1 });
    expect(segments.map((s) => s.text)).toEqual(['Qualité 38 %', 'Vitesse 38 %', '15 %', '']);

    // Les f32 de Rust reviennent arrondis : pas une modification.
    const f32 = {
      ...SETTINGS,
      estimate: { ...SETTINGS.estimate, vramMargin: 0.10000000149011612 },
    };
    expect(countDifferences(SETTINGS, f32)).toBe(0);
    expect(countDifferences(SETTINGS, { ...SETTINGS, pageSize: 50, minDownloads: 0 })).toBe(2);

    expect(calibratedPaths(CALIBRATION, { ...SETTINGS.estimate, gpuEfficiency: 0.48 })).toEqual(
      new Set(['estimate.gpuEfficiency']),
    );
    expect(calibratedPaths(CALIBRATION, SETTINGS.estimate).size).toBe(0);

    expect(searchRows('efficacite')).toEqual(new Set(['gpuEfficiency', 'ramEfficiency']));
    expect(searchRows('TOKEN')).toContain('token');
  });

  it('modifie une copie et l’enregistre ; le classement est recalculé', async () => {
    const { store, fixture, el, row, input, saved, closed } = await open();
    expect(text(byTestId('nav-estimate', el))).toBe('Estimation');
    expect(byTestId('nav-estimate', el).getAttribute('aria-current')).toBe('page');
    expect(input('set-vram-margin').value).toBe('10');
    expect(text(row('vramMargin'))).toContain('défaut 10 %');
    expect(queryTestId('modified', row('vramMargin')!)).toBeNull();
    expect(byTestId('settings-save', el).hasAttribute('disabled')).toBe(true);
    const version = store.version();

    await typeInto(fixture, input('set-vram-margin'), '15');
    expect(text(byTestId('modified', row('vramMargin')!))).toBe('modifié');
    expect(text(byTestId('unsaved', el))).toBe('1 modification non enregistrée');
    expect(input('set-gpu-efficiency').value).toBe('0,81');
    await typeInto(fixture, input('set-ram-efficiency'), '0,5');
    await chooseOption(fixture, 'set-kv-cache', 'q8_0');
    expect(await toggleSwitch(fixture, 'set-close-apps')).toBe(true);
    expect(text(byTestId('nav-modified', byTestId('nav-estimate', el)))).toContain('4');
    expect(text(byTestId('unsaved', el))).toBe('4 modifications non enregistrées');
    // Rien n'est enregistré avant « Enregistrer ».
    expect(saved()).toBeUndefined();

    await click(fixture, 'settings-save');
    expect(saved()!.estimate.vramMargin).toBeCloseTo(0.15);
    expect(saved()!.estimate.kvCache).toBe('q8_0');
    expect(saved()!.estimate.closeApps).toBe(true);
    expect(saved()!.estimate.ramShare).toBe(0.75);
    // Les deux efficacités sont indépendantes.
    expect(saved()!.estimate.ramEfficiency).toBe(0.5);
    expect(saved()!.estimate.gpuEfficiency).toBe(0.81);
    expect(store.settings()!.estimate.kvCache).toBe('q8_0');
    expect(store.version()).toBe(version + 1);
    expect(closed).toEqual([true]);
    expect(text(document.querySelector('.g-toast__message'))).toBe('Paramètres enregistrés.');
  });

  it('cas d’usage : usage modifié, contexte en pastilles, lent sous, poids du score', async () => {
    const { fixture, el, nav, input, saved } = await open();
    await nav('useCases');
    const useCase = byTestId('set-use-case', el);
    expect(text(useCase.querySelector('[aria-pressed="true"]'))).toBe('Polyvalent');
    await chooseOption(fixture, useCase, 'Raisonnement');
    expect(text(byTestId('settings-reset', el))).toBe('Rétablir « Raisonnement »');

    // Lent sous : − / +, saisie, flèches.
    expect(input('set-slow').value).toBe('8');
    await click(fixture, 'slow-plus');
    expect(input('set-slow').value).toBe('9');
    await typeInto(fixture, input('set-slow'), '12');
    await press(fixture, input('set-slow'), 'ArrowDown');
    expect(input('set-slow').value).toBe('11');

    // Contexte par défaut en pastilles.
    const ctx = byTestId('set-ctx', el);
    expect(text(ctx.querySelector('[aria-pressed="true"]'))).toBe('16k');
    await chooseOption(fixture, ctx, '32k');

    // Poids : barre de répartition, curseurs et champs.
    const bar = byTestId('weights-bar', el);
    expect(bar.getAttribute('aria-label')).toBe(
      'Répartition : qualité 55 %, vitesse 20 %, marge 10 %, contexte 15 %',
    );
    const weights = () =>
      Object.fromEntries(
        [...el.querySelectorAll<HTMLElement>('[data-testid="weight"]')].map((w) => [
          w.dataset['weight']!,
          w,
        ]),
      );
    expect(Object.keys(weights())).toEqual(['quality', 'speed', 'fit', 'context']);
    await typeInto(fixture, byTestId('weight-value', weights()['quality']), '1,1');
    expect(bar.getAttribute('aria-label')).toContain('qualité 71 %');
    expect(text(weights()['quality'])).toContain('modifié');
    await click(fixture, byTestId('weight-reset', weights()['quality']));
    expect(byTestId<HTMLInputElement>('weight-value', weights()['quality']).value).toBe('0,55');
    const range = byTestId<HTMLInputElement>('weight-range', weights()['speed']);
    range.value = '0.3';
    range.dispatchEvent(new Event('input'));
    await fixture.whenStable();
    expect(byTestId<HTMLInputElement>('weight-value', weights()['speed']).value).toBe('0,3');

    await click(fixture, 'settings-save');
    expect(saved()!.useCases.reasoning).toEqual({
      ctx: 32768,
      slowTokS: 11,
      weights: { ...SETTINGS.useCases.reasoning.weights, speed: 0.3 },
    });
    expect(saved()!.useCases.general).toEqual(SETTINGS.useCases.general);
  });

  it('« rétablir » une ligne ou une section ; recherche d’un réglage', async () => {
    const custom: Settings = {
      ...SETTINGS,
      estimate: { ...SETTINGS.estimate, vramMargin: 0.2 },
      catalogTtlHours: 72,
    };
    const { fixture, el, nav, row, input } = await open({ get_settings: () => custom });
    expect(text(byTestId('nav-modified', byTestId('nav-estimate', el)))).toContain('1');
    expect(input('set-vram-margin').value).toBe('20');
    await click(fixture, byTestId('row-reset', row('vramMargin')!));
    expect(input('set-vram-margin').value).toBe('10');
    expect(queryTestId('modified', row('vramMargin')!)).toBeNull();
    expect(queryTestId('nav-modified', byTestId('nav-estimate', el))).toBeNull();

    await nav('catalog');
    expect(input('set-ttl').value).toBe('72');
    expect(text(byTestId('settings-reset', el))).toBe('Rétablir le catalogue');
    await click(fixture, 'settings-reset');
    expect(input('set-ttl').value).toBe('24');
    expect(text(byTestId('unsaved', el))).toBe('2 modifications non enregistrées');

    // La recherche montre les réglages trouvés, toutes sections confondues.
    const search = input('settings-search');
    await typeInto(fixture, search, 'efficacité');
    expect(
      [...el.querySelectorAll('[data-section]')].map((s) => s.getAttribute('data-section')),
    ).toEqual(['estimate']);
    expect([...el.querySelectorAll('[data-row]')].map((r) => r.getAttribute('data-row'))).toEqual([
      'gpuEfficiency',
      'ramEfficiency',
    ]);
    expect(byTestId('nav-useCases', el).classList).toContain('nav__item--dim');
    expect(queryTestId('settings-reset', el)).toBeNull();
    await typeInto(fixture, search, 'hugging face');
    expect(
      [...el.querySelectorAll('[data-section]')].map((s) => s.getAttribute('data-section')),
    ).toEqual(['catalog', 'about']);
    await typeInto(fixture, search, 'zzz');
    expect(text(byTestId('settings-empty', el))).toBe('Aucun réglage ne correspond à « zzz ».');

    // Échap vide la recherche ; un clic dans la navigation aussi.
    const escape = await press(fixture, search, 'Escape');
    expect(escape.defaultPrevented).toBe(true);
    expect(search.value).toBe('');
    await typeInto(fixture, search, 'cache');
    await nav('useCases');
    expect(search.value).toBe('');
    expect(byTestId('nav-useCases', el).getAttribute('aria-current')).toBe('page');
  });

  it('le token est masqué, jamais réaffiché, et son état seul est montré', async () => {
    let configured = false;
    const { fixture, el, nav, input, calls } = await open({
      has_hf_token: () => configured,
      set_hf_token: () => {
        configured = true;
        return null;
      },
    });
    await nav('catalog');
    expect(text(byTestId('token-status', el))).toBe('non configuré');

    const field = input('token-input');
    expect(field.type).toBe('password');
    await typeInto(fixture, field, 'hf_secret123');
    await click(fixture, 'token-save');

    expect(argsOf(calls, 'set_hf_token')).toEqual([{ token: 'hf_secret123' }]);
    expect(text(byTestId('token-status', el))).toBe('configuré');
    expect(field.value).toBe('');
    expect(el.textContent).not.toContain('hf_secret123');

    await click(fixture, 'token-clear');
    expect(argsOf(calls, 'clear_hf_token')).toHaveLength(1);
    expect(text(byTestId('token-status', el))).toBe('non configuré');
  });

  it('affiche la taille du cache et le vide', async () => {
    const { fixture, el, nav, calls } = await open();
    await nav('catalog');
    expect(text(byTestId('cache-summary', el))).toBe('312 fichiers · 256 Mo');
    await click(fixture, 'clear-cache');
    expect(argsOf(calls, 'clear_cache')).toHaveLength(1);
    expect(text(byTestId('cache-summary', el))).toBe('vide');
  });

  it('une erreur d’enregistrement reste affichée dans le dialogue', async () => {
    const { fixture, input, el, closed } = await open({
      save_settings: () => {
        throw 'Paramètres non enregistrés dans C:\\x\\settings.json : accès refusé';
      },
    });
    await typeInto(fixture, input('set-gpu-efficiency'), '0,7');
    await click(fixture, 'settings-save');
    expect(text(el.querySelector('[role="alert"]'))).toContain('accès refusé');
    expect(closed).toEqual([]);
  });

  it('mesures : efficacités proposées, puis repère « recalibrée »', async () => {
    const { fixture, el, row, input, saved } = await open({ get_calibration: () => CALIBRATION });
    expect(queryTestId('nav-recalibrated', el)).toBeNull();
    expect(text(byTestId('calibration-offer', el))).toContain('efficacité GPU 0,48');

    await click(fixture, 'use-calibration');
    expect(input('set-gpu-efficiency').value).toBe('0,48');
    expect(text(byTestId('calibrated', row('gpuEfficiency')!))).toBe('recalibrée');
    expect(queryTestId('calibration-offer', el)).toBeNull();
    expect(queryTestId('nav-recalibrated', el)).not.toBeNull();
    // Une valeur recalibrée n'est pas comptée parmi les réglages modifiés.
    expect(queryTestId('nav-modified', byTestId('nav-estimate', el))).toBeNull();

    await click(fixture, 'settings-save');
    expect(saved()!.estimate.gpuEfficiency).toBe(0.48);
  });

  it('mises à jour et à propos', async () => {
    let available = false;
    const { fixture, el, nav, calls } = await open({
      check_update: () => ({
        currentVersion: '0.1.0',
        update: available ? { version: '0.2.0', notes: null, date: null } : null,
      }),
    });
    await nav('updates');
    expect(text(byTestId('app-version', el))).toBe('GRAM 0.1.0');
    await click(fixture, 'update-check');
    await new Promise((r) => setTimeout(r));
    await fixture.whenStable();
    expect(argsOf(calls, 'check_update')).toHaveLength(1);
    expect(text(byTestId('update-state', el))).toBe('GRAM est à jour.');

    available = true;
    await click(fixture, 'update-check');
    await new Promise((r) => setTimeout(r));
    await fixture.whenStable();
    expect(text(byTestId('update-state', el))).toBe('GRAM 0.2.0 disponible');
    expect(queryTestId('update-open', el)).not.toBeNull();

    await click(fixture, 'open-releases');
    expect(argsOf(calls, 'plugin:opener|open_url')[0]['url']).toContain('/releases/latest');

    await nav('about');
    expect(text(byTestId('about-version', el))).toBe('Version 0.1.0');
    await click(fixture, 'open-hf');
    expect(argsOf(calls, 'plugin:opener|open_url')[1]['url']).toContain('huggingface.co');
    expect(allByTestId('nav-about', el)).toHaveLength(1);
  });
});
