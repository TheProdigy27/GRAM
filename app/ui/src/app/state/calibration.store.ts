import { Injectable, effect, inject, signal, untracked } from '@angular/core';
import type { Calibration } from '../core/generated/Calibration';
import type { MeasureRequest } from '../core/generated/MeasureRequest';
import type { Measurement } from '../core/generated/Measurement';
import type { Settings } from '../core/generated/Settings';
import { TauriService, errorMessage } from '../core/tauri.service';
import { HardwareStore } from './hardware.store';
import { SettingsStore } from './settings.store';

/** Quantification d'un modèle du catalogue. */
export interface VariantKey {
  repoId: string;
  label: string;
}

/** Erreur de la dernière mesure, pour la quantification concernée. */
export interface MeasureError extends VariantKey {
  message: string;
}

/**
 * Mesures réelles avec Ollama (phase 7) et réglages de vitesse qu'elles suggèrent. Tout est
 * calculé en Rust ; ce store lance les mesures (une à la fois) et garde le dernier état.
 */
@Injectable({ providedIn: 'root' })
export class CalibrationStore {
  private readonly tauri = inject(TauriService);
  private readonly settings = inject(SettingsStore);
  private readonly hardware = inject(HardwareStore);

  readonly calibration = signal<Calibration | null>(null);
  /** Quantification en cours de mesure. */
  readonly measuring = signal<VariantKey | null>(null);
  readonly error = signal<MeasureError | null>(null);
  readonly applying = signal(false);

  private loaded = false;

  constructor() {
    // La proposition dépend des paramètres (efficacités actuelles) et du matériel (bandes
    // passantes corrigées) : relue quand ils changent.
    effect(() => {
      this.settings.version();
      this.hardware.version();
      untracked(() => {
        if (this.loaded) void this.refresh().catch(() => undefined);
      });
    });
  }

  /** Premier chargement (à l'ouverture d'une fiche) ; sans effet ensuite. */
  async load(): Promise<void> {
    if (this.loaded) return;
    this.loaded = true;
    await this.refresh().catch(() => {
      this.loaded = false;
    });
  }

  /** Dernière mesure d'une quantification, tous contextes confondus. */
  latest(repoId: string, label: string): Measurement | undefined {
    return this.calibration()?.measurements.find((m) => m.repoId === repoId && m.label === label);
  }

  /** Lance une mesure ; l'erreur éventuelle est gardée dans `error` (et la promesse rejetée). */
  async measure(request: MeasureRequest): Promise<Measurement> {
    if (this.measuring()) throw new Error('Une mesure est déjà en cours.');
    const key = { repoId: request.repoId, label: request.label };
    this.measuring.set(key);
    this.error.set(null);
    try {
      const m = await this.tauri.ollamaMeasure(request);
      this.loaded = true;
      await this.refresh();
      return m;
    } catch (e) {
      this.error.set({ ...key, message: errorMessage(e) });
      throw e;
    } finally {
      this.measuring.set(null);
    }
  }

  /** Applique les réglages proposés : le classement et la fiche sont recalculés. */
  async apply(): Promise<Settings> {
    this.applying.set(true);
    try {
      const saved = await this.tauri.applyCalibration();
      // Relit aussi la calibration (effet sur `settings.version`).
      this.settings.replace(saved);
      return saved;
    } finally {
      this.applying.set(false);
    }
  }

  async clear(): Promise<void> {
    this.calibration.set(await this.tauri.clearMeasurements());
    this.error.set(null);
  }

  private async refresh(): Promise<void> {
    this.calibration.set(await this.tauri.getCalibration());
  }
}
