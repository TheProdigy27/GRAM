//! Application Tauri de GRAM : commandes IPC, état partagé et événements.
//!
//! Toute la logique reste côté Rust ; le frontend Angular affiche les données et
//! envoie des filtres.

mod commands;

use tracing_subscriber::EnvFilter;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::system::ping])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application Tauri");
}
