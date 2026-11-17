//! Application Tauri de GRAM Harness : discuter en local avec les modèles installés dans
//! Ollama, en premier ceux que GRAM y a ajoutés.
//!
//! Ollama fait foi : la liste des modèles vient de son API locale ; le manifeste que GRAM écrit
//! dans le dossier partagé (`models.json`) ne fait que l'enrichir (nom lisible, contexte,
//! vitesses). Aucune commande système n'est lancée : tout passe par l'API d'Ollama.
//!
//! GRAM ouvre un modèle par le lien profond `gram-harness://open?model=…` ([`link`]) ; à chaque
//! démarrage, GRAM Harness lui dit qu'il est installé (`harness.json`).

pub mod chat;
mod commands;
pub mod conversations;
mod error;
pub mod images;
pub mod link;
pub mod models;
mod state;

#[cfg(test)]
mod tests;

use state::HarnessState;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_deep_link::DeepLinkExt;
use tracing_subscriber::EnvFilter;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    tauri::Builder::default()
        // En premier : un deuxième lancement (lien profond ouvert par Windows ou Linux) passe ses
        // arguments à la première instance, qui les donne au plugin deep-link (`on_open_url`),
        // puis s'arrête. Sans lien (double-clic), la fenêtre revient au premier plan.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            link::show_window(app)
        }))
        // Schéma `gram-harness` (tauri.conf.json) ; utilisé depuis Rust seulement.
        .plugin(tauri_plugin_deep_link::init())
        // Presse-papiers (permission d'écriture seule dans capabilities/default.json) ; page de
        // téléchargement d'Ollama ouverte depuis Rust, liens des réponses depuis la WebView
        // (http et https seulement).
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .manage(HarnessState::production())
        .setup(|app| {
            setup(app.handle());
            Ok(())
        })
        .invoke_handler(commands::handler())
        .run(tauri::generate_context!())
        .expect("erreur au lancement de GRAM Harness");
}

/// Présence pour GRAM, enregistrement du schéma en développement, liens profonds.
fn setup<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<HarnessState>();
    match state.shared_dir() {
        Some(dir) => {
            let version = app.package_info().version.to_string();
            if let Err(e) = link::announce(dir, &version) {
                tracing::warn!(
                    "présence de GRAM Harness non écrite dans {} : {e}",
                    dir.display()
                );
            }
        }
        None => tracing::warn!(
            "dossier partagé introuvable : GRAM ne saura pas que GRAM Harness est installé"
        ),
    }

    let deep_link = app.deep_link();
    // Les installeurs enregistrent le schéma ; en développement, il désigne l'exécutable de
    // `target/debug` (clé `HKCU\Software\Classes\gram-harness`).
    #[cfg(all(windows, debug_assertions))]
    if let Err(e) = deep_link.register_all() {
        tracing::warn!("schéma gram-harness non enregistré : {e}");
    }
    let handle = app.clone();
    deep_link.on_open_url(move |event| {
        let urls = event.urls();
        let state = handle.state::<HarnessState>();
        link::receive(
            &handle,
            state.open_requests(),
            urls.iter().map(|u| u.as_str()),
        );
    });
    match deep_link.get_current() {
        Ok(Some(urls)) => {
            link::receive(app, state.open_requests(), urls.iter().map(|u| u.as_str()));
        }
        Ok(None) => {}
        Err(e) => tracing::warn!("lien profond du lancement illisible : {e}"),
    }
}
