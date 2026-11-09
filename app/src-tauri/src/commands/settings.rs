//! Paramètres utilisateur (§12).

use crate::error::CmdResult;
use crate::state::AppState;
use gram_core::Settings;
use tauri::State;

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(state.settings())
}

/// Enregistre les paramètres dans `%LOCALAPPDATA%\GRAM\settings.json` et les renvoie, ramenés
/// dans leurs bornes. `Settings` par défaut : bouton « Réinitialiser ».
#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    state.save_settings(settings)
}
