//! Matériel et runtimes installés.

use crate::error::CmdResult;
use crate::state::AppState;
use gram_core::{HardwareProfile, Overrides, RuntimesStatus};
use tauri::{AppHandle, Runtime, State};

/// Profil matériel, détecté au premier appel (normalement dès le démarrage) puis gardé en
/// mémoire.
#[tauri::command]
pub async fn get_hardware<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> CmdResult<HardwareProfile> {
    state.hardware(&app).await
}

/// Nouvelle détection (bouton « Ré-analyser »), en gardant les corrections.
#[tauri::command]
pub async fn rescan_hardware<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> CmdResult<HardwareProfile> {
    state.rescan(&app).await
}

/// Corrections manuelles et simulation (§6.6) ; renvoie le profil qui en résulte, utilisé par
/// les classements suivants. `Overrides` vide : retour aux valeurs détectées.
#[tauri::command]
pub async fn set_hardware_overrides<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    overrides: Overrides,
) -> CmdResult<HardwareProfile> {
    state.set_overrides(&app, overrides).await
}

/// Ollama, LM Studio et llama.cpp : installés, en cours d'exécution.
#[tauri::command]
pub async fn detect_runtimes(state: State<'_, AppState>) -> CmdResult<RuntimesStatus> {
    state.detect_runtimes().await
}
