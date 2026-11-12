//! Classement et export.

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;
use chrono::{Local, Utc};
use gram_core::export::{to_csv, to_markdown};
use gram_core::{ExportFormat, RankContext, RankRequest, RankResponse};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, Runtime, State, Window};
use tauri_plugin_dialog::DialogExt;

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

/// Classement calculé pour la palette de recherche (modèles trouvés, modèles récents) : comme
/// `rank`, mais la requête n'est pas retenue, l'export reprend donc toujours le classement
/// affiché.
#[tauri::command]
pub async fn search_ranking<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    request: RankRequest,
) -> CmdResult<RankResponse> {
    compute(&app, &state, request).await
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

/// Dialogue « Enregistrer sous » (modal sur la fenêtre), dans Documents par défaut ;
/// `None` si l'utilisateur annule.
async fn ask_export_path<R: Runtime>(
    app: &AppHandle<R>,
    window: &Window<R>,
    format: ExportFormat,
) -> CmdResult<Option<String>> {
    let ext = format.extension();
    let name = format!("classement-gram-{}.{ext}", Local::now().format("%Y-%m-%d"));
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Exporter le classement")
        .set_file_name(name)
        .add_filter(format.label(), &[ext])
        .set_parent(window);
    if let Ok(dir) = app.path().document_dir() {
        dialog = dialog.set_directory(dir);
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    dialog.save_file(move |picked| {
        let _ = tx.send(picked);
    });
    let Some(picked) = rx
        .await
        .map_err(|_| CommandError::msg("Dialogue d'enregistrement interrompu."))?
    else {
        return Ok(None);
    };
    let mut path = picked
        .into_path()
        .map_err(|e| CommandError::msg(format!("Chemin d'export invalide : {e}")))?;
    if path.extension().is_none() {
        path.set_extension(ext);
    }
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// Exporte le dernier classement demandé, en entier (sans limite de lignes), en Markdown ou en
/// CSV. Sans `path`, demande le fichier avec le dialogue « Enregistrer sous ». Renvoie le chemin
/// écrit, ou `null` si l'utilisateur a annulé.
#[tauri::command]
pub async fn export_ranking<R: Runtime>(
    app: AppHandle<R>,
    window: Window<R>,
    state: State<'_, AppState>,
    format: ExportFormat,
    path: Option<String>,
) -> CmdResult<Option<String>> {
    let path = match path {
        Some(p) => p,
        None => match ask_export_path(&app, &window, format).await? {
            Some(p) => p,
            None => return Ok(None),
        },
    };
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
        .map_err(|e| CommandError::msg(format!("Export impossible vers {path} : {e}")))?;
    Ok(Some(path))
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
