//! Application Tauri de GRAM : commandes IPC, état partagé et événements.
//!
//! Toute la logique reste côté Rust ; le frontend Angular affiche les données et
//! envoie des filtres. Au démarrage, le matériel, les runtimes et le catalogue (cache ou
//! catalogue embarqué) sont chargés en parallèle ; `hardware-ready` signale la fin de la
//! détection du matériel.

mod commands;
mod error;
mod events;
mod state;
mod store;
mod token;

#[cfg(test)]
mod tests;

use state::AppState;
use tracing_subscriber::EnvFilter;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    tauri::Builder::default()
        // Bouton « Copier » des commandes, pages Hugging Face ouvertes dans le navigateur
        // (permissions restreintes dans capabilities/default.json), dialogue « Enregistrer
        // sous » de l'export (ouvert depuis Rust seulement : aucune permission côté WebView).
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::production())
        .setup(|app| {
            tauri::async_runtime::spawn(state::startup(app.handle().clone()));
            Ok(())
        })
        .invoke_handler(commands::handler())
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application Tauri");
}
