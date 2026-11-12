import { DialogRef } from '@angular/cdk/dialog';
import { Component, computed, inject, signal } from '@angular/core';
import type { CacheInfo } from '../../core/generated/CacheInfo';
import type { KvCacheType } from '../../core/generated/KvCacheType';
import type { ScoreWeights } from '../../core/generated/ScoreWeights';
import type { Settings } from '../../core/generated/Settings';
import type { UseCase } from '../../core/generated/UseCase';
import { TauriService, errorMessage } from '../../core/tauri.service';
import {
  CTX_STEPS,
  ctxK,
  decimal,
  fileSize,
  int,
  parseNumber,
  sameNumber,
} from '../../shared/format';
import { USE_CASES } from '../../shared/labels';
import { Button } from '../../shared/ui/button/button';
import { Callout } from '../../shared/ui/callout/callout';
import { GDialog } from '../../shared/ui/dialog/dialog';
import { FIELD } from '../../shared/ui/field/field';
import { Icon } from '../../shared/ui/icon/icon';
import { Progress } from '../../shared/ui/progress/progress';
import { Spinner } from '../../shared/ui/spinner/spinner';
import { Switch } from '../../shared/ui/switch/switch';
import { Tag } from '../../shared/ui/tag/tag';
import { ToastService } from '../../shared/ui/toast/toast';
import { ToggleGroup, type ToggleOption } from '../../shared/ui/toggle-group/toggle-group';
import { Tooltip } from '../../shared/ui/tooltip/tooltip';
import { CalibrationStore } from '../../state/calibration.store';
import { RankingStore } from '../../state/ranking.store';
import { SettingsStore } from '../../state/settings.store';
import { RELEASES_URL, UpdateStore } from '../../state/update.store';
import { UpdateLauncher } from '../update/update-launcher';
import { type RowMarker, SettingRow } from './setting-row';
import {
  ROWS,
  type RowDef,
  SECTIONS,
  type SectionId,
  calibratedPaths,
  calibrationPending,
  countDifferences,
  getPath,
  rowPath,
  sameValue,
  searchRows,
  sectionDifferences,
  setPath,
  withCalibration,
} from './settings-model';
import { type WeightChange, WeightsEditor } from './weights-editor';

const GIB = 2 ** 30;
const HF_URL = 'https://huggingface.co/models?library=gguf';

export const KV_CACHE_TYPES: readonly ToggleOption<KvCacheType>[] = [
  { value: 'f16', label: 'f16', tooltip: 'Défaut d’Ollama et de llama.cpp' },
  { value: 'q8_0', label: 'q8_0', tooltip: 'Contexte deux fois plus léger' },
  { value: 'q4_0', label: 'q4_0', tooltip: 'Contexte quatre fois plus léger, qualité réduite' },
];

/** Fraction → pourcentage affiché (0,1 → 10). */
export function toPercent(x: number): number {
  return Math.round(x * 1000) / 10;
}

/** Octets → Go affichés, deux décimales au plus. */
export function bytesToGb(bytes: number): number {
  return Math.round((bytes / GIB) * 100) / 100;
}

/** Valeur d'un réglage numérique telle qu'affichée : unité d'affichage, décimales. */
export function displayNumber(row: RowDef, value: number): string {
  const shown = value * (row.scale ?? 1);
  return decimal(Math.round(shown * 1e6) / 1e6, row.digits ?? 2);
}

/**
 * Paramètres (§11.2 et §12) : navigation par section (estimation, cas d'usage, catalogue, mises
 * à jour, à propos) et recherche d'un réglage. Chaque réglage est une ligne avec sa valeur par
 * défaut et un repère « modifié » ; les réglages sont modifiés sur une copie et enregistrés avec
 * « Enregistrer » (Rust les ramène dans leurs bornes). Le token et le vidage du cache
 * s'appliquent tout de suite.
 *
 * Pas d'aperçu du classement avec les poids en cours : `rank` lit les poids des paramètres
 * enregistrés, il faudrait enregistrer à chaque modification.
 */
@Component({
  selector: 'app-settings-dialog',
  imports: [
    Button,
    Callout,
    FIELD,
    GDialog,
    Icon,
    Progress,
    SettingRow,
    Spinner,
    Switch,
    Tag,
    ToggleGroup,
    Tooltip,
    WeightsEditor,
  ],
  templateUrl: './settings-dialog.html',
  styleUrl: './settings-dialog.scss',
})
export class SettingsDialog {
  private readonly tauri = inject(TauriService);
  private readonly store = inject(SettingsStore);
  private readonly dialogRef = inject(DialogRef);
  private readonly toast = inject(ToastService);
  private readonly calibration = inject(CalibrationStore);
  protected readonly updates = inject(UpdateStore);
  private readonly updateLauncher = inject(UpdateLauncher);

  protected readonly sections = SECTIONS;
  protected readonly kvTypes = KV_CACHE_TYPES;
  protected readonly sameNumber = sameNumber;
  protected readonly ctxK = ctxK;
  protected readonly useCaseOptions: ToggleOption<UseCase>[] = USE_CASES.map((u) => ({
    value: u.value,
    label: u.label,
  }));
  protected readonly ctxOptions: ToggleOption<number>[] = CTX_STEPS.map((c) => ({
    value: c,
    label: ctxK(c),
  }));

  /** Copie modifiée, enregistrée avec « Enregistrer ». */
  protected readonly draft = signal<Settings | null>(this.copy(this.store.settings()));
  protected readonly defaults = signal<Settings | null>(null);
  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly section = signal<SectionId>('estimate');
  /** Cas d'usage modifié : celui du classement à l'ouverture. */
  protected readonly useCase = signal<UseCase>(inject(RankingStore).useCase());
  protected readonly query = signal('');

  protected readonly hasToken = signal<boolean | null>(null);
  protected readonly token = signal('');
  protected readonly tokenBusy = signal(false);

  protected readonly cache = signal<CacheInfo | null>(null);
  protected readonly cacheBusy = signal(false);

  protected readonly appVersion = signal<string | null>(null);

  // ── Recherche et navigation ───────────────────────────────────────────────────────────────

  protected readonly searching = computed(() => this.query().trim() !== '');
  private readonly found = computed(() => searchRows(this.query()));

  /** Sections affichées : la section choisie, ou toutes celles qui ont un réglage trouvé. */
  protected readonly visibleSections = computed(() => {
    if (!this.searching()) return SECTIONS.filter((s) => s.id === this.section());
    const found = this.found();
    return SECTIONS.filter((s) => ROWS.some((r) => r.section === s.id && found.has(r.id)));
  });

  protected shown(id: string): boolean {
    if (this.searching()) return this.found().has(id);
    return ROWS.find((r) => r.id === id)?.section === this.section();
  }

  protected rowsOf(section: SectionId): RowDef[] {
    return ROWS.filter((r) => r.section === section && this.shown(r.id));
  }

  protected readonly sectionHasMatch = computed(() => {
    const found = this.found();
    return new Set(ROWS.filter((r) => found.has(r.id)).map((r) => r.section));
  });

  protected goTo(id: SectionId): void {
    this.query.set('');
    this.section.set(id);
  }

  /** ↑ ↓ (Début, Fin) d'une section à l'autre dans la navigation. */
  protected onNavKeydown(event: KeyboardEvent): void {
    const keys = ['ArrowDown', 'ArrowUp', 'Home', 'End'];
    if (!keys.includes(event.key)) return;
    const buttons = [
      ...((event.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('button') ?? []),
    ];
    const i = buttons.indexOf(event.target as HTMLElement);
    if (i < 0) return;
    event.preventDefault();
    const last = buttons.length - 1;
    const next =
      event.key === 'Home'
        ? 0
        : event.key === 'End'
          ? last
          : event.key === 'ArrowDown'
            ? Math.min(i + 1, last)
            : Math.max(i - 1, 0);
    buttons[next].focus();
  }

  /** Échap dans la recherche : la vide d'abord, sans fermer le dialogue. */
  protected onSearchEscape(event: Event): void {
    if (!this.searching()) return;
    event.preventDefault();
    event.stopPropagation();
    this.query.set('');
  }

  // ── Repères : modifié, recalibré, modifications non enregistrées ─────────────────────────

  private readonly calibrated = computed(() => {
    const d = this.draft();
    return d ? calibratedPaths(this.calibration.calibration(), d.estimate) : new Set<string>();
  });

  /** Les mesures proposent d'autres efficacités que celles de la copie. */
  protected readonly calibrationOffer = computed(() => {
    const d = this.draft();
    const cal = this.calibration.calibration();
    return d && cal && calibrationPending(cal, d.estimate) ? cal.summary : null;
  });

  /** Repère de chaque section dans la navigation. */
  protected readonly navBadges = computed(() => {
    const d = this.draft();
    const def = this.defaults();
    const badges: Partial<Record<SectionId, number>> = {};
    if (!d || !def) return badges;
    for (const s of ['estimate', 'useCases', 'catalog'] as const) {
      badges[s] = sectionDifferences(s, d, def, this.calibrated());
    }
    return badges;
  });

  protected readonly recalibrated = computed(() => this.calibrated().size > 0);

  protected readonly unsaved = computed(() => {
    const d = this.draft();
    const saved = this.store.settings();
    return d && saved ? countDifferences(d, saved) : 0;
  });

  protected path(row: RowDef): string {
    return rowPath(row.path ?? '', this.useCase());
  }

  protected value<T = unknown>(path: string): T {
    return getPath(this.draft()!, path) as T;
  }

  protected marker(path: string): RowMarker {
    if (this.calibrated().has(path)) return 'calibrated';
    const def = this.defaults();
    const d = this.draft();
    if (!def || !d) return null;
    return sameValue(getPath(d, path), getPath(def, path)) ? null : 'modified';
  }

  /** Valeur affichée d'un réglage numérique (« 10 », « 0,5 »). */
  protected numberText(row: RowDef): string {
    return displayNumber(row, this.value<number>(this.path(row)));
  }

  protected decimalText(value: number): string {
    return decimal(value, 1);
  }

  /** « défaut 10 % » d'un réglage numérique. */
  protected defaultNumber(row: RowDef): string {
    const def = this.defaults();
    if (!def || !row.path) return '';
    const text = displayNumber(row, getPath(def, this.path(row)) as number);
    return `défaut ${text}${row.unit ? ' ' + row.unit : ''}`;
  }

  protected defaultText(path: string, format: (v: never) => string): string {
    const def = this.defaults();
    return def ? `défaut ${format(getPath(def, path) as never)}` : '';
  }

  protected readonly onOff = (v: boolean) => (v ? 'activé' : 'désactivé');
  protected readonly plain = (v: string | number) => String(v);
  protected readonly tokS = (v: number) => `${decimal(v, 1)} tok/s`;

  // ── Modification de la copie ─────────────────────────────────────────────────────────────

  constructor() {
    if (!this.draft()) {
      void this.store.load().then(() => this.draft.set(this.copy(this.store.settings())));
    }
    void this.calibration.load();
    this.tauri
      .getDefaultSettings()
      .then((d) => this.defaults.set(d))
      .catch(() => undefined);
    this.tauri
      .hasHfToken()
      .then((t) => this.hasToken.set(t))
      .catch(() => this.hasToken.set(null));
    this.tauri
      .getCacheInfo()
      .then((c) => this.cache.set(c))
      .catch(() => undefined);
    this.tauri
      .ping()
      .then((p) => this.appVersion.set(p.appVersion))
      .catch(() => undefined);
  }

  private copy(s: Settings | null): Settings | null {
    return s ? structuredClone(s) : null;
  }

  protected set(path: string, value: unknown): void {
    this.draft.update((d) => d && setPath(d, path, value));
  }

  /** Réglage numérique saisi dans son unité d'affichage. */
  protected setNumber(row: RowDef, raw: string): void {
    const x = parseNumber(raw);
    if (x === null || x < 0) return;
    const value = x / (row.scale ?? 1);
    this.set(this.path(row), row.integer ? Math.round(value) : value);
  }

  /** « rétablir » : la valeur par défaut revient dans la copie. */
  protected resetPath(path: string): void {
    const def = this.defaults();
    if (def) this.set(path, structuredClone(getPath(def, path)));
  }

  protected setWeight(change: WeightChange): void {
    this.set(`useCases.${this.useCase()}.weights.${change.key}`, change.value);
  }

  protected weightsOf(settings: Settings | null): ScoreWeights | null {
    return settings ? settings.useCases[this.useCase()].weights : null;
  }

  protected slowValue(): number {
    return this.value<number>(`useCases.${this.useCase()}.slowTokS`);
  }

  protected setSlow(raw: string): void {
    const x = parseNumber(raw);
    if (x !== null && x >= 0) this.set(`useCases.${this.useCase()}.slowTokS`, x);
  }

  /** − / + et flèches : un tok/s de moins ou de plus. */
  protected stepSlow(step: number, event?: Event): void {
    event?.preventDefault();
    const next = Math.max(0, Math.round((this.slowValue() + step) * 10) / 10);
    this.set(`useCases.${this.useCase()}.slowTokS`, next);
  }

  /** Les efficacités proposées par les mesures (appliquées avec « Enregistrer »). */
  protected applyCalibration(): void {
    const cal = this.calibration.calibration();
    if (cal) this.draft.update((d) => d && { ...d, estimate: withCalibration(d.estimate, cal) });
  }

  /** Bouton du pied : la section affichée (ou le cas d'usage) reprend ses valeurs par défaut. */
  protected readonly resetLabel = computed(() => {
    if (this.searching()) return null;
    switch (this.section()) {
      case 'estimate':
        return 'Rétablir l’estimation';
      case 'useCases':
        return `Rétablir « ${USE_CASES.find((u) => u.value === this.useCase())?.label} »`;
      case 'catalog':
        return 'Rétablir le catalogue';
      default:
        return null;
    }
  });

  protected resetSection(): void {
    const def = this.defaults();
    if (!def) return;
    if (this.section() === 'useCases') {
      this.resetPath(`useCases.${this.useCase()}`);
      return;
    }
    for (const r of ROWS) {
      if (r.section === this.section() && r.path) this.resetPath(r.path);
    }
  }

  protected close(): void {
    this.dialogRef.close();
  }

  protected async save(): Promise<void> {
    const d = this.draft();
    if (!d) return;
    this.saving.set(true);
    this.error.set(null);
    try {
      await this.store.save(d);
      this.dialogRef.close(true);
      this.toast.show('Paramètres enregistrés.', { kind: 'success' });
    } catch (e) {
      this.error.set(errorMessage(e));
    } finally {
      this.saving.set(false);
    }
  }

  // ── Actions immédiates : token, cache, mises à jour ──────────────────────────────────────

  protected readonly cacheSummary = computed(() => {
    const c = this.cache();
    if (!c) return '…';
    if (c.files === 0) return 'vide';
    return `${int(c.files)} fichier${c.files > 1 ? 's' : ''} · ${fileSize(c.bytes)}`;
  });

  /** Le token est envoyé à Rust, qui le range dans le trousseau du système. */
  protected async saveToken(): Promise<void> {
    const token = this.token().trim();
    if (!token) return;
    await this.tokenAction(async () => {
      await this.tauri.setHfToken(token);
      this.hasToken.set(true);
      this.toast.show('Token Hugging Face enregistré.', { kind: 'success' });
    });
  }

  protected async clearToken(): Promise<void> {
    await this.tokenAction(async () => {
      await this.tauri.clearHfToken();
      this.hasToken.set(false);
    });
  }

  private async tokenAction(action: () => Promise<void>): Promise<void> {
    this.tokenBusy.set(true);
    try {
      await action();
    } catch (e) {
      this.toast.show(`Token : ${errorMessage(e)}`, { kind: 'error' });
    } finally {
      // Le champ est vidé dans tous les cas : le token ne reste pas affiché.
      this.token.set('');
      this.tokenBusy.set(false);
    }
  }

  protected async clearCache(): Promise<void> {
    this.cacheBusy.set(true);
    try {
      const before = this.cache()?.bytes ?? 0;
      const after = await this.tauri.clearCache();
      this.cache.set(after);
      const freed = fileSize(Math.max(before - after.bytes, 0));
      this.toast.show(`Cache vidé (${freed} libérés).`, { kind: 'success' });
    } catch (e) {
      this.toast.show(errorMessage(e), { kind: 'error' });
    } finally {
      this.cacheBusy.set(false);
    }
  }

  protected readonly version = computed(() => this.updates.currentVersion() ?? this.appVersion());

  protected checkUpdate(): void {
    void this.updates.check();
  }

  /** Dialogue « Mise à jour » par-dessus les paramètres (notes, installation). */
  protected openUpdate(): void {
    this.updateLauncher.open();
  }

  protected openReleases(): void {
    this.open(RELEASES_URL);
  }

  protected openHuggingFace(): void {
    this.open(HF_URL);
  }

  private open(url: string): void {
    this.tauri.openUrl(url).catch((e: unknown) => {
      this.toast.show(`Ouverture impossible : ${errorMessage(e)}`, { kind: 'error' });
    });
  }
}
