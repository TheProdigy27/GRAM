//! Calibration avec Ollama (phase 7 du plan) : mesure réelle d'une quantification installée,
//! comparaison avec l'estimation et recalibrage de la formule de vitesse.
//!
//! Les mesures portent toujours sur la machine réelle (corrections comprises) : une simulation
//! en cours est ignorée.

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;
use chrono::Utc;
use gram_core::calibration::{calibrate, measurement};
use gram_core::estimate::{candidate_variants, estimate};
use gram_core::launch::ollama_pull_command;
use gram_core::runtime::installed_variant;
use gram_core::{
    Calibration, MeasureRequest, Measurement, MemoryBudget, QuantTable, RunMode, Settings, UseCase,
    Workload,
};
use tauri::{AppHandle, Runtime, State};

/// Mesures enregistrées et réglages qu'elles suggèrent, pour la machine réelle et les
/// paramètres actuels.
async fn calibration<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> CmdResult<Calibration> {
    let hw = state.real_profile(app).await?;
    let settings = state.settings();
    let budget = MemoryBudget::from_profile(&hw, &settings.estimate);
    Ok(calibrate(state.measurements(), &budget, &settings.estimate))
}

/// Mesure une quantification déjà installée dans Ollama, au contexte retenu par l'estimation :
/// chargement, génération de 128 tokens, mémoire occupée (`/api/ps`). La mesure est enregistrée
/// (`measurements.json`) et comparée à l'estimation. Quelques secondes à quelques minutes
/// selon la taille du modèle ; une seule mesure à la fois.
#[tauri::command]
pub async fn ollama_measure<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    request: MeasureRequest,
) -> CmdResult<Measurement> {
    let _guard = state
        .measure_lock
        .try_lock()
        .map_err(|_| CommandError::msg("Une mesure est déjà en cours."))?;
    let client = state.ollama().cloned().ok_or_else(|| {
        CommandError::msg(
            "API d'Ollama inaccessible : la variable OLLAMA_HOST désigne une adresse non prise \
             en charge (HTTPS ou hôte inconnu).",
        )
    })?;
    // Liste fraîche : le modèle vient peut-être d'être installé.
    let installed = client.tags().await?;
    state.set_ollama_models(installed.clone()).await;

    let catalog = state.catalog().await?;
    let model = catalog
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

    let hw = state.real_profile(&app).await?;
    let settings = state.settings();
    let params = &settings.estimate;
    let budget = MemoryBudget::from_profile(&hw, params);
    let work = Workload {
        ctx: settings.ctx_for(request.use_case, request.ctx),
        vision: request.use_case == UseCase::Vision,
    };
    let variant = candidate_variants(&model, params, QuantTable::embedded())
        .into_iter()
        .find(|v| v.label == request.label)
        .ok_or_else(|| {
            CommandError::msg(format!(
                "Quantification {} inconnue pour {}.",
                request.label, model.display_name
            ))
        })?;
    let est = estimate(&model, &variant, &budget, work, params);
    if est.mode == RunMode::TooBig {
        return Err(CommandError::msg(format!(
            "{} ne tient pas en mémoire d'après l'estimation : mesure non lancée (le système \
             risquerait de saturer).",
            variant.label
        )));
    }
    let ollama_model = installed_variant(&installed, &model, &variant.repo_id, &variant.label)
        .map(|om| om.name.clone())
        .ok_or_else(|| {
            CommandError::msg(format!(
                "{} n'est pas installé dans Ollama. L'installer avec : {}",
                variant.label,
                ollama_pull_command(&variant)
            ))
        })?;

    let run = gram_ollama::measure(&client, &ollama_model, est.ctx_used).await?;
    let mut m = measurement(&model, &variant, &est, &ollama_model, &run, Utc::now());
    if state.simulation() {
        m.notes.push(
            "Comparée au matériel réel de la machine : la simulation en cours est ignorée.".into(),
        );
    }
    tracing::info!(
        model = %ollama_model,
        measured = m.tok_s,
        estimated = m.estimated_tok_s,
        "mesure enregistrée"
    );
    state.record_measurement(m.clone());
    Ok(m)
}

/// Mesures enregistrées (la plus récente d'abord) et réglages proposés.
#[tauri::command]
pub async fn get_calibration<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> CmdResult<Calibration> {
    calibration(&app, &state).await
}

/// Applique l'efficacité (et le temps fixe par token, s'il a pu être ajusté) proposés par les
/// mesures, enregistre les paramètres et les renvoie.
#[tauri::command]
pub async fn apply_calibration<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> CmdResult<Settings> {
    let cal = calibration(&app, &state).await?;
    let mut settings = state.settings();
    if !cal.apply_to(&mut settings.estimate) {
        return Err(CommandError::msg(
            "Aucune mesure exploitable : rien à appliquer.",
        ));
    }
    let saved = state.save_settings(settings)?;
    tracing::info!(
        efficiency = saved.estimate.efficiency,
        token_overhead_ms = saved.estimate.token_overhead_ms,
        "calibration appliquée"
    );
    Ok(saved)
}

/// Efface les mesures enregistrées (les paramètres ne changent pas).
#[tauri::command]
pub async fn clear_measurements<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> CmdResult<Calibration> {
    state.clear_measurements();
    calibration(&app, &state).await
}
