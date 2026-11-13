//! Application Tauri de GRAM Harness : discuter en local avec les modèles installés dans
//! Ollama, en premier ceux que GRAM y a ajoutés.
//!
//! Ollama fait foi : la liste des modèles vient de son API locale ; le manifeste que GRAM écrit
//! dans le dossier partagé (`models.json`) ne fait que l'enrichir (nom lisible, contexte,
//! vitesses). Aucune commande système n'est lancée : tout passe par l'API d'Ollama.

mod commands;
mod error;
pub mod models;
mod state;

#[cfg(test)]
mod tests;

use state::HarnessState;
use tracing_subscriber::EnvFilter;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    tauri::Builder::default()
        // Presse-papiers (permission d'écriture seule dans capabilities/default.json) ; page de
        // téléchargement d'Ollama ouverte depuis Rust (aucune permission côté WebView).
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .manage(HarnessState::production())
        .invoke_handler(commands::handler())
        .run(tauri::generate_context!())
        .expect("erreur au lancement de GRAM Harness");
}
