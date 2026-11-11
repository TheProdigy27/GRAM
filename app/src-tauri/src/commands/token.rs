//! Token Hugging Face (optionnel : il élargit les quotas). Jamais renvoyé au frontend.

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;
use gram_hub::HfToken;
use tauri::State;

/// Enregistre le token dans le trousseau du système (espaces retirés).
#[tauri::command]
pub async fn set_hf_token(state: State<'_, AppState>, token: String) -> CmdResult<()> {
    let token = token.trim();
    if token.is_empty() {
        return Err(CommandError::msg("Token Hugging Face vide."));
    }
    state.tokens().set(token).map_err(CommandError::Message)
}

#[tauri::command]
pub async fn clear_hf_token(state: State<'_, AppState>) -> CmdResult<()> {
    state.tokens().clear().map_err(CommandError::Message)
}

#[tauri::command]
pub async fn has_hf_token(state: State<'_, AppState>) -> CmdResult<bool> {
    Ok(state
        .tokens()
        .get()
        .map_err(CommandError::Message)?
        .is_some_and(|t| HfToken::new(t).is_some()))
}
