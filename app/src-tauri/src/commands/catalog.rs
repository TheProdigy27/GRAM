//! Catalogue Hugging Face : synchronisation et fiche détail d'un modèle.

use crate::error::{CmdResult, CommandError};
use crate::events::{self, RATE_LIMIT, SYNC_DONE, SYNC_PROGRESS};
use crate::state::AppState;
use chrono::Utc;
use gram_core::{DetailRequest, ModelDetail, RankContext, SyncSummary};
use gram_hub::{Hub, HubEvent, ReqwestTransport, SyncOptions};
use tauri::{AppHandle, Runtime, State};

/// Client Hugging Face qui relaie la progression et le quota au frontend.
fn hub<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    progress: bool,
) -> CmdResult<Hub<ReqwestTransport>> {
    let app = app.clone();
    Ok(
        Hub::new(ReqwestTransport::new(state.hf_token())?).on_event(move |e| match e {
            HubEvent::Progress(p) if progress => events::emit(&app, SYNC_PROGRESS, p),
            HubEvent::Progress(_) => {}
            HubEvent::RateLimit(r) => events::emit(&app, RATE_LIMIT, r),
        }),
    )
}

/// Synchronise le catalogue (§7.3) : rien à faire si le cache a moins de 24 h et couvre le
/// matériel, sauf avec `force`. Hors ligne ou si Hugging Face est injoignable : cache, puis
/// catalogue embarqué (signalé dans `warnings`). Émet `sync-progress`, `rate-limit`, puis
/// `sync-done`.
#[tauri::command]
pub async fn sync_catalog<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    force: bool,
) -> CmdResult<SyncSummary> {
    let _guard = state
        .sync_lock
        .try_lock()
        .map_err(|_| CommandError::msg("Une synchronisation est déjà en cours."))?;
    let hw = state.hardware(&app).await?;
    let settings = state.settings();
    let opts = SyncOptions {
        force,
        offline: !state.network(),
        max_params: Some(gram_hub::max_params_for(&hw)),
        min_downloads: settings.min_downloads,
        ..SyncOptions::default()
    };
    let hub = hub(&app, &state, true)?;
    let out = gram_hub::sync(&hub, state.cache(), &opts).await?;
    state
        .replace_catalog(out.catalog.models, out.summary.source.clone())
        .await;
    events::emit(&app, SYNC_DONE, &out.summary);
    Ok(out.summary)
}

/// Fiche détail (§11.2) : toutes les quantifications estimées et notées, avec les commandes du
/// runtime choisi. À la première ouverture, les tailles réelles et l'architecture sont chargées
/// depuis tous les dépôts du modèle (cache par commit, sinon réseau) ; en cas d'échec, la fiche
/// est construite avec ce qui est connu et l'explique dans `notes`.
#[tauri::command]
pub async fn get_model_detail<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    request: DetailRequest,
) -> CmdResult<ModelDetail> {
    let hw = state.hardware(&app).await?;
    let catalog = state.catalog().await?;
    let mut entry = catalog
        .models
        .iter()
        .find(|m| m.repo_id == request.repo_id)
        .cloned()
        .ok_or_else(|| {
            CommandError::msg(format!(
                "Modèle « {} » absent du catalogue.",
                request.repo_id
            ))
        })?;
    drop(catalog);

    let mut notes = Vec::new();
    if state.network() && !state.is_enriched(&entry.repo_id) {
        let hub = hub(&app, &state, false)?;
        match gram_hub::enrich(&hub, state.cache(), &entry).await {
            Ok(e) => {
                entry = e;
                state.update_model(entry.clone()).await;
            }
            Err(e) => notes.push(format!("Fiche incomplète : {e}.")),
        }
    }

    let runtimes = state.known_runtimes().await;
    let settings = state.settings();
    let cx = RankContext::new(&hw, runtimes.as_ref(), &settings, Utc::now());
    let mut detail = gram_core::model_detail(&entry, &request, &cx);
    detail.notes.splice(0..0, notes);
    Ok(detail)
}
