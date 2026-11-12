//! Événements envoyés au frontend (§5.3 du plan), à écouter avec `listen` de
//! `@tauri-apps/api/event`.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

/// Détection du matériel terminée (au démarrage ou après `rescan_hardware`) :
/// `HardwareProfile`.
pub const HARDWARE_READY: &str = "hardware-ready";
/// Progression de `sync_catalog` : `SyncProgress { phase, done, total }`.
pub const SYNC_PROGRESS: &str = "sync-progress";
/// Fin de `sync_catalog` : `SyncSummary`.
pub const SYNC_DONE: &str = "sync-done";
/// Quota restant de l'API Hugging Face : `RateLimitInfo { remaining, resetS }`.
pub const RATE_LIMIT: &str = "rate-limit";
/// Téléchargement d'une mise à jour (`install_update`) : `UpdateProgress { downloaded, total }`.
pub const UPDATE_PROGRESS: &str = "update-progress";
/// Ajout à GRAM Harness en cours (`harness_add`), au plus toutes les 200 ms et à chaque
/// changement d'étape : `InstallProgress`.
pub const HARNESS_PROGRESS: &str = "harness-progress";
/// Fin d'un ajout à GRAM Harness, réussi, annulé ou en échec : `InstallDone`. Permet à une
/// interface rechargée pendant l'ajout d'en connaître l'issue.
pub const HARNESS_DONE: &str = "harness-done";

/// Émet un événement à toutes les fenêtres ; un échec est journalisé, jamais bloquant.
pub fn emit<R: Runtime, T: Serialize + Clone>(app: &AppHandle<R>, event: &str, payload: T) {
    if let Err(e) = app.emit(event, payload) {
        tracing::warn!(event, "événement non émis : {e}");
    }
}
