//! Classement (§9 du plan) et fiche détail d'un modèle.
//!
//! Pour chaque modèle (un modèle de base par ligne, dépôts déjà dédoublonnés par `gram-hub`) :
//!
//! 1. toutes ses quantifications candidates sont estimées (§8) puis notées (§9) ;
//! 2. la meilleure est celle qui a le score maximal parmi celles qui tiennent en mémoire (et
//!    sous la taille maximale demandée), en préférant celles qui atteignent le seuil de lenteur
//!    du cas d'usage : un modèle ne passe dans « Exécutable mais lent » que si aucune de ses
//!    quantifications n'est assez rapide ;
//! 3. une explication courte est générée.
//!
//! Le filtre « Vision » ne garde que les modèles vision ; les autres cas d'usage gardent tous
//! les modèles, en favorisant ceux qui correspondent (les autres sont pénalisés de 8 % pour Code
//! et Raisonnement). À score égal, le plus téléchargé passe devant. Tout est déterministe : même
//! entrée, même classement.

use crate::catalog::DataSource;
use crate::estimate::{
    Estimate, EstimateParams, MemoryBudget, QuantTable, RunMode, VariantEstimate, Workload,
    estimate_variants,
};
use crate::fmt::{ctx_k, gb, gb_value, tok_s};
use crate::hardware::HardwareProfile;
use crate::launch::{LaunchPlan, Runtime, hf_url, launch_plan, ollama_pull_command, runtime_notes};
use crate::model::{ModelEntry, Pipeline, QuantVariant, UseCase};
use crate::runtime::{OllamaModel, RuntimesStatus, installed_for, installed_variant};
use crate::score::{self, Curation, Modifiers, Score, effective_bpw};
use crate::settings::{Settings, UseCaseProfile};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use ts_rs::TS;

/// Commande `rank` : filtres du panneau de gauche.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, optional_fields)]
pub struct RankRequest {
    pub use_case: UseCase,
    /// Contexte souhaité en tokens ; absent : celui du cas d'usage (paramètres).
    pub ctx: Option<u32>,
    /// Runtime des avertissements ; absent : celui des paramètres.
    pub runtime: Option<Runtime>,
    /// Taille maximale d'une quantification, en octets.
    pub max_size: Option<u64>,
    /// Remplit aussi la liste des modèles trop gros.
    pub show_too_big: bool,
    /// Nombre maximal de lignes par liste ; absent : toutes.
    pub limit: Option<u32>,
}

/// Commande `get_model_detail`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, optional_fields)]
pub struct DetailRequest {
    pub repo_id: String,
    pub use_case: UseCase,
    pub ctx: Option<u32>,
    pub runtime: Option<Runtime>,
    pub max_size: Option<u64>,
}

/// Une ligne du classement : un modèle et sa meilleure quantification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RankItem {
    pub repo_id: String,
    pub display_name: String,
    pub base_model: Option<String>,
    pub params_total: u64,
    pub params_active: Option<u64>,
    pub pipeline: Pipeline,
    pub use_cases: Vec<UseCase>,
    pub downloads: u64,
    pub likes: u64,
    pub gated: bool,
    pub created_at: Option<DateTime<Utc>>,
    /// Tailles réelles connues (sinon, échelle de repli estimée).
    pub detailed: bool,
    /// Quantifications évaluées.
    pub variant_count: u32,
    /// Meilleure quantification (la plus petite si le modèle est trop gros).
    pub best: VariantEstimate,
    pub score: Score,
    /// Sous le seuil de lenteur du cas d'usage.
    pub slow: bool,
    /// « Tient entièrement en VRAM (6,7 / 10,8 Go) en Q5_K_M · ~38 tok/s estimés · … »
    pub explanation: String,
    /// Quantifications de ce modèle déjà installées dans Ollama (depuis un de ses dépôts).
    pub installed: Vec<String>,
}

/// Résultat de `rank`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RankResponse {
    pub use_case: UseCase,
    /// Contexte souhaité retenu, en tokens.
    pub ctx: u32,
    pub runtime: Runtime,
    /// Mémoire utilisable et bandes passantes de la machine (ou de la simulation).
    pub budget: MemoryBudget,
    /// Seuil de lenteur du cas d'usage, en tok/s.
    pub slow_tok_s: f32,
    /// Modèles exécutables, du meilleur score au moins bon.
    pub items: Vec<RankItem>,
    /// Nombre de modèles exécutables avant `limit`.
    pub total_items: u32,
    /// « Exécutable mais lent », du meilleur score au moins bon.
    pub slow: Vec<RankItem>,
    pub total_slow: u32,
    /// Modèles trop gros, du plus proche de tenir au plus éloigné (seulement avec
    /// `show_too_big`).
    pub too_big: Vec<RankItem>,
    pub too_big_count: u32,
    pub data_source: DataSource,
    /// Avertissements généraux (runtime, GPU intégré…), en français.
    pub notes: Vec<String>,
}

/// Une quantification dans la fiche détail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct VariantDetail {
    pub variant: QuantVariant,
    pub estimate: Estimate,
    pub score: Score,
    pub slow: bool,
    /// Sous la taille maximale demandée.
    pub within_size: bool,
    pub explanation: String,
    pub launch: LaunchPlan,
    /// Nom du modèle dans Ollama si cette quantification y est déjà installée (bouton
    /// « Mesurer »).
    pub ollama_model: Option<String>,
    /// Commande qui l'installe dans Ollama, pour la mesurer (`ollama pull hf.co/…`).
    pub ollama_pull: String,
}

/// Résultat de `get_model_detail`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ModelDetail {
    pub model: ModelEntry,
    pub use_case: UseCase,
    pub ctx: u32,
    pub runtime: Runtime,
    pub budget: MemoryBudget,
    pub slow_tok_s: f32,
    /// Quantifications candidates, de la plus petite à la plus grosse.
    pub variants: Vec<VariantDetail>,
    /// Indice de la meilleure quantification dans `variants` (celle du classement) ; absent si
    /// aucune ne tient.
    pub best: Option<u32>,
    /// Page Hugging Face du dépôt canonique.
    pub hf_url: String,
    /// Avertissements (runtime, chargement de la fiche), en français.
    pub notes: Vec<String>,
}

/// Ce dont le classement a besoin, en plus du catalogue.
#[derive(Debug, Clone, Copy)]
pub struct RankContext<'a> {
    pub hardware: &'a HardwareProfile,
    /// `None` tant que les runtimes n'ont pas été détectés.
    pub runtimes: Option<&'a RuntimesStatus>,
    pub settings: &'a Settings,
    pub quants: &'a QuantTable,
    pub curation: &'a Curation,
    /// Date de référence de la récence (§9).
    pub now: DateTime<Utc>,
}

impl<'a> RankContext<'a> {
    /// Avec les tables embarquées.
    pub fn new(
        hardware: &'a HardwareProfile,
        runtimes: Option<&'a RuntimesStatus>,
        settings: &'a Settings,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            hardware,
            runtimes,
            settings,
            quants: QuantTable::embedded(),
            curation: Curation::embedded(),
            now,
        }
    }
}

/// Réglages d'une requête, calculés une fois pour tous les modèles.
struct Setup<'a> {
    use_case: UseCase,
    profile: UseCaseProfile,
    work: Workload,
    budget: MemoryBudget,
    params: &'a EstimateParams,
    max_size: Option<u64>,
    cx: &'a RankContext<'a>,
}

impl<'a> Setup<'a> {
    fn new(
        cx: &'a RankContext<'a>,
        use_case: UseCase,
        ctx: Option<u32>,
        max_size: Option<u64>,
    ) -> Self {
        let params = &cx.settings.estimate;
        Self {
            use_case,
            profile: *cx.settings.use_cases.get(use_case),
            work: Workload {
                ctx: cx.settings.ctx_for(use_case, ctx),
                vision: use_case == UseCase::Vision,
            },
            budget: MemoryBudget::from_profile(cx.hardware, params),
            params,
            max_size,
            cx,
        }
    }

    /// Modèles installés dans Ollama (vide tant que les runtimes n'ont pas été détectés).
    fn installed(&self) -> &'a [OllamaModel] {
        self.cx.runtimes.map_or(&[], |r| r.ollama_models.as_slice())
    }

    /// `false` pour un modèle qui ne correspond pas au cas d'usage Code ou Raisonnement.
    fn use_case_match(&self, m: &ModelEntry) -> bool {
        !matches!(self.use_case, UseCase::Code | UseCase::Reasoning)
            || m.use_cases.contains(&self.use_case)
    }
}

/// Une quantification estimée et notée.
struct Row {
    ve: VariantEstimate,
    score: Score,
    slow: bool,
    within_size: bool,
}

impl Row {
    fn runnable(&self) -> bool {
        self.ve.estimate.mode != RunMode::TooBig
    }
}

struct Evaluation {
    rows: Vec<Row>,
    best: Option<usize>,
}

impl Evaluation {
    /// Quantification affichée pour un modèle trop gros : la plus petite sous la taille
    /// maximale (aucune : le modèle est écarté par le filtre de taille).
    fn smallest_within_size(&self) -> Option<usize> {
        (0..self.rows.len())
            .filter(|&i| self.rows[i].within_size)
            .min_by_key(|&i| self.rows[i].ve.variant.size_bytes)
    }
}

fn evaluate(m: &ModelEntry, s: &Setup) -> Evaluation {
    let curation = s.cx.curation.bonus(m);
    let matches = s.use_case_match(m);
    let rows: Vec<Row> = estimate_variants(m, &s.budget, s.work, s.params, s.cx.quants)
        .into_iter()
        .map(|ve| {
            let bpw = effective_bpw(ve.variant.bpw, ve.variant.size_bytes, m.params_total);
            let axes = score::axes(
                m,
                bpw,
                &ve.estimate,
                s.work.ctx,
                curation,
                s.cx.now,
                s.cx.quants,
            );
            let modifiers = Modifiers {
                downloads: m.downloads,
                use_case_match: matches,
                approximate: ve.estimate.approximate,
            };
            Row {
                score: score::score(axes, s.profile.weights, modifiers),
                slow: ve.estimate.tok_s < s.profile.slow_tok_s,
                within_size: s.max_size.is_none_or(|max| ve.variant.size_bytes <= max),
                ve,
            }
        })
        .collect();

    let pick = |allow_slow: bool| {
        (0..rows.len())
            .filter(|&i| {
                let r = &rows[i];
                r.runnable() && r.within_size && (allow_slow || !r.slow)
            })
            // À score égal, la plus petite : même qualité estimée (le facteur de quantification
            // plafonne à 8 bits par poids, la vitesse à 40 tok/s) pour moins de mémoire. Sans
            // cela, BF16 l'emporterait sur Q8_0 pour les petits modèles.
            .max_by(|&a, &b| {
                let (ra, rb) = (&rows[a], &rows[b]);
                ra.score
                    .total
                    .total_cmp(&rb.score.total)
                    .then(rb.ve.variant.size_bytes.cmp(&ra.ve.variant.size_bytes))
            })
    };
    let best = pick(false).or_else(|| pick(true));
    Evaluation { rows, best }
}

/// Explication courte d'une estimation (§9), par exemple « Tient entièrement en VRAM
/// (6,7 / 10,8 Go) en Q5_K_M · ~38 tok/s estimés · contexte 32k possible ».
pub fn explain(
    ve: &VariantEstimate,
    budget: &MemoryBudget,
    ctx_wanted: u32,
    ctx_train: u32,
) -> String {
    let (v, e) = (&ve.variant, &ve.estimate);
    // En mémoire unifiée, pas de VRAM à proprement parler : la part de la RAM que Metal laisse
    // au GPU.
    let (in_vram, on_gpu) = if budget.unified {
        ("dans la mémoire du GPU", "sur le GPU")
    } else {
        ("en VRAM", "en VRAM")
    };
    let mut parts = vec![match e.mode {
        RunMode::Gpu => format!(
            "Tient entièrement {in_vram} ({} / {} Go) en {}",
            gb_value(e.mem_gpu),
            gb_value(budget.vram),
            v.label
        ),
        RunMode::Hybrid { gpu_layers, layers } => format!(
            "Hybride : {gpu_layers} couches sur {layers} {on_gpu}, {} en RAM, en {}",
            gb(e.mem_cpu),
            v.label
        ),
        RunMode::Cpu => format!(
            "Sur le CPU : {} / {} Go de RAM en {}",
            gb_value(e.mem_cpu),
            gb_value(budget.ram),
            v.label
        ),
        RunMode::TooBig => {
            let available = if budget.gpu_offload() {
                budget.combined()
            } else {
                budget.ram
            };
            format!(
                "Trop gros : même en {} au contexte {}, il faut {} pour {} utilisables",
                v.label,
                ctx_k(e.ctx_used),
                gb(e.breakdown.total()),
                gb(available)
            )
        }
    }];
    if e.mode != RunMode::TooBig {
        parts.push(format!("{} tok/s estimés", tok_s(e.tok_s)));
        parts.push(if e.ctx_used >= ctx_wanted {
            format!("contexte {} possible", ctx_k(e.ctx_max))
        } else if e.ctx_used == ctx_train {
            format!("contexte limité à {} par le modèle", ctx_k(e.ctx_used))
        } else {
            format!("contexte réduit à {} pour tenir", ctx_k(e.ctx_used))
        });
    }
    if e.approximate {
        parts.push("estimation approximative".into());
    }
    parts.join(" · ")
}

/// Quantifications de `m` installées dans Ollama, avec l'orthographe de GRAM quand elles
/// correspondent à une variante évaluée.
fn installed_labels(m: &ModelEntry, rows: &[Row], installed: &[OllamaModel]) -> Vec<String> {
    let mut labels: Vec<String> = installed_for(installed, m)
        .filter_map(|om| {
            let q = om.quant.as_deref()?;
            Some(
                rows.iter()
                    .find(|r| om.is_quant(&r.ve.variant.label))
                    .map_or_else(|| q.to_string(), |r| r.ve.variant.label.clone()),
            )
        })
        .collect();
    labels.sort();
    labels.dedup();
    labels
}

fn item(m: &ModelEntry, ev: &Evaluation, i: usize, s: &Setup) -> RankItem {
    let row = &ev.rows[i];
    RankItem {
        repo_id: m.repo_id.clone(),
        display_name: m.display_name.clone(),
        base_model: m.base_model.clone(),
        params_total: m.params_total,
        params_active: m.params_active,
        pipeline: m.pipeline,
        use_cases: m.use_cases.clone(),
        downloads: m.downloads,
        likes: m.likes,
        gated: m.gated,
        created_at: m.created_at,
        detailed: !m.variants.is_empty(),
        variant_count: ev.rows.len() as u32,
        best: row.ve.clone(),
        score: row.score,
        slow: row.slow,
        explanation: explain(&row.ve, &s.budget, s.work.ctx, m.ctx_train),
        installed: installed_labels(m, &ev.rows, s.installed()),
    }
}

/// Meilleur score d'abord ; à égalité, le plus téléchargé, puis l'ordre alphabétique du dépôt.
fn by_score(a: &RankItem, b: &RankItem) -> Ordering {
    b.score
        .total
        .total_cmp(&a.score.total)
        .then(b.downloads.cmp(&a.downloads))
        .then_with(|| a.repo_id.cmp(&b.repo_id))
}

/// Le plus proche de tenir d'abord.
fn by_usage(a: &RankItem, b: &RankItem) -> Ordering {
    a.best
        .estimate
        .mem_usage
        .total_cmp(&b.best.estimate.mem_usage)
        .then(b.downloads.cmp(&a.downloads))
        .then_with(|| a.repo_id.cmp(&b.repo_id))
}

fn finish(
    mut list: Vec<RankItem>,
    order: fn(&RankItem, &RankItem) -> Ordering,
    limit: Option<u32>,
) -> (Vec<RankItem>, u32) {
    list.sort_by(order);
    let total = list.len() as u32;
    if let Some(limit) = limit {
        list.truncate(limit as usize);
    }
    (list, total)
}

/// Classe les modèles du catalogue pour la machine (§9).
pub fn rank(
    models: &[ModelEntry],
    req: &RankRequest,
    cx: &RankContext,
    data_source: DataSource,
) -> RankResponse {
    let s = Setup::new(cx, req.use_case, req.ctx, req.max_size);
    let runtime = req.runtime.unwrap_or(cx.settings.runtime);
    let (mut items, mut slow, mut too_big) = (Vec::new(), Vec::new(), Vec::new());
    let mut too_big_count = 0u32;

    for m in models {
        if req.use_case == UseCase::Vision && !m.use_cases.contains(&UseCase::Vision) {
            continue;
        }
        let ev = evaluate(m, &s);
        if let Some(i) = ev.best {
            let it = item(m, &ev, i, &s);
            if it.slow {
                slow.push(it);
            } else {
                items.push(it);
            }
        } else if let Some(i) = ev.smallest_within_size() {
            too_big_count += 1;
            if req.show_too_big {
                too_big.push(item(m, &ev, i, &s));
            }
        }
    }

    let (items, total_items) = finish(items, by_score, req.limit);
    let (slow, total_slow) = finish(slow, by_score, req.limit);
    let (too_big, _) = finish(too_big, by_usage, req.limit);
    RankResponse {
        use_case: req.use_case,
        ctx: s.work.ctx,
        runtime,
        notes: runtime_notes(runtime, cx.hardware, &s.budget, cx.runtimes),
        budget: s.budget,
        slow_tok_s: s.profile.slow_tok_s,
        items,
        total_items,
        slow,
        total_slow,
        too_big,
        too_big_count,
        data_source,
    }
}

/// Fiche détail d'un modèle : toutes ses quantifications estimées et notées, avec les
/// commandes de lancement pour le runtime choisi.
pub fn model_detail(m: &ModelEntry, req: &DetailRequest, cx: &RankContext) -> ModelDetail {
    let s = Setup::new(cx, req.use_case, req.ctx, req.max_size);
    let runtime = req.runtime.unwrap_or(cx.settings.runtime);
    let ev = evaluate(m, &s);
    let variants = ev
        .rows
        .into_iter()
        .map(|r| VariantDetail {
            ollama_model: installed_variant(
                s.installed(),
                m,
                &r.ve.variant.repo_id,
                &r.ve.variant.label,
            )
            .map(|om| om.name.clone()),
            ollama_pull: ollama_pull_command(&r.ve.variant),
            explanation: explain(&r.ve, &s.budget, s.work.ctx, m.ctx_train),
            launch: launch_plan(
                runtime,
                &r.ve.variant,
                &r.ve.estimate,
                m.gated,
                s.params.kv_cache,
                cx.hardware.os,
            ),
            variant: r.ve.variant,
            estimate: r.ve.estimate,
            score: r.score,
            slow: r.slow,
            within_size: r.within_size,
        })
        .collect();
    ModelDetail {
        model: m.clone(),
        use_case: req.use_case,
        ctx: s.work.ctx,
        runtime,
        notes: runtime_notes(runtime, cx.hardware, &s.budget, cx.runtimes),
        budget: s.budget,
        slow_tok_s: s.profile.slow_tok_s,
        variants,
        best: ev.best.map(|i| i as u32),
        hf_url: hf_url(&m.repo_id),
    }
}
