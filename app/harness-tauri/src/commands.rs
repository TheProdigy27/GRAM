//! Commandes Tauri de GRAM Harness (`#[tauri::command]`).
//!
//! Depuis l'interface (ou la console du WebView en développement) :
//! `window.__TAURI_INTERNALS__.invoke('list_models')`. Les arguments sont en camelCase ; une
//! erreur rejette la promesse avec un message en français.

use crate::error::{CmdResult, CommandError};
use crate::models::{self, ModelList, OllamaState};
use crate::state::HarnessState;
use gram_ollama::OllamaError;
use tauri::ipc::Invoke;
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_opener::OpenerExt;

/// Page de téléchargement d'Ollama (seule adresse que le harness ouvre en phase 4).
pub const OLLAMA_DOWNLOAD_URL: &str = "https://ollama.com/download";

/// Toutes les commandes, pour le runtime réel comme pour le runtime simulé des tests.
pub fn handler<R: Runtime>() -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![list_models, open_ollama_download]
}

/// Modèles avec lesquels discuter et état d'Ollama. Ollama arrêté ou absent n'est pas une
/// erreur : la liste est vide et `ollama` dit quoi faire.
#[tauri::command]
pub async fn list_models(state: State<'_, HarnessState>) -> CmdResult<ModelList> {
    let client = state.require_ollama()?;
    let manifest = state.manifest();
    match models::list_models(client, &manifest, state.shows()).await {
        Ok(models) => Ok(ModelList {
            ollama: OllamaState::Running,
            models,
        }),
        Err(OllamaError::Unreachable(_)) => {
            let ollama = if state.ollama_installed().await {
                OllamaState::Stopped
            } else {
                OllamaState::Absent
            };
            Ok(ModelList {
                ollama,
                models: Vec::new(),
            })
        }
        Err(e) => Err(e.into()),
    }
}

/// Ouvre la page de téléchargement d'Ollama dans le navigateur du système. L'adresse est fixe :
/// la WebView n'a aucune permission d'ouvrir une adresse.
#[tauri::command]
pub fn open_ollama_download<R: Runtime>(app: AppHandle<R>) -> CmdResult<()> {
    app.opener()
        .open_url(OLLAMA_DOWNLOAD_URL, None::<&str>)
        .map_err(|e| {
            CommandError::msg(format!(
                "Navigateur non ouvert ({e}) : aller sur {OLLAMA_DOWNLOAD_URL}"
            ))
        })
}
