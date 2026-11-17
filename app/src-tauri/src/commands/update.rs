//! Mises à jour de GRAM depuis les releases GitHub : le manifeste `latest.json` de la dernière
//! release (adresse et clé publique dans `tauri.conf.json`). Toute la logique est dans le crate
//! `gram-update`, partagé avec GRAM Harness ; la WebView n'a aucune permission sur le plugin.

use crate::error::CmdResult;
use gram_update::{UpdateCheck, UpdateState};
use tauri::{AppHandle, Runtime, State};

/// Lit le manifeste de la dernière release.
#[tauri::command]
pub async fn check_update<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, UpdateState>,
) -> CmdResult<UpdateCheck> {
    Ok(gram_update::check(&app, &state).await?)
}

/// Télécharge la mise à jour trouvée par `check_update` (ou cherchée à nouveau), vérifie sa
/// signature (événements `update-progress`), l'installe et relance l'application. Ne revient
/// qu'en cas d'erreur.
#[tauri::command]
pub async fn install_update<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, UpdateState>,
) -> CmdResult<()> {
    Ok(gram_update::install(&app, &state).await?)
}
