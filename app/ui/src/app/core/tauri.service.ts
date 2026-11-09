import { Injectable } from '@angular/core';
import { invoke } from '@tauri-apps/api/core';
import type { PingResponse } from './generated/PingResponse';

/**
 * Seul point d'accès à l'IPC Tauri (`invoke` / `listen`).
 * Les types viennent de `./generated`, produits par ts-rs depuis le code Rust.
 */
@Injectable({ providedIn: 'root' })
export class TauriService {
  ping(): Promise<PingResponse> {
    return invoke<PingResponse>('ping');
  }
}
