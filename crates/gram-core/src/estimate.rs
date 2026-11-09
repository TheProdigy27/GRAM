//! Moteur d'estimation (§8 du plan) : pour un modèle, une quantification et une machine, la
//! mémoire nécessaire, le mode d'exécution (GPU / hybride / CPU), le contexte maximal
//! atteignable et la vitesse de génération.
//!
//! 1. [`MemoryBudget::from_profile`] : mémoire utilisable et bandes passantes (§6.5), calculées
//!    une fois par classement ;
//! 2. [`candidate_variants`] : quantifications à évaluer (tailles réelles, sinon échelle de
//!    repli de `data/quant_quality.json`) ;
//! 3. [`estimate`] : placement, contexte, vitesse et niveau d'adéquation d'une variante.
//!
//! Toutes les valeurs sont des estimations (précision visée : ±30 %). Le choix de la meilleure
//! variante revient au scoring (§9).

mod memory;
mod quants;
#[cfg(test)]
mod tests;

pub use memory::{
    CLOSE_APPS_RAM_RESERVE, EstimateParams, KvCacheType, MemoryBudget, VRAM_FREE_TOLERANCE,
    fallback_kv_bytes_per_token, fallback_layers, kv_bytes_per_token,
};
pub use quants::{QuantTable, TypicalQuant, candidate_variants};

use crate::hardware::DEFAULT_GPU_BANDWIDTH_GBS;
use crate::model::{ModelEntry, QuantVariant};
use memory::ModelShape;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Plafond du contexte maximal quand le modèle n'indique pas son contexte d'entraînement.
pub const UNKNOWN_CTX_TRAIN_CAP: u32 = 131_072;

/// Seuils de la part de mémoire occupée (§9) : « Parfait » jusqu'à 80 %, « Bon » jusqu'à 95 %.
pub const PERFECT_USAGE: f32 = 0.80;
pub const GOOD_USAGE: f32 = 0.95;

/// En hybride, « Bon » à partir de 75 % des couches sur le GPU.
const GOOD_HYBRID_NUM: u32 = 3;
const GOOD_HYBRID_DEN: u32 = 4;

/// Mode d'exécution estimé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum RunMode {
    /// Tout tient en VRAM.
    Gpu,
    /// `gpu_layers` couches sur `layers` en VRAM (option `-ngl` de llama.cpp), le reste en RAM.
    Hybrid { gpu_layers: u32, layers: u32 },
    /// Tout en RAM.
    Cpu,
    /// Ne tient pas, même au contexte minimal.
    TooBig,
}

/// Niveau d'adéquation affiché : parfait, bon, limite, trop juste.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FitLevel {
    /// GPU complet avec au plus 80 % de la mémoire utilisée.
    Perfect,
    /// GPU complet jusqu'à 95 %, ou hybride avec au moins 75 % des couches sur le GPU.
    Good,
    /// Autres cas hybrides, CPU, ou GPU complet au-delà de 95 %.
    Marginal,
    /// Ne tient pas.
    TooTight,
}

/// Besoin mémoire au contexte retenu, poste par poste (barre mémoire de la fiche détail).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MemoryBreakdown {
    /// Fichiers de la quantification, en octets.
    pub weights: u64,
    /// Cache KV, en octets.
    pub kv: u64,
    /// Surcoût du runtime, en octets.
    pub overhead: u64,
    /// Projecteur vision (`mmproj`), si l'usage vision est demandé, en octets.
    pub vision: u64,
}

impl MemoryBreakdown {
    pub fn total(&self) -> u64 {
        self.weights
            .saturating_add(self.kv)
            .saturating_add(self.overhead)
            .saturating_add(self.vision)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Estimate {
    pub mode: RunMode,
    /// Octets en VRAM.
    pub mem_gpu: u64,
    /// Octets en RAM.
    pub mem_cpu: u64,
    pub breakdown: MemoryBreakdown,
    /// Cache KV par token de contexte, en octets.
    pub kv_per_token: u64,
    /// Part de la mémoire du mode occupée (VRAM, VRAM + RAM en hybride, ou RAM) : le `u` du §9.
    /// Au-delà de 1 pour un modèle trop gros.
    pub mem_usage: f32,
    /// Contexte retenu pour l'estimation, en tokens (réduit si besoin pour tenir en mémoire).
    pub ctx_used: u32,
    /// Contexte maximal atteignable dans ce mode, en tokens (0 si le modèle ne tient pas).
    pub ctx_max: u32,
    /// Vitesse de génération estimée (le traitement du prompt n'est pas modélisé).
    pub tok_s: f32,
    pub fit: FitLevel,
    /// Architecture inconnue ou taille estimée : badge « estimation approximative ».
    pub approximate: bool,
    /// Remarques en français, affichées telles quelles.
    pub warnings: Vec<String>,
}

/// Une quantification et son estimation (tableau de la fiche détail).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct VariantEstimate {
    pub variant: QuantVariant,
    pub estimate: Estimate,
}

/// Ce que l'utilisateur veut faire du modèle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Workload {
    /// Contexte souhaité, en tokens.
    pub ctx: u32,
    /// Usage vision : la taille du projecteur (`mmproj`) s'ajoute au besoin mémoire.
    pub vision: bool,
}

/// Estime toutes les variantes candidates d'un modèle ([`candidate_variants`]).
pub fn estimate_variants(
    model: &ModelEntry,
    budget: &MemoryBudget,
    work: Workload,
    params: &EstimateParams,
    table: &QuantTable,
) -> Vec<VariantEstimate> {
    candidate_variants(model, params, table)
        .into_iter()
        .map(|variant| VariantEstimate {
            estimate: estimate(model, &variant, budget, work, params),
            variant,
        })
        .collect()
}

/// Estime une variante (§8.1 à §8.4).
///
/// Le contexte retenu est le contexte souhaité, limité au contexte d'entraînement. S'il ne
/// tient nulle part, il est divisé par deux jusqu'au contexte minimal ; au-delà, le modèle est
/// trop gros.
pub fn estimate(
    model: &ModelEntry,
    variant: &QuantVariant,
    budget: &MemoryBudget,
    work: Workload,
    params: &EstimateParams,
) -> Estimate {
    let weights = variant.size_bytes;
    let shape = ModelShape::of(model, weights, params.kv_cache);
    let vision = if work.vision {
        model.mmproj_bytes.unwrap_or(0)
    } else {
        0
    };
    let fixed = params.overhead_bytes.saturating_add(vision);

    let mut warnings = Vec::new();
    if !shape.arch_known {
        warnings.push(
            "Architecture inconnue : cache KV et nombre de couches estimés de façon approximative."
                .to_string(),
        );
    }
    if !variant.exact {
        warnings.push(
            "Taille estimée : les fichiers de ce dépôt n'ont pas encore été détaillés.".to_string(),
        );
    }
    if shape.sliding_window {
        warnings.push(
            "Attention à fenêtre glissante : le cache KV est probablement surestimé.".to_string(),
        );
    }

    let mut ctx = work.ctx.max(1);
    let ctx_cap = if model.ctx_train > 0 {
        if model.ctx_train < ctx {
            ctx = model.ctx_train;
            warnings.push(format!(
                "Contexte limité à {} tokens par le modèle.",
                tokens(ctx)
            ));
        }
        model.ctx_train
    } else {
        warnings.push("Contexte d'entraînement inconnu.".to_string());
        UNKNOWN_CTX_TRAIN_CAP
    };

    let start = ctx;
    let floor = params.min_ctx.clamp(1, ctx);
    let placement = loop {
        let kv = kv_for(shape.kv_per_token, ctx);
        if let Some(p) = place(weights, kv, fixed, shape.layers, budget) {
            break Some(p);
        }
        if ctx <= floor {
            break None;
        }
        ctx = (ctx / 2).max(floor);
    };

    let breakdown = MemoryBreakdown {
        weights,
        kv: kv_for(shape.kv_per_token, ctx),
        overhead: params.overhead_bytes,
        vision,
    };
    let total = breakdown.total();
    let approximate = !shape.arch_known || !variant.exact;

    let Some(p) = placement else {
        return Estimate {
            mode: RunMode::TooBig,
            mem_gpu: 0,
            mem_cpu: 0,
            breakdown,
            kv_per_token: shape.kv_per_token,
            mem_usage: usage(total, budget.vram.saturating_add(budget.ram)),
            ctx_used: ctx,
            ctx_max: 0,
            tok_s: 0.0,
            fit: FitLevel::TooTight,
            approximate,
            warnings,
        };
    };
    if ctx < start {
        warnings.push(format!(
            "Contexte réduit à {} tokens pour tenir en mémoire.",
            tokens(ctx)
        ));
    }

    let capacity = match p.mode {
        RunMode::Gpu => budget.vram,
        RunMode::Cpu => budget.ram,
        _ => budget.vram.saturating_add(budget.ram),
    };
    let mem_usage = usage(total, capacity);
    Estimate {
        mode: p.mode,
        mem_gpu: p.mem_gpu,
        mem_cpu: p.mem_cpu,
        breakdown,
        kv_per_token: shape.kv_per_token,
        mem_usage,
        ctx_used: ctx,
        ctx_max: ctx_max(p.mode, weights, fixed, shape.kv_per_token, budget, ctx_cap).max(ctx),
        tok_s: tok_s(p.mode, weights, shape.active_ratio, budget, params),
        fit: fit_level(p.mode, mem_usage),
        approximate,
        warnings,
    }
}

/// Répartition de la mémoire pour un mode donné.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Placement {
    mode: RunMode,
    mem_gpu: u64,
    mem_cpu: u64,
}

fn kv_for(kv_per_token: u64, ctx: u32) -> u64 {
    kv_per_token.saturating_mul(u64::from(ctx))
}

/// §8.2. En hybride, le cache KV, le surcoût et le projecteur vision restent en VRAM
/// (simplification du MVP) ; les poids y prennent la place restante, par couches entières.
fn place(weights: u64, kv: u64, fixed: u64, layers: u32, b: &MemoryBudget) -> Option<Placement> {
    let resident = kv.saturating_add(fixed);
    let total = weights.saturating_add(resident);
    if b.gpu_offload() {
        if total <= b.vram {
            return Some(Placement {
                mode: RunMode::Gpu,
                mem_gpu: total,
                mem_cpu: 0,
            });
        }
        // Ici `room < weights`, donc moins de `layers` couches.
        let room = b.vram.saturating_sub(resident);
        let gpu_layers =
            (u128::from(room) * u128::from(layers) / u128::from(weights.max(1))) as u32;
        if gpu_layers > 0 {
            let on_gpu = (u128::from(weights) * u128::from(gpu_layers) / u128::from(layers)) as u64;
            let mem_cpu = weights - on_gpu;
            if mem_cpu <= b.ram {
                return Some(Placement {
                    mode: RunMode::Hybrid { gpu_layers, layers },
                    mem_gpu: resident + on_gpu,
                    mem_cpu,
                });
            }
        }
    }
    (total <= b.ram).then_some(Placement {
        mode: RunMode::Cpu,
        mem_gpu: 0,
        mem_cpu: total,
    })
}

/// §8.3 : `min(contexte d'entraînement, (mémoire du mode − poids − surcoût) / KV par token)`.
/// En hybride, le cache KV doit aussi laisser au moins une couche en VRAM.
fn ctx_max(
    mode: RunMode,
    weights: u64,
    fixed: u64,
    kv_per_token: u64,
    b: &MemoryBudget,
    cap: u32,
) -> u32 {
    let static_mem = weights.saturating_add(fixed);
    let room = match mode {
        RunMode::Gpu => b.vram.saturating_sub(static_mem),
        RunMode::Hybrid { layers, .. } => {
            let one_layer = weights.div_ceil(u64::from(layers.max(1)));
            b.vram
                .saturating_add(b.ram)
                .saturating_sub(static_mem)
                .min(b.vram.saturating_sub(fixed.saturating_add(one_layer)))
        }
        RunMode::Cpu => b.ram.saturating_sub(static_mem),
        RunMode::TooBig => 0,
    };
    (room / kv_per_token.max(1)).min(u64::from(cap)) as u32
}

/// §8.4 : la génération est limitée par la bande passante mémoire. Chaque token lit les poids
/// actifs une fois (seulement les paramètres actifs d'un MoE), plus un temps fixe :
/// `temps = (octets GPU / BW GPU + octets RAM / BW RAM) / efficacité + temps fixe`, et
/// `tok/s = 1 / temps`. En hybride, les poids se répartissent selon les couches.
///
/// Sans temps fixe, la vitesse d'un modèle qui lit peu d'octets par token (MoE à 3B actifs,
/// modèle de 0,6B) serait surestimée : son coût n'est plus dominé par la bande passante.
fn tok_s(
    mode: RunMode,
    weights: u64,
    active_ratio: f64,
    b: &MemoryBudget,
    p: &EstimateParams,
) -> f32 {
    let gpu_share = match mode {
        RunMode::Gpu => 1.0,
        RunMode::Hybrid { gpu_layers, layers } => f64::from(gpu_layers) / f64::from(layers),
        RunMode::Cpu => 0.0,
        RunMode::TooBig => return 0.0,
    };
    let bytes = weights as f64 * active_ratio;
    let gpu_bw = f64::from(b.gpu_bandwidth_gbs.unwrap_or(DEFAULT_GPU_BANDWIDTH_GBS)) * 1e9;
    let ram_bw = f64::from(b.ram_bandwidth_gbs) * 1e9;
    let transfer = bytes * gpu_share / gpu_bw + bytes * (1.0 - gpu_share) / ram_bw;
    let seconds =
        transfer / f64::from(p.efficiency) + f64::from(p.token_overhead_ms.max(0.0)) / 1000.0;
    if seconds.is_finite() && seconds > 0.0 {
        (1.0 / seconds) as f32
    } else {
        0.0
    }
}

/// Niveaux d'adéquation du §9.
fn fit_level(mode: RunMode, mem_usage: f32) -> FitLevel {
    match mode {
        RunMode::Gpu if mem_usage <= PERFECT_USAGE => FitLevel::Perfect,
        RunMode::Gpu if mem_usage <= GOOD_USAGE => FitLevel::Good,
        RunMode::Hybrid { gpu_layers, layers }
            if gpu_layers * GOOD_HYBRID_DEN >= layers * GOOD_HYBRID_NUM =>
        {
            FitLevel::Good
        }
        RunMode::Gpu | RunMode::Hybrid { .. } | RunMode::Cpu => FitLevel::Marginal,
        RunMode::TooBig => FitLevel::TooTight,
    }
}

/// Toujours fini (un `f32` infini deviendrait `null` en JSON).
fn usage(used: u64, capacity: u64) -> f32 {
    (used as f64 / capacity.max(1) as f64) as f32
}

/// Nombre de tokens à la française : « 8 192 » (espace insécable entre les milliers).
fn tokens(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 * 2);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('\u{a0}');
        }
        out.push(c);
    }
    out
}
