import { Injectable } from '@angular/core';
import { invoke, type InvokeArgs } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { openUrl } from '@tauri-apps/plugin-opener';
import type { CacheInfo } from './generated/CacheInfo';
import type { DetailRequest } from './generated/DetailRequest';
import type { ExportFormat } from './generated/ExportFormat';
import type { HardwareOptions } from './generated/HardwareOptions';
import type { HardwareProfile } from './generated/HardwareProfile';
import type { ModelDetail } from './generated/ModelDetail';
import type { Overrides } from './generated/Overrides';
import type { PingResponse } from './generated/PingResponse';
import type { RankRequest } from './generated/RankRequest';
import type { RankResponse } from './generated/RankResponse';
import type { RateLimitInfo } from './generated/RateLimitInfo';
import type { RuntimesStatus } from './generated/RuntimesStatus';
import type { Settings } from './generated/Settings';
import type { SyncProgress } from './generated/SyncProgress';
import type { SyncSummary } from './generated/SyncSummary';

/** Événements émis par le backend (`app/src-tauri/src/events.rs`) et leur charge utile. */
export interface GramEvents {
  'hardware-ready': HardwareProfile;
  'sync-progress': SyncProgress;
  'sync-done': SyncSummary;
  'rate-limit': RateLimitInfo;
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

  // ── Plugins ──────────────────────────────────────────────────────────────────────────────

  /** Copie une commande dans le presse-papiers. */
  copyText(text: string): Promise<void> {
    return writeText(text).catch(rethrow);
  }

  /** Ouvre une page dans le navigateur par défaut (seul huggingface.co est autorisé). */
  openUrl(url: string): Promise<void> {
    return openUrl(url).catch(rethrow);
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
