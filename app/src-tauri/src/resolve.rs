//! Résolution commune à la mesure Ollama et à l'ajout à GRAM Harness : le modèle du catalogue,
//! la quantification demandée et son estimation sur la machine réelle (corrections comprises,
//! simulation ignorée), avec les paramètres actuels.

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;
use gram_core::estimate::{candidate_variants, estimate};
use gram_core::{
    Estimate, HardwareProfile, MemoryBudget, ModelEntry, QuantTable, QuantVariant, Settings,
    UseCase, Workload,
};
use tauri::{AppHandle, Runtime};

/// Une quantification du catalogue, estimée sur la machine réelle.
pub struct Resolved {
    pub model: ModelEntry,
    pub variant: QuantVariant,
    /// Au contexte souhaité (`ctx`, sinon celui du cas d'usage), réduit si besoin pour tenir
    /// en mémoire : c'est le contexte mesuré, et celui que règle le modèle dérivé.
    pub estimate: Estimate,
    pub settings: Settings,
    /// La machine réelle : corrections comprises, simulation ignorée.
    pub hardware: HardwareProfile,
}

/// Modèle `repo_id` (dépôt canonique), quantification `label`, pour le cas d'usage
/// `use_case` et le contexte souhaité `ctx`.
pub async fn resolve<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    repo_id: &str,
    label: &str,
    use_case: UseCase,
    ctx: Option<u32>,
) -> CmdResult<Resolved> {
    let catalog = state.catalog().await?;
    let model = catalog
        .models
        .iter()
        .find(|m| m.repo_id == repo_id)
        .cloned()
        .ok_or_else(|| CommandError::msg(format!("Modèle « {repo_id} » absent du catalogue.")))?;
    drop(catalog);

    let hardware = state.real_profile(app).await?;
    let settings = state.settings();
    let params = &settings.estimate;
    let budget = MemoryBudget::from_profile(&hardware, params);
    let work = Workload {
        ctx: settings.ctx_for(use_case, ctx),
        vision: use_case == UseCase::Vision,
    };
    let variant = candidate_variants(&model, params, QuantTable::embedded())
        .into_iter()
        .find(|v| v.label == label)
        .ok_or_else(|| {
            CommandError::msg(format!(
                "Quantification {label} inconnue pour {}.",
                model.display_name
            ))
        })?;
    let estimate = estimate(&model, &variant, &budget, work, params);
    Ok(Resolved {
        model,
        variant,
        estimate,
        settings,
        hardware,
    })
}
