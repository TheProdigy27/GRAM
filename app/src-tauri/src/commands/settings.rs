//! Paramètres utilisateur (§12).

use crate::error::CmdResult;
use crate::state::AppState;
use gram_core::Settings;
use tauri::State;

/// Valeurs par défaut du §12 : bouton « Réinitialiser » et rappel « défaut : … » des champs.
#[tauri::command]
pub fn get_default_settings() -> Settings {
    Settings::default()
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(state.settings())
}

/// Enregistre les paramètres dans `settings.json` (dossier de l'application) et les renvoie,
/// ramenés dans leurs bornes.
#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    state.save_settings(settings)
}
