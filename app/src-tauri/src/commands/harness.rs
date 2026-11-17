//! Ajout à GRAM Harness : une quantification du catalogue est installée dans Ollama
//! (téléchargée si besoin), puis reprise par un modèle dérivé `gram/<nom>:<quant>` réglé au
//! contexte calculé par GRAM, et inscrite dans le manifeste partagé (`models.json`) que lit
//! GRAM Harness.
//!
//! Un seul ajout à la fois. Il continue si l'interface est rechargée : `harness_status` donne
//! sa progression, et `harness-done` son issue. Tout passe par l'API locale d'Ollama, aucune
//! commande système n'est lancée.
//!
//! « Ouvrir dans GRAM Harness » (`harness_open`) passe par le lien profond
//! `gram-harness://open?model=…`, ouvert depuis Rust, seulement si GRAM Harness s'est déclaré
//! installé (`harness.json`, qu'il écrit à chaque démarrage).
//!
//! La liste « Modèles installés » de GRAM supprime aussi des modèles d'Ollama qu'il n'a pas
//! ajoutés (`ollama_delete`) ; « Retirer » (`harness_remove`) s'en tient aux siens.

use crate::error::{CmdResult, CommandError};
use crate::events::{self, HARNESS_DONE, HARNESS_PROGRESS};
use crate::resolve::{Resolved, resolve};
use crate::state::AppState;
use chrono::Utc;
use gram_core::harness::{
    ALIAS_PREFIX, PreflightInput, ProgressThrottle, added_warnings, preflight,
};
use gram_core::{HarnessAddRequest, InstallProgress, OllamaModel, Preflight, UseCase};
use gram_link::{
    DeepLink, HarnessPresence, SharedModel, read_manifest, read_presence, same_model,
    update_manifest,
};
use gram_ollama::{Capability, OllamaClient};
use serde::Serialize;
use std::path::Path;
use std::time::Instant;
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;
use ts_rs::TS;

/// Résultat d'un ajout réussi.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AddOutcome {
    /// L'entrée inscrite dans le manifeste.
    pub model: SharedModel,
    /// Ce qu'Ollama a fait autrement que prévu (vision absente, contexte…), en français.
    pub warnings: Vec<String>,
}

/// Un modèle ajouté à GRAM Harness.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HarnessEntry {
    pub model: SharedModel,
    /// « Retirer » peut aussi supprimer sa base d'Ollama : GRAM l'a téléchargée et aucun autre
    /// modèle ajouté ne l'utilise.
    pub base_removable: bool,
}

/// Résultat de `harness_status`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HarnessStatus {
    /// Modèles ajoutés, d'après le manifeste (sans ceux qui ont disparu d'Ollama).
    pub models: Vec<HarnessEntry>,
    /// Ajout en cours.
    pub install: Option<InstallProgress>,
    /// GRAM Harness, s'il est installé : `harness.json` lisible et son exécutable toujours là.
    pub presence: Option<HarnessPresence>,
}

/// Résultat de `harness_remove`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RemoveOutcome {
    /// Le modèle dérivé a été supprimé d'Ollama (`false` s'il en avait déjà disparu).
    pub removed: bool,
    /// Sa base aussi.
    pub base_removed: bool,
}

/// Événement `harness-done` : fin d'un ajout.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InstallDone {
    pub repo_id: String,
    pub display_name: String,
    pub label: String,
    pub ollama_name: String,
    pub result: InstallResult,
}

/// Issue d'un ajout.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum InstallResult {
    Added { warnings: Vec<String> },
    Cancelled,
    Failed { message: String },
}

/// Ce que fera l'ajout : taille à télécharger (0 si la base est déjà dans Ollama), place sur
/// le disque, contexte réglé, précautions et refus éventuel. Ollama doit répondre.
#[tauri::command]
pub async fn harness_preflight<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    request: HarnessAddRequest,
) -> CmdResult<Preflight> {
    let client = state.require_ollama()?;
    let installed = client.tags().await?;
    state.set_ollama_models(installed.clone()).await;
    let r = resolve_request(&app, &state, &request).await?;
    Ok(check(&r, request.use_case, &installed, state.models_dir()))
}

/// Ajoute une quantification à GRAM Harness : téléchargement de la base si elle manque
/// (`harness-progress`), modèle dérivé au contexte de l'estimation, manifeste. Refusé si le
/// modèle est trop gros ou la place insuffisante. `None` si l'ajout est annulé
/// (`harness_cancel`) : le téléchargement s'arrête, Ollama garde ce qu'il a reçu et le prochain
/// ajout reprend.
#[tauri::command]
pub async fn harness_add<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    request: HarnessAddRequest,
) -> CmdResult<Option<AddOutcome>> {
    let _lock = state.install_lock.try_lock().map_err(|_| {
        CommandError::msg(match state.install_progress() {
            Some(p) => format!(
                "Ajout de {} {} déjà en cours : attendre sa fin, ou l'annuler.",
                p.display_name, p.label
            ),
            None => "Un ajout à GRAM Harness est déjà en cours.".into(),
        })
    })?;
    let client = state.require_ollama()?;
    let installed = client.tags().await?;
    state.set_ollama_models(installed.clone()).await;
    let r = resolve_request(&app, &state, &request).await?;
    let plan = check(&r, request.use_case, &installed, state.models_dir());
    if let Some(reason) = &plan.blocked {
        return Err(CommandError::msg(reason.clone()));
    }

    let (cancel_tx, cancel_rx) = oneshot::channel();
    // `install` donne aussitôt la première étape, émise avec les suivantes.
    let guard = state.begin_install(InstallProgress::new(&plan, Default::default()), cancel_tx);
    // Un expéditeur détruit sans envoi n'annule rien.
    let cancel = async move {
        if cancel_rx.await.is_err() {
            std::future::pending::<()>().await;
        }
    };
    let clock = Instant::now();
    let mut throttle = ProgressThrottle::new();
    let outcome = gram_ollama::install(
        &client,
        &plan.base_name,
        &plan.ollama_name,
        plan.ctx,
        |p| {
            if throttle.ready(p.phase, clock.elapsed()) {
                let progress = InstallProgress::new(&plan, p);
                state.set_install_progress(progress.clone());
                events::emit(&app, HARNESS_PROGRESS, progress);
            }
        },
        cancel,
    )
    .await;
    drop(guard);

    let done = |result| InstallDone {
        repo_id: plan.repo_id.clone(),
        display_name: plan.display_name.clone(),
        label: plan.label.clone(),
        ollama_name: plan.ollama_name.clone(),
        result,
    };
    let installed = match outcome {
        Ok(Some(i)) => i,
        Ok(None) => {
            events::emit(&app, HARNESS_DONE, done(InstallResult::Cancelled));
            return Ok(None);
        }
        Err(e) => {
            let gated = r.model.repo_gated(&r.variant.repo_id);
            let message = e.pull_failure().map_or_else(
                || e.to_string(),
                |f| f.message(&r.variant.repo_id, &r.variant.label, gated),
            );
            tracing::warn!(model = %plan.ollama_name, "ajout à GRAM Harness en échec : {e}");
            events::emit(
                &app,
                HARNESS_DONE,
                done(InstallResult::Failed {
                    message: message.clone(),
                }),
            );
            return Err(CommandError::msg(message));
        }
    };

    let measured = state
        .measurements()
        .iter()
        .find(|m| same_model(&m.ollama_model, &installed.base))
        .map(|m| m.tok_s);
    let mut model = SharedModel::new(
        &plan,
        request.use_case,
        installed.base.clone(),
        installed.pulled,
        measured,
        Utc::now(),
    );
    let mut warnings = added_warnings(
        &plan,
        installed.show.has(&Capability::Vision),
        installed.show.num_ctx,
    );
    let entry = model.clone();
    match update_manifest(state.shared_dir(), move |m| {
        let old_base = m.upsert(entry.clone());
        (old_base, m.get(&entry.ollama_name).cloned())
    }) {
        Ok((old_base, saved)) => {
            model = saved.unwrap_or(model);
            if let Some(base) = old_base {
                remove_old_base(&client, &base, &mut warnings).await;
            }
        }
        Err(e) => {
            tracing::warn!("manifeste de GRAM Harness non mis à jour : {e}");
            warnings.push(format!(
                "Modèle ajouté à Ollama, mais models.json n'a pas été mis à jour ({e}) : GRAM \
                 Harness l'affichera sous son nom technique."
            ));
        }
    }
    refresh_ollama_models(&state, &client).await;
    events::emit(
        &app,
        HARNESS_DONE,
        done(InstallResult::Added {
            warnings: warnings.clone(),
        }),
    );
    Ok(Some(AddOutcome { model, warnings }))
}

/// Annule l'ajout en cours ; `false` s'il n'y en a pas.
#[tauri::command]
pub fn harness_cancel(state: State<'_, AppState>) -> bool {
    let cancelled = state.cancel_install();
    if cancelled {
        tracing::info!("annulation de l'ajout à GRAM Harness demandée");
    }
    cancelled
}

/// Retire un modèle de GRAM Harness : supprime le modèle dérivé d'Ollama et son entrée du
/// manifeste ; avec `remove_base`, supprime aussi la base si GRAM l'a téléchargée et qu'aucun
/// autre modèle ajouté ne l'utilise (sinon, elle est gardée).
#[tauri::command]
pub async fn harness_remove(
    state: State<'_, AppState>,
    ollama_name: String,
    remove_base: bool,
) -> CmdResult<RemoveOutcome> {
    let client = state.require_ollama()?;
    if state
        .install_progress()
        .is_some_and(|p| same_model(&p.ollama_name, &ollama_name))
    {
        return Err(CommandError::msg(format!(
            "{ollama_name} est en cours d'ajout : annuler l'ajout d'abord."
        )));
    }
    let manifest = read_manifest(state.shared_dir());
    let entry = manifest.get(&ollama_name);
    // Seuls les modèles dérivés de GRAM : jamais un modèle installé autrement.
    if entry.is_none() && !ollama_name.to_ascii_lowercase().starts_with(ALIAS_PREFIX) {
        return Err(CommandError::msg(format!(
            "{ollama_name} n'a pas été ajouté par GRAM : le supprimer depuis Ollama."
        )));
    }
    let base = entry
        .filter(|e| remove_base && manifest.base_removable(e))
        .map(|e| e.base_name.clone());
    if remove_base && base.is_none() {
        tracing::info!(model = %ollama_name, "base gardée : GRAM ne l'a pas téléchargée, ou elle sert encore");
    }

    let removed = client.delete(&ollama_name).await?;
    if let Err(e) = update_manifest(state.shared_dir(), |m| m.remove(&ollama_name)) {
        tracing::warn!("manifeste de GRAM Harness non mis à jour : {e}");
    }
    let base_removed = match &base {
        Some(b) => client.delete(b).await.map_err(|e| {
            CommandError::msg(format!(
                "{ollama_name} est retiré, mais {b} n'a pas pu être supprimé d'Ollama : {e}"
            ))
        })?,
        None => false,
    };
    tracing::info!(model = %ollama_name, removed, base_removed, "retiré de GRAM Harness");
    refresh_ollama_models(&state, &client).await;
    Ok(RemoveOutcome {
        removed,
        base_removed,
    })
}

/// Supprime un modèle d'Ollama, quelle que soit son origine (liste « Modèles installés ») ;
/// `false` s'il en avait déjà disparu. Ollama n'efface que les couches qu'aucun autre modèle
/// n'utilise. Un modèle dérivé de GRAM sort aussi du manifeste. Refusé pour le modèle dérivé
/// en cours d'ajout.
#[tauri::command]
pub async fn ollama_delete(state: State<'_, AppState>, name: String) -> CmdResult<bool> {
    let client = state.require_ollama()?;
    if state
        .install_progress()
        .is_some_and(|p| same_model(&p.ollama_name, &name))
    {
        return Err(CommandError::msg(format!(
            "{name} est en cours d'ajout : annuler l'ajout d'abord."
        )));
    }
    let removed = client.delete(&name).await?;
    match update_manifest(state.shared_dir(), |m| m.remove(&name).is_some()) {
        Ok(true) => tracing::info!(model = %name, "retiré du manifeste de GRAM Harness"),
        Ok(false) => {}
        Err(e) => tracing::warn!("manifeste de GRAM Harness non mis à jour : {e}"),
    }
    tracing::info!(model = %name, removed, "supprimé d'Ollama");
    refresh_ollama_models(&state, &client).await;
    Ok(removed)
}

/// Modèles ajoutés à GRAM Harness, ajout en cours et présence de GRAM Harness. Quand Ollama
/// répond, les modèles dérivés qui en ont disparu (supprimés avec `ollama rm`…) sont retirés du
/// manifeste.
#[tauri::command]
pub async fn harness_status(state: State<'_, AppState>) -> CmdResult<HarnessStatus> {
    let dir = state.shared_dir();
    if let Some(client) = state.ollama() {
        let listed_at = Utc::now();
        match client.tags().await {
            Ok(installed) => {
                let names: Vec<&str> = installed.iter().map(|m| m.name.as_str()).collect();
                match update_manifest(dir, |m| m.retain_installed(&names, listed_at)) {
                    Ok(0) => {}
                    Ok(n) => {
                        tracing::info!(n, "modèles dérivés disparus d'Ollama, retirés du manifeste")
                    }
                    Err(e) => tracing::warn!("manifeste de GRAM Harness non mis à jour : {e}"),
                }
                state.set_ollama_models(installed).await;
            }
            Err(e) => tracing::debug!("manifeste gardé tel quel : {e}"),
        }
    }
    let manifest = read_manifest(dir);
    let models = manifest
        .models
        .iter()
        .map(|m| HarnessEntry {
            model: m.clone(),
            base_removable: manifest.base_removable(m),
        })
        .collect();
    Ok(HarnessStatus {
        models,
        install: state.install_progress(),
        presence: read_presence(dir),
    })
}

/// Ouvre un modèle dans GRAM Harness : lien profond `gram-harness://open?model=…`, ouvert
/// depuis Rust (la WebView n'a aucune permission pour ce schéma). GRAM Harness choisit le
/// modèle s'il est dans Ollama. Refusé si GRAM Harness n'est pas installé : sous Windows, un
/// schéma inconnu afficherait « Choisir une application » au lieu d'échouer.
#[tauri::command]
pub fn harness_open<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    ollama_name: String,
) -> CmdResult<()> {
    let link = DeepLink::new(ollama_name.trim()).map_err(|e| {
        CommandError::msg(format!(
            "{ollama_name} ne peut pas être ouvert dans GRAM Harness : {e}"
        ))
    })?;
    if read_presence(state.shared_dir()).is_none() {
        return Err(CommandError::msg(
            "GRAM Harness n'est pas installé sur cet ordinateur : le télécharger depuis la page \
             des releases, puis le lancer une fois.",
        ));
    }
    tracing::info!(model = %link.model, "ouverture dans GRAM Harness");
    app.opener()
        .open_url(link.to_url(), None::<&str>)
        .map_err(|e| {
            CommandError::msg(format!(
                "GRAM Harness ne s'est pas ouvert ({e}) : le lancer, puis choisir {}.",
                link.model
            ))
        })
}

async fn resolve_request<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    request: &HarnessAddRequest,
) -> CmdResult<Resolved> {
    resolve(
        app,
        state,
        &request.repo_id,
        &request.label,
        request.use_case,
        request.ctx,
    )
    .await
}

/// Vérifications avant l'ajout, sur la machine réelle.
fn check(
    r: &Resolved,
    use_case: UseCase,
    installed: &[OllamaModel],
    models_dir: Option<&Path>,
) -> Preflight {
    let dir = models_dir.map(|d| d.to_string_lossy().into_owned());
    preflight(&PreflightInput {
        model: &r.model,
        variant: &r.variant,
        estimate: &r.estimate,
        kv_cache: r.settings.estimate.kv_cache,
        slow_tok_s: r.settings.use_cases.get(use_case).slow_tok_s,
        installed,
        disks: &r.hardware.disks,
        models_dir: dir.as_deref(),
    })
}

/// Supprime la base d'une entrée remplacée (GRAM l'avait téléchargée, plus rien ne
/// l'utilise) ; un échec devient un avertissement.
async fn remove_old_base(client: &OllamaClient, base: &str, warnings: &mut Vec<String>) {
    match client.delete(base).await {
        Ok(_) => tracing::info!(base, "ancienne base supprimée d'Ollama"),
        Err(e) => {
            tracing::warn!(base, "ancienne base non supprimée : {e}");
            warnings.push(format!(
                "L'ancienne base {base} n'a pas pu être supprimée d'Ollama ({e}) : la supprimer \
                 depuis Ollama pour libérer la place."
            ));
        }
    }
}

/// Liste des modèles d'Ollama après un ajout ou un retrait (fiche, classement).
async fn refresh_ollama_models(state: &AppState, client: &OllamaClient) {
    match client.tags().await {
        Ok(models) => state.set_ollama_models(models).await,
        Err(e) => tracing::warn!("modèles installés dans Ollama : {e}"),
    }
}
