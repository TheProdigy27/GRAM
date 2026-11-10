//! Classement et export.

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;
use chrono::{Local, Utc};
use gram_core::export::{to_csv, to_markdown};
use gram_core::{ExportFormat, RankContext, RankRequest, RankResponse};
use std::path::PathBuf;
use tauri::{AppHandle, Runtime, State};

/// Calcule le classement en Rust (§9) : quelques centaines de modèles, quelques dizaines de
/// millisecondes. Le frontend applique un debounce sur le curseur de contexte.
async fn compute<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    request: RankRequest,
) -> CmdResult<RankResponse> {
    let hw = state.hardware(app).await?;
    let catalog = state.catalog().await?;
    let runtimes = state.known_runtimes().await;
    let settings = state.settings();
    Ok(tauri::async_runtime::spawn_blocking(move || {
        let cx = RankContext::new(&hw, runtimes.as_ref(), &settings, Utc::now());
        gram_core::rank(&catalog.models, &request, &cx, catalog.source.clone())
    })
    .await?)
}

/// Classement des modèles pour la machine (ou la simulation en cours).
#[tauri::command]
pub async fn rank<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    request: RankRequest,
) -> CmdResult<RankResponse> {
    state.remember_rank(&request);
    let response = compute(&app, &state, request).await?;
    state.note_rank_served();
    Ok(response)
}

/// Chemin d'export accepté : absolu, avec l'extension du format (évite d'écraser un fichier
/// d'un autre type par erreur).
fn export_path(path: &str, format: ExportFormat) -> CmdResult<PathBuf> {
    let p = PathBuf::from(path);
    if !p.is_absolute() {
        return Err(CommandError::msg(format!(
            "Chemin d'export invalide (absolu attendu) : {path}"
        )));
    }
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    if ext.as_deref() != Some(format.extension()) {
        return Err(CommandError::msg(format!(
            "Le fichier d'export doit avoir l'extension .{}",
            format.extension()
        )));
    }
    Ok(p)
}

/// Exporte le dernier classement demandé, en entier (sans limite de lignes), en Markdown ou en
/// CSV.
#[tauri::command]
pub async fn export_ranking<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    format: ExportFormat,
    path: String,
) -> CmdResult<()> {
    let target = export_path(&path, format)?;
    let request = RankRequest {
        limit: None,
        ..state.last_rank()
    };
    let response = compute(&app, &state, request).await?;
    let text = match format {
        ExportFormat::Md => {
            let hw = state.hardware(&app).await?;
            to_markdown(&response, &hw, Local::now().fixed_offset())
        }
        ExportFormat::Csv => to_csv(&response),
    };
    tauri::async_runtime::spawn_blocking(move || std::fs::write(&target, text))
        .await?
        .map_err(|e| CommandError::msg(format!("Export impossible vers {path} : {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chemins_d_export() {
        let abs = if cfg!(windows) {
            r"C:\Users\x\classement"
        } else {
            "/home/x/classement"
        };
        assert!(export_path(&format!("{abs}.md"), ExportFormat::Md).is_ok());
        assert!(export_path(&format!("{abs}.CSV"), ExportFormat::Csv).is_ok());
        assert!(export_path(&format!("{abs}.csv"), ExportFormat::Md).is_err());
        assert!(export_path(&format!("{abs}.exe"), ExportFormat::Csv).is_err());
        assert!(export_path(abs, ExportFormat::Md).is_err());
        assert!(export_path("classement.md", ExportFormat::Md).is_err());
    }
}
