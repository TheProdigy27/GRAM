//! Application Tauri de GRAM : commandes IPC, état partagé et événements.
//!
//! Toute la logique reste côté Rust ; le frontend Angular affiche les données et
//! envoie des filtres. Au démarrage, le matériel, les runtimes et le catalogue (cache ou
//! catalogue embarqué) sont chargés en parallèle ; `hardware-ready` signale la fin de la
//! détection du matériel. Les mises à jour viennent des releases GitHub (`commands::update`).

mod commands;
mod error;
mod events;
mod state;
mod store;
mod token;

#[cfg(test)]
mod tests;

use commands::update::UpdateState;
use state::AppState;
use tracing_subscriber::EnvFilter;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    tauri::Builder::default()
        // Bouton « Copier » des commandes, pages Hugging Face ouvertes dans le navigateur
        // (permissions restreintes dans capabilities/default.json), dialogue « Enregistrer
        // sous » de l'export et mises à jour (appelés depuis Rust seulement : aucune permission
        // côté WebView).
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState::production())
        .manage(UpdateState::production())
        .setup(|app| {
            tauri::async_runtime::spawn(state::startup(app.handle().clone()));
            Ok(())
        })
        .invoke_handler(commands::handler())
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application Tauri");
}
