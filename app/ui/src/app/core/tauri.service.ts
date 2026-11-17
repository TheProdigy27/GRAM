import { Injectable } from '@angular/core';
import { invoke, type InvokeArgs } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { openUrl } from '@tauri-apps/plugin-opener';
import type { CacheInfo } from './generated/CacheInfo';
import type { Calibration } from './generated/Calibration';
import type { DetailRequest } from './generated/DetailRequest';
import type { AddOutcome } from './generated/AddOutcome';
import type { ExportFormat } from './generated/ExportFormat';
import type { HardwareOptions } from './generated/HardwareOptions';
import type { HardwareProfile } from './generated/HardwareProfile';
import type { HarnessAddRequest } from './generated/HarnessAddRequest';
import type { HarnessStatus } from './generated/HarnessStatus';
import type { InstallDone } from './generated/InstallDone';
import type { InstallProgress } from './generated/InstallProgress';
import type { MeasureRequest } from './generated/MeasureRequest';
import type { Measurement } from './generated/Measurement';
import type { ModelDetail } from './generated/ModelDetail';
import type { Overrides } from './generated/Overrides';
import type { PingResponse } from './generated/PingResponse';
import type { Preflight } from './generated/Preflight';
import type { RankRequest } from './generated/RankRequest';
import type { RankResponse } from './generated/RankResponse';
import type { RateLimitInfo } from './generated/RateLimitInfo';
import type { RemoveOutcome } from './generated/RemoveOutcome';
import type { RuntimesStatus } from './generated/RuntimesStatus';
import type { Settings } from './generated/Settings';
import type { SyncProgress } from './generated/SyncProgress';
import type { SyncSummary } from './generated/SyncSummary';
import type { UpdateCheck } from './generated/UpdateCheck';
import type { UpdateProgress } from './generated/UpdateProgress';

/** Événements émis par le backend (`app/src-tauri/src/events.rs`) et leur charge utile. */
export interface GramEvents {
  'hardware-ready': HardwareProfile;
  'sync-progress': SyncProgress;
  'sync-done': SyncSummary;
  'rate-limit': RateLimitInfo;
  'update-progress': UpdateProgress;
  'harness-progress': InstallProgress;
  'harness-done': InstallDone;
}

/** Message lisible d'une erreur : le backend rejette `invoke` avec une chaîne en français. */
export function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === 'string') return e;
  return String(e);
}

/**
 * Seul point d'accès à l'IPC Tauri (`invoke`, `listen`) et aux plugins (presse-papiers,
 * ouverture d'URL). Les types viennent de `./generated`, produits par ts-rs depuis le code Rust.
 * Toutes les promesses rejetées le sont avec une `Error` dont le message est en français.
 */
@Injectable({ providedIn: 'root' })
export class TauriService {
  ping(): Promise<PingResponse> {
    return this.call('ping');
  }

  // ── Matériel et runtimes ─────────────────────────────────────────────────────────────────

  getHardware(): Promise<HardwareProfile> {
    return this.call('get_hardware');
  }

  rescanHardware(): Promise<HardwareProfile> {
    return this.call('rescan_hardware');
  }

  setHardwareOverrides(overrides: Partial<Overrides>): Promise<HardwareProfile> {
    return this.call('set_hardware_overrides', { overrides });
  }

  /** Matériel tel que détecté (sans corrections) et GPU simulables : dialogue « Corriger… ». */
  hardwareOptions(): Promise<HardwareOptions> {
    return this.call('hardware_options');
  }

  detectRuntimes(): Promise<RuntimesStatus> {
    return this.call('detect_runtimes');
  }

  // ── Catalogue et classement ──────────────────────────────────────────────────────────────

  syncCatalog(force: boolean): Promise<SyncSummary> {
    return this.call('sync_catalog', { force });
  }

  rank(request: RankRequest): Promise<RankResponse> {
    return this.call('rank', { request });
  }

  /**
   * Classement pour la palette de recherche : comme `rank`, sans devenir le classement exporté
   * (le backend retient la dernière requête de `rank` pour `export_ranking`).
   */
  searchRanking(request: RankRequest): Promise<RankResponse> {
    return this.call('search_ranking', { request });
  }

  getModelDetail(request: DetailRequest): Promise<ModelDetail> {
    return this.call('get_model_detail', { request });
  }

  /**
   * Exporte le dernier classement en entier. Sans `path`, le backend ouvre le dialogue
   * « Enregistrer sous ». Renvoie le chemin écrit, ou `null` si l'utilisateur a annulé.
   */
  exportRanking(format: ExportFormat, path?: string): Promise<string | null> {
    return this.call('export_ranking', path === undefined ? { format } : { format, path });
  }

  getCacheInfo(): Promise<CacheInfo> {
    return this.call('get_cache_info');
  }

  clearCache(): Promise<CacheInfo> {
    return this.call('clear_cache');
  }

  // ── Mesures réelles avec Ollama ──────────────────────────────────────────────────────────

  /**
   * Mesure une quantification déjà installée dans Ollama (chargement puis génération de
   * 128 tokens : de quelques secondes à quelques minutes). La mesure est enregistrée.
   */
  ollamaMeasure(request: MeasureRequest): Promise<Measurement> {
    return this.call('ollama_measure', { request });
  }

  /** Mesures enregistrées et réglages de vitesse qu'elles suggèrent. */
  getCalibration(): Promise<Calibration> {
    return this.call('get_calibration');
  }

  /** Applique les réglages proposés par les mesures ; renvoie les paramètres enregistrés. */
  applyCalibration(): Promise<Settings> {
    return this.call('apply_calibration');
  }

  clearMeasurements(): Promise<Calibration> {
    return this.call('clear_measurements');
  }

  // ── Ajout à GRAM Harness ─────────────────────────────────────────────────────────────────

  /** Modèles ajoutés (d'après le manifeste), ajout en cours et présence de GRAM Harness. */
  harnessStatus(): Promise<HarnessStatus> {
    return this.call('harness_status');
  }

  /**
   * Ce que fera l'ajout d'une quantification : taille à télécharger, place sur le disque,
   * contexte réglé, précautions, refus éventuel. Ollama doit répondre.
   */
  harnessPreflight(request: HarnessAddRequest): Promise<Preflight> {
    return this.call('harness_preflight', { request });
  }

  /**
   * Ajoute une quantification à GRAM Harness (événements `harness-progress`, puis
   * `harness-done`) : téléchargement de la base si elle manque, modèle dérivé au contexte de
   * l'estimation, manifeste. `null` si l'ajout est annulé.
   */
  harnessAdd(request: HarnessAddRequest): Promise<AddOutcome | null> {
    return this.call('harness_add', { request });
  }

  /** Annule l'ajout en cours ; `false` s'il n'y en a pas. */
  harnessCancel(): Promise<boolean> {
    return this.call('harness_cancel');
  }

  /**
   * Retire un modèle de GRAM Harness ; avec `removeBase`, supprime aussi sa base d'Ollama si
   * GRAM l'a téléchargée et qu'aucun autre modèle ajouté ne l'utilise.
   */
  harnessRemove(ollamaName: string, removeBase: boolean): Promise<RemoveOutcome> {
    return this.call('harness_remove', { ollamaName, removeBase });
  }

  /**
   * Ouvre un modèle dans GRAM Harness (lien profond ouvert depuis Rust). Rejetée si GRAM Harness
   * n'est pas installé (plus de `harness.json`, ou son exécutable a disparu).
   */
  harnessOpen(ollamaName: string): Promise<void> {
    return this.call('harness_open', { ollamaName });
  }

  // ── Paramètres et token ──────────────────────────────────────────────────────────────────

  getSettings(): Promise<Settings> {
    return this.call('get_settings');
  }

  /** Valeurs par défaut du §12 (bouton « Réinitialiser »). */
  getDefaultSettings(): Promise<Settings> {
    return this.call('get_default_settings');
  }

  saveSettings(settings: Settings): Promise<Settings> {
    return this.call('save_settings', { settings });
  }

  setHfToken(token: string): Promise<void> {
    return this.call('set_hf_token', { token });
  }

  clearHfToken(): Promise<void> {
    return this.call('clear_hf_token');
  }

  hasHfToken(): Promise<boolean> {
    return this.call('has_hf_token');
  }

  // ── Mises à jour (releases GitHub) ───────────────────────────────────────────────────────

  /** Version en cours et mise à jour proposée par la dernière release (`update: null` si à jour). */
  checkUpdate(): Promise<UpdateCheck> {
    return this.call('check_update');
  }

  /**
   * Télécharge la mise à jour (événements `update-progress`), vérifie sa signature, l'installe
   * et relance l'application : la promesse ne se résout pas, sauf erreur.
   */
  installUpdate(): Promise<void> {
    return this.call('install_update');
  }

  // ── Plugins ──────────────────────────────────────────────────────────────────────────────

  /** Copie une commande dans le presse-papiers. */
  copyText(text: string): Promise<void> {
    return writeText(text).catch(rethrow);
  }

  /**
   * Ouvre une page dans le navigateur par défaut (seuls huggingface.co et les releases GitHub de
   * GRAM sont autorisés).
   */
  openUrl(url: string): Promise<void> {
    return openUrl(url).catch(rethrow);
  }

  // ── Fenêtre ──────────────────────────────────────────────────────────────────────────────

  /**
   * Thème de la fenêtre : barre de titre native et `prefers-color-scheme` de la WebView ;
   * `null` : celui du système. Appel direct plutôt que `getCurrentWindow()`, qui alourdirait le
   * bundle : `main` est la seule fenêtre (`tauri.conf.json`).
   */
  setWindowTheme(theme: 'light' | 'dark' | null): Promise<void> {
    return this.call('plugin:window|set_theme', { label: 'main', value: theme });
  }

  // ── Événements ───────────────────────────────────────────────────────────────────────────

  listen<K extends keyof GramEvents>(
    event: K,
    handler: (payload: GramEvents[K]) => void,
  ): Promise<UnlistenFn> {
    return listen<GramEvents[K]>(event, (e) => handler(e.payload));
  }

  private call<T>(cmd: string, args?: InvokeArgs): Promise<T> {
    return invoke<T>(cmd, args).catch(rethrow);
  }
}

function rethrow(e: unknown): never {
  throw e instanceof Error ? e : new Error(errorMessage(e));
}
