//! Commandes Tauri (`#[tauri::command]`), regroupées par domaine (§5.3 du plan).
//!
//! Depuis le frontend (ou la console du WebView en développement) :
//! `window.__TAURI_INTERNALS__.invoke('rank', { request: { useCase: 'code', ctx: 16384 } })`.
//! Les arguments sont en camelCase ; une erreur rejette la promesse avec un message en français.

pub mod catalog;
pub mod hardware;
pub mod ranking;
pub mod settings;
pub mod system;
pub mod token;

use tauri::Runtime;
use tauri::ipc::Invoke;

/// Toutes les commandes, pour le runtime réel comme pour le runtime simulé des tests.
pub fn handler<R: Runtime>() -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        system::ping,
        hardware::get_hardware,
        hardware::rescan_hardware,
        hardware::set_hardware_overrides,
        hardware::detect_runtimes,
        catalog::sync_catalog,
        catalog::get_model_detail,
        ranking::rank,
        ranking::export_ranking,
        settings::get_settings,
        settings::save_settings,
        token::set_hf_token,
        token::clear_hf_token,
        token::has_hf_token,
    ]
}
