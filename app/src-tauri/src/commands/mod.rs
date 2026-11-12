//! Commandes Tauri (`#[tauri::command]`), regroupées par domaine (§5.3 du plan).
//!
//! Depuis le frontend (ou la console du WebView en développement) :
//! `window.__TAURI_INTERNALS__.invoke('rank', { request: { useCase: 'code', ctx: 16384 } })`.
//! Les arguments sont en camelCase ; une erreur rejette la promesse avec un message en français.

pub mod calibration;
pub mod catalog;
pub mod hardware;
pub mod ranking;
pub mod settings;
pub mod system;
pub mod token;
pub mod update;

use tauri::Runtime;
use tauri::ipc::Invoke;

/// Toutes les commandes, pour le runtime réel comme pour le runtime simulé des tests.
pub fn handler<R: Runtime>() -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        system::ping,
        hardware::get_hardware,
        hardware::rescan_hardware,
        hardware::set_hardware_overrides,
        hardware::hardware_options,
        hardware::detect_runtimes,
        catalog::sync_catalog,
        catalog::get_model_detail,
        catalog::get_cache_info,
        catalog::clear_cache,
        ranking::rank,
        ranking::search_ranking,
        ranking::export_ranking,
        settings::get_settings,
        settings::get_default_settings,
        settings::save_settings,
        token::set_hf_token,
        token::clear_hf_token,
        token::has_hf_token,
        calibration::ollama_measure,
        calibration::get_calibration,
        calibration::apply_calibration,
        calibration::clear_measurements,
        update::check_update,
        update::install_update,
    ]
}
