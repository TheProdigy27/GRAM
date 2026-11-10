import { Component, computed, inject, signal } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatCheckboxModule } from '@angular/material/checkbox';
import { MatDialogModule, MatDialogRef } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatSelectModule } from '@angular/material/select';
import { MatSnackBar } from '@angular/material/snack-bar';
import { MatTabsModule } from '@angular/material/tabs';
import type { CacheInfo } from '../../core/generated/CacheInfo';
import type { EstimateParams } from '../../core/generated/EstimateParams';
import type { KvCacheType } from '../../core/generated/KvCacheType';
import type { ScoreWeights } from '../../core/generated/ScoreWeights';
import type { Settings } from '../../core/generated/Settings';
import type { UseCase } from '../../core/generated/UseCase';
import type { UseCaseProfile } from '../../core/generated/UseCaseProfile';
import { TauriService, errorMessage } from '../../core/tauri.service';
import { CTX_STEPS, ctxK, decimal, fileSize, int } from '../../shared/format';
import { USE_CASES } from '../../shared/labels';
import { SettingsStore } from '../../state/settings.store';

const GIB = 2 ** 30;

export const KV_CACHE_TYPES: readonly { value: KvCacheType; label: string }[] = [
  { value: 'f16', label: 'f16 (défaut d’Ollama et de llama.cpp)' },
  { value: 'q8_0', label: 'q8_0 : contexte deux fois plus léger' },
  { value: 'q4_0', label: 'q4_0 : quatre fois plus léger, qualité réduite' },
];

export const WEIGHTS: readonly { key: keyof ScoreWeights; label: string }[] = [
  { key: 'quality', label: 'Qualité' },
  { key: 'speed', label: 'Vitesse' },
  { key: 'fit', label: 'Marge' },
  { key: 'context', label: 'Contexte' },
];

/** Fraction → pourcentage affiché (0,1 → 10). */
export function toPercent(x: number): number {
  return Math.round(x * 1000) / 10;
}

/** Octets → Go affichés, deux décimales au plus. */
export function bytesToGb(bytes: number): number {
  return Math.round((bytes / GIB) * 100) / 100;
}

/** Nombre saisi (virgule acceptée) ; vide ou invalide : `null`, le champ garde sa valeur. */
export function parseNumber(value: string): number | null {
  const text = value.trim().replace(',', '.');
  const x = Number(text);
  return text !== '' && Number.isFinite(x) ? x : null;
}

type NumberKeys<T> = { [K in keyof T]: T[K] extends number ? K : never }[keyof T];

/**
 * Paramètres (§11.2 et §12) : estimation, cas d'usage, catalogue, token Hugging Face. Les
 * réglages sont modifiés sur une copie et enregistrés avec « Enregistrer » (Rust les ramène
 * dans leurs bornes) ; le token et le vidage du cache s'appliquent tout de suite.
 */
@Component({
  selector: 'app-settings-dialog',
  imports: [
    MatButtonModule,
    MatCheckboxModule,
    MatDialogModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatProgressBarModule,
    MatSelectModule,
    MatTabsModule,
  ],
  templateUrl: './settings-dialog.html',
  styleUrl: './settings-dialog.scss',
})
export class SettingsDialog {
  private readonly tauri = inject(TauriService);
  private readonly store = inject(SettingsStore);
  private readonly dialogRef = inject(MatDialogRef<SettingsDialog>);
  private readonly snackBar = inject(MatSnackBar);

  protected readonly useCases = USE_CASES;
  protected readonly gib = GIB;
  protected readonly kvTypes = KV_CACHE_TYPES;
  protected readonly weights = WEIGHTS;
  protected readonly pct = toPercent;
  protected readonly toGb = bytesToGb;
  protected readonly ctxK = ctxK;
  protected readonly decimal = decimal;
  protected readonly int = int;

  /** Copie modifiée, enregistrée avec « Enregistrer ». */
  protected readonly draft = signal<Settings | null>(this.copy(this.store.settings()));
  protected readonly defaults = signal<Settings | null>(null);
  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly hasToken = signal<boolean | null>(null);
  protected readonly token = signal('');
  protected readonly tokenBusy = signal(false);

  protected readonly cache = signal<CacheInfo | null>(null);
  protected readonly cacheBusy = signal(false);

  protected readonly changed = computed(
    () => JSON.stringify(this.draft()) !== JSON.stringify(this.store.settings()),
  );

  /** Contextes proposés par cas d'usage : ceux du curseur, et la valeur actuelle si différente. */
  protected readonly ctxOptions = computed(() => {
    const d = this.draft();
    const values = new Set<number>(CTX_STEPS);
    if (d) for (const u of USE_CASES) values.add(d.useCases[u.value].ctx);
    return [...values].sort((a, b) => a - b);
  });

  protected readonly cacheSummary = computed(() => {
    const c = this.cache();
    if (!c) return '…';
    if (c.files === 0) return 'vide';
    return `${int(c.files)} fichier${c.files > 1 ? 's' : ''} · ${fileSize(c.bytes)}`;
  });

  constructor() {
    if (!this.draft()) {
      void this.store.load().then(() => this.draft.set(this.copy(this.store.settings())));
    }
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
  }

  private copy(s: Settings | null): Settings | null {
    return s ? structuredClone(s) : null;
  }

  // ── Modification de la copie ─────────────────────────────────────────────────────────────

  protected set<K extends keyof Settings>(key: K, value: Settings[K]): void {
    this.draft.update((d) => d && { ...d, [key]: value });
  }

  protected setNumber(key: NumberKeys<Settings>, raw: string): void {
    const x = parseNumber(raw);
    if (x !== null) this.set(key, Math.round(x));
  }

  protected setEstimate<K extends keyof EstimateParams>(key: K, value: EstimateParams[K]): void {
    this.draft.update((d) => d && { ...d, estimate: { ...d.estimate, [key]: value } });
  }

  /** Champ numérique de l'estimation ; `scale` convertit l'unité affichée (% → fraction…). */
  protected setEstimateNumber(key: NumberKeys<EstimateParams>, raw: string, scale = 1): void {
    const x = parseNumber(raw);
    if (x === null) return;
    const value = x * scale;
    this.setEstimate(key, key === 'overheadBytes' || key === 'minCtx' ? Math.round(value) : value);
  }

  protected setUseCase(useCase: UseCase, patch: Partial<UseCaseProfile>): void {
    this.draft.update(
      (d) =>
        d && {
          ...d,
          useCases: { ...d.useCases, [useCase]: { ...d.useCases[useCase], ...patch } },
        },
    );
  }

  protected setSlow(useCase: UseCase, raw: string): void {
    const x = parseNumber(raw);
    if (x !== null) this.setUseCase(useCase, { slowTokS: x });
  }

  protected setWeight(useCase: UseCase, key: keyof ScoreWeights, raw: string): void {
    const x = parseNumber(raw);
    const d = this.draft();
    if (x === null || !d) return;
    this.setUseCase(useCase, { weights: { ...d.useCases[useCase].weights, [key]: x } });
  }

  /** Valeurs par défaut du §12, appliquées avec « Enregistrer ». */
  protected resetDefaults(): void {
    const d = this.defaults();
    if (d) this.draft.set(structuredClone(d));
  }

  protected async save(): Promise<void> {
    const d = this.draft();
    if (!d) return;
    this.saving.set(true);
    this.error.set(null);
    try {
      await this.store.save(d);
      this.dialogRef.close(true);
      this.snackBar.open('Paramètres enregistrés.', undefined, { duration: 3000 });
    } catch (e) {
      this.error.set(errorMessage(e));
    } finally {
      this.saving.set(false);
    }
  }

  // ── Actions immédiates : token et cache ──────────────────────────────────────────────────

  /** Le token est envoyé à Rust, qui le range dans le Gestionnaire d'identifiants Windows. */
  protected async saveToken(): Promise<void> {
    const token = this.token().trim();
    if (!token) return;
    await this.tokenAction(async () => {
      await this.tauri.setHfToken(token);
      this.hasToken.set(true);
      this.snackBar.open('Token Hugging Face enregistré.', undefined, { duration: 3000 });
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
      this.snackBar.open(`Token : ${errorMessage(e)}`, 'OK');
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
      this.snackBar.open(`Cache vidé (${freed} libérés).`, 'OK', {
        duration: 4000,
      });
    } catch (e) {
      this.snackBar.open(errorMessage(e), 'OK');
    } finally {
      this.cacheBusy.set(false);
    }
  }
}
