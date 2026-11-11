import { TestBed } from '@angular/core/testing';
import { TestbedHarnessEnvironment } from '@angular/cdk/testing/testbed';
import { MatCheckboxHarness } from '@angular/material/checkbox/testing';
import { MatDialogHarness } from '@angular/material/dialog/testing';
import { MatMenuHarness } from '@angular/material/menu/testing';
import { MatSelectHarness } from '@angular/material/select/testing';
import { MatSnackBarHarness } from '@angular/material/snack-bar/testing';
import { emit } from '@tauri-apps/api/event';
import { clearMocks } from '@tauri-apps/api/mocks';
import { App } from './app';
import { argsOf, mockBackend } from '../testing/backend';
import { HARDWARE, RANKING, SYNC_SUMMARY } from '../testing/fixtures';
import type { RankRequest } from './core/generated/RankRequest';
import { FILTER_DEBOUNCE_MS } from './state/ranking.store';
import { UPDATE_CHECK_DELAY_MS } from './state/update.store';

async function start(updateCheckDelayMs = 60_000) {
  await TestBed.configureTestingModule({
    imports: [App],
    providers: [
      { provide: FILTER_DEBOUNCE_MS, useValue: 0 },
      { provide: UPDATE_CHECK_DELAY_MS, useValue: updateCheckDelayMs },
    ],
  }).compileComponents();
  const fixture = TestBed.createComponent(App);
  await fixture.whenStable();
  return fixture;
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

describe('App', () => {
  afterEach(() => clearMocks());

  it('au lancement : matériel, classement et source des données, puis synchronisation sans forcer', async () => {
    const calls = mockBackend();
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;

    expect(text(el.querySelector('.brand'))).toBe('GRAM');
    expect(text(el.querySelector('[data-testid="hw-GPU"]'))).toContain('NVIDIA GeForce RTX 5080');
    expect(text(el.querySelector('[data-testid="hw-GPU"]'))).toContain('(table de référence)');
    expect(text(el.querySelector('[data-testid="hw-RAM"]'))).toContain('64 Go DDR5-6000');
    expect(text(el.querySelector('[data-testid="hw-usable"] dd'))).toBe(
      'VRAM 14,3 Go · RAM 47,3 Go',
    );

    const names = [
      ...el.querySelectorAll('[data-testid="table-items"] [data-testid="model-name"]'),
    ];
    expect(names.map(text)).toEqual(['Qwen3-8B', 'Gemma-3-12B', 'Mistral-Small-24B']);
    expect(text(el.querySelector('[data-testid="data-source"]'))).toContain('cache local');

    expect(argsOf(calls, 'sync_catalog')).toEqual([{ force: false }]);
    expect(argsOf(calls, 'get_settings')).toHaveLength(1);
  });

  it('les filtres relancent le classement ; le runtime choisi est enregistré', async () => {
    const calls = mockBackend();
    const fixture = await start();
    const loader = TestbedHarnessEnvironment.loader(fixture);

    const useCase = await loader.getHarness(
      MatSelectHarness.with({ selector: '[data-testid="filter-use-case"]' }),
    );
    await useCase.clickOptions({ text: 'Code' });
    await fixture.whenStable();
    let last = argsOf(calls, 'rank').at(-1)!['request'] as RankRequest;
    expect(last.useCase).toBe('code');

    const tooBig = await loader.getHarness(MatCheckboxHarness);
    await tooBig.check();
    await fixture.whenStable();
    last = argsOf(calls, 'rank').at(-1)!['request'] as RankRequest;
    expect(last.showTooBig).toBe(true);

    const runtime = await loader.getHarness(
      MatSelectHarness.with({ selector: '[data-testid="filter-runtime"]' }),
    );
    await runtime.clickOptions({ text: /llama\.cpp/ });
    await fixture.whenStable();
    last = argsOf(calls, 'rank').at(-1)!['request'] as RankRequest;
    expect(last.runtime).toBe('llamaCpp');
    const saved = argsOf(calls, 'save_settings').at(-1)!['settings'] as { runtime: string };
    expect(saved.runtime).toBe('llamaCpp');
  });

  it('ouvre la fiche détail au clic sur une ligne', async () => {
    const calls = mockBackend();
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;

    (el.querySelector('[data-testid="table-items"] tr.mat-mdc-row') as HTMLElement).click();
    await fixture.whenStable();

    expect(argsOf(calls, 'get_model_detail')[0]['request']).toEqual({
      repoId: 'Qwen/Qwen3-8B-GGUF',
      useCase: 'general',
    });
    expect(text(el.querySelector('[data-testid="detail-title"]'))).toBe('Qwen3-8B');
  });

  it('affiche la progression de la synchronisation et recalcule le classement à la fin', async () => {
    let finish: (v: unknown) => void = () => undefined;
    const calls = mockBackend({
      sync_catalog: () => new Promise((resolve) => (finish = resolve)),
    });
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;
    expect(argsOf(calls, 'rank')).toHaveLength(1);

    await emit('sync-progress', { phase: 'details', done: 45, total: 150 });
    await emit('rate-limit', { remaining: 412, resetS: 240 });
    fixture.detectChanges();
    expect(text(el.querySelector('[data-testid="sync-progress"]'))).toContain(
      'Tailles des fichiers 45/150',
    );
    expect(el.textContent).toContain('Quota Hugging Face : 412 requêtes');

    finish({ ...SYNC_SUMMARY, source: { kind: 'online', fetchedAt: new Date().toISOString() } });
    await new Promise((r) => setTimeout(r)); // la promesse de sync_catalog n'est pas suivie par Angular
    await fixture.whenStable();
    expect(el.querySelector('[data-testid="sync-progress"]')).toBeNull();
    expect(argsOf(calls, 'rank')).toHaveLength(2);
  });

  it('exporte le classement affiché depuis le menu', async () => {
    const calls = mockBackend();
    const fixture = await start();
    const loader = TestbedHarnessEnvironment.loader(fixture);
    const root = TestbedHarnessEnvironment.documentRootLoader(fixture);

    const menu = await loader.getHarness(
      MatMenuHarness.with({ selector: '[data-testid="export"]' }),
    );
    await menu.clickItem({ text: /CSV/ });
    await fixture.whenStable();

    // Sans chemin : le backend ouvre le dialogue « Enregistrer sous ».
    expect(argsOf(calls, 'export_ranking')).toEqual([{ format: 'csv' }]);
    const snack = await root.getHarness(MatSnackBarHarness);
    expect(await snack.getMessage()).toBe(
      'Classement exporté : C:\\Users\\x\\Documents\\classement-gram.csv',
    );
  });

  it('un export annulé ne montre rien', async () => {
    const calls = mockBackend({ export_ranking: () => null });
    const fixture = await start();
    const loader = TestbedHarnessEnvironment.loader(fixture);
    const root = TestbedHarnessEnvironment.documentRootLoader(fixture);
    const menu = await loader.getHarness(
      MatMenuHarness.with({ selector: '[data-testid="export"]' }),
    );
    await menu.clickItem({ text: /Markdown/ });
    await fixture.whenStable();
    expect(argsOf(calls, 'export_ranking')).toEqual([{ format: 'md' }]);
    expect(await root.getAllHarnesses(MatSnackBarHarness)).toHaveLength(0);
  });

  it('simulation en cours : signalée dans le bandeau, arrêtée en gardant les corrections', async () => {
    const simulated = {
      ...HARDWARE,
      overrides: {
        ...HARDWARE.overrides,
        ramChannels: 4,
        simulatedGpu: 'NVIDIA GeForce RTX 4090 24 Go',
      },
    };
    const calls = mockBackend({ get_hardware: () => simulated });
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;
    expect(text(el.querySelector('[data-testid="simulation"]'))).toContain('Simulation');
    const ranks = argsOf(calls, 'rank').length;

    (el.querySelector('[data-testid="stop-simulation"]') as HTMLButtonElement).click();
    await fixture.whenStable();
    expect(argsOf(calls, 'set_hardware_overrides')).toEqual([
      { overrides: { ...simulated.overrides, simulatedGpu: null, simulatedRamTotal: null } },
    ]);
    expect(el.querySelector('[data-testid="simulation"]')).toBeNull();
    expect(argsOf(calls, 'rank').length).toBe(ranks + 1);
  });

  it('signale une mise à jour trouvée au lancement et ouvre son dialogue', async () => {
    const calls = mockBackend({
      check_update: () => ({
        currentVersion: '0.1.0',
        update: { version: '0.2.0', notes: null, date: null },
      }),
    });
    const fixture = await start(0);
    await new Promise((r) => setTimeout(r, 10)); // vérification différée, hors d'Angular
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    const root = TestbedHarnessEnvironment.documentRootLoader(fixture);
    expect(argsOf(calls, 'check_update')).toHaveLength(1);

    const snack = await root.getHarness(MatSnackBarHarness);
    expect(await snack.getMessage()).toBe('GRAM 0.2.0 est disponible.');
    expect(text(el.querySelector('[data-testid="open-update"]'))).toBe(
      'system_update_alt Mise à jour 0.2.0',
    );

    await snack.dismissWithAction();
    await fixture.whenStable();
    const dialog = await root.getHarness(MatDialogHarness);
    expect(await dialog.getTitleText()).toBe('Mise à jour de GRAM');
    expect(await dialog.getContentText()).toContain('GRAM 0.2.0 est disponible');
    // Déjà connue : pas de nouvelle vérification.
    expect(argsOf(calls, 'check_update')).toHaveLength(1);
  });

  it('rien n’est signalé si GRAM est à jour ; le bouton lance une vérification', async () => {
    const calls = mockBackend();
    const fixture = await start(0);
    await new Promise((r) => setTimeout(r, 10));
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    const root = TestbedHarnessEnvironment.documentRootLoader(fixture);
    expect(argsOf(calls, 'check_update')).toHaveLength(1);
    expect(await root.getAllHarnesses(MatSnackBarHarness)).toHaveLength(0);

    (el.querySelector('[data-testid="open-update"]') as HTMLButtonElement).click();
    await fixture.whenStable();
    await new Promise((r) => setTimeout(r));
    await fixture.whenStable();
    const dialog = await root.getHarness(MatDialogHarness);
    expect(await dialog.getContentText()).toContain('GRAM 0.1.0 est à jour.');
    expect(argsOf(calls, 'check_update')).toHaveLength(2);
  });

  it('ouvre les paramètres et le dialogue « Corriger… »', async () => {
    const calls = mockBackend();
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;
    const root = TestbedHarnessEnvironment.documentRootLoader(fixture);

    (el.querySelector('[data-testid="open-settings"]') as HTMLButtonElement).click();
    await fixture.whenStable();
    let dialog = await root.getHarness(MatDialogHarness);
    expect(await dialog.getTitleText()).toBe('Paramètres');
    await dialog.close();
    expect(argsOf(calls, 'has_hf_token')).toHaveLength(1);

    (el.querySelector('[data-testid="open-corrections"]') as HTMLButtonElement).click();
    await fixture.whenStable();
    // Le dialogue des paramètres peut être encore en train de se fermer.
    dialog = (await root.getAllHarnesses(MatDialogHarness)).at(-1)!;
    expect(await dialog.getTitleText()).toBe('Corriger le matériel');
    expect(argsOf(calls, 'hardware_options')).toHaveLength(1);
  });

  it('signale une erreur de classement et permet de réessayer', async () => {
    let fail = true;
    const calls = mockBackend({
      rank: () => {
        if (fail) throw 'aucun catalogue disponible';
        return RANKING;
      },
    });
    const fixture = await start();
    const el = fixture.nativeElement as HTMLElement;
    expect(text(el.querySelector('.error-box'))).toContain(
      'Classement impossible : aucun catalogue disponible',
    );

    fail = false;
    (el.querySelector('.error-box button') as HTMLButtonElement).click();
    await fixture.whenStable();
    expect(argsOf(calls, 'rank').length).toBeGreaterThanOrEqual(2);
    expect(el.querySelector('.error-box')).toBeNull();
    expect(el.querySelectorAll('[data-testid="table-items"] tr.mat-mdc-row')).toHaveLength(3);
  });
});
