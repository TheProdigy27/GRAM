//! Calibration sur des mesures réelles (phase 7 du plan).
//!
//! Ollama mesure la vitesse de génération (`eval_count / eval_duration`) et la mémoire occupée
//! (`/api/ps`) ; [`measurement`] les compare à l'estimation de GRAM au même contexte.
//! [`calibrate`] en déduit les réglages de la formule de vitesse (§8.4) :
//!
//! `temps par token = transfert / efficacité + temps fixe`, où `transfert` est le temps de
//! lecture des poids actifs à pleine bande passante, calculé avec la répartition VRAM / RAM
//! **mesurée** (une estimation fausse du placement ne fausse pas la calibration).
//!
//! - Une mesure, ou des mesures de tailles proches : l'efficacité seule, médiane des valeurs
//!   `transfert / (temps mesuré − temps fixe actuel)`.
//! - Au moins deux mesures dont les octets lus par token diffèrent d'un facteur 2 (un modèle
//!   dense et un MoE, par exemple) : droite des moindres carrés `temps = a × transfert + b`,
//!   d'où l'efficacité (`1 / a`) et le temps fixe (`b`), s'ils restent plausibles.
//!
//! Pur : aucune I/O. L'appel à Ollama est dans `gram-ollama`.

use crate::estimate::{
    Estimate, EstimateParams, MemoryBudget, RunMode, active_bytes, gpu_share, transfer_seconds,
};
use crate::fmt::{gb, num, thousands};
use crate::model::{ModelEntry, QuantVariant, UseCase};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Tokens générés au moins pour qu'une mesure serve à la calibration.
pub const MIN_RELIABLE_TOKENS: u32 = 32;

/// Mesures gardées, les plus récentes.
pub const MAX_MEASUREMENTS: usize = 50;

/// Bornes de l'efficacité (les mêmes que l'assainissement des paramètres).
const EFFICIENCY_MIN: f64 = 0.05;
const EFFICIENCY_MAX: f64 = 2.0;

/// Temps fixe par token ajusté au plus, en millisecondes : au-delà, l'ajustement n'est pas
/// crédible (bruit de mesure).
const MAX_FITTED_OVERHEAD_MS: f64 = 20.0;

/// Écart d'octets lus par token nécessaire pour ajuster aussi le temps fixe.
const MIN_SPREAD: f64 = 2.0;

/// En dessous de cette part de la mémoire en VRAM, une partie du modèle est en RAM.
const FULL_VRAM: f64 = 0.98;

/// Commande `ollama_measure` : la quantification d'un modèle du catalogue, avec les filtres de
/// la fiche détail (le contexte mesuré est celui retenu par l'estimation).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, optional_fields)]
pub struct MeasureRequest {
    pub repo_id: String,
    pub label: String,
    pub use_case: UseCase,
    /// Contexte souhaité ; absent : celui du cas d'usage.
    pub ctx: Option<u32>,
}

/// Ce qu'Ollama a mesuré : réponse de `POST /api/generate` et entrée de `GET /api/ps`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OllamaRun {
    pub eval_count: u32,
    pub eval_duration_ns: u64,
    pub prompt_eval_count: u32,
    pub prompt_eval_duration_ns: u64,
    /// Chargement du modèle (0 s'il était déjà en mémoire avec ce contexte).
    pub load_duration_ns: u64,
    /// Le modèle tel que chargé ; absent s'il n'apparaît pas dans `/api/ps`.
    pub loaded: Option<LoadedModel>,
    /// Autres modèles chargés dans Ollama au même moment.
    pub others_loaded: u32,
}

/// Un modèle chargé en mémoire par Ollama (`/api/ps`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LoadedModel {
    /// Mémoire occupée, en octets.
    pub size: u64,
    /// Dont VRAM, en octets.
    pub size_vram: u64,
    pub context_length: Option<u32>,
}

/// Une mesure réelle face à l'estimation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Measurement {
    /// Modèle du catalogue (dépôt canonique) et quantification.
    pub repo_id: String,
    pub display_name: String,
    pub label: String,
    /// Nom du modèle dans Ollama.
    pub ollama_model: String,
    /// Contexte demandé à Ollama (`num_ctx`) : celui de l'estimation.
    pub ctx: u32,
    pub measured_at: DateTime<Utc>,

    /// Vitesse de génération mesurée, en tok/s.
    pub tok_s: f32,
    /// Tokens générés pendant la mesure.
    pub eval_count: u32,
    /// Traitement du prompt, en tok/s (pour information : non estimé).
    pub prompt_tok_s: Option<f32>,
    /// Chargement du modèle, en ms.
    pub load_ms: u64,
    /// Mémoire occupée d'après Ollama, en octets.
    pub mem_total: Option<u64>,
    /// Dont VRAM, en octets.
    pub mem_vram: Option<u64>,
    /// Contexte avec lequel Ollama a chargé le modèle.
    pub ctx_loaded: Option<u32>,

    /// Estimation de GRAM au même contexte, au moment de la mesure.
    pub estimated_tok_s: f32,
    pub estimated_mode: RunMode,
    pub estimated_mem_total: u64,
    pub estimated_mem_vram: u64,
    /// `(estimé − mesuré) / mesuré` : négatif quand GRAM sous-estime.
    pub tok_s_error: Option<f32>,
    pub mem_error: Option<f32>,

    /// Octets lus par token (poids actifs), pour la calibration.
    pub active_bytes: u64,
    /// Part des poids en VRAM : mesurée (`size_vram / size`), sinon celle de l'estimation.
    pub gpu_share: f32,
    /// Assez de tokens générés pour servir à la calibration.
    pub reliable: bool,
    /// Remarques en français.
    pub notes: Vec<String>,
}

/// Mesures enregistrées et réglages qu'elles suggèrent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Calibration {
    /// La plus récente d'abord.
    pub measurements: Vec<Measurement>,
    /// Mesures retenues pour le calcul.
    pub used: u32,
    pub current_efficiency: f32,
    pub current_token_overhead_ms: f32,
    /// Efficacité proposée (arrondie au centième) ; absente sans mesure exploitable.
    pub efficiency: Option<f32>,
    /// Temps fixe par token proposé (arrondi au dixième de ms) ; absent s'il ne peut pas être
    /// ajusté (une seule taille de modèle mesurée) : la valeur actuelle est gardée.
    pub token_overhead_ms: Option<f32>,
    /// La proposition diffère des réglages actuels.
    pub changed: bool,
    /// Explication en français.
    pub summary: String,
}

impl Calibration {
    /// Applique la proposition aux paramètres ; `false` s'il n'y a rien à appliquer.
    pub fn apply_to(&self, p: &mut EstimateParams) -> bool {
        let Some(efficiency) = self.efficiency else {
            return false;
        };
        p.efficiency = efficiency;
        if let Some(ms) = self.token_overhead_ms {
            p.token_overhead_ms = ms;
        }
        true
    }
}

/// Compare une mesure d'Ollama à l'estimation de la même quantification (au contexte
/// `estimate.ctx_used`).
pub fn measurement(
    model: &ModelEntry,
    variant: &QuantVariant,
    estimate: &Estimate,
    ollama_model: &str,
    run: &OllamaRun,
    at: DateTime<Utc>,
) -> Measurement {
    let rate = |count: u32, ns: u64| (ns > 0).then(|| (f64::from(count) * 1e9 / ns as f64) as f32);
    let tok_s = rate(run.eval_count, run.eval_duration_ns).unwrap_or(0.0);
    let prompt_tok_s = rate(run.prompt_eval_count, run.prompt_eval_duration_ns)
        .filter(|_| run.prompt_eval_count > 0);
    let ctx = estimate.ctx_used;
    let est_total = estimate.breakdown.total();
    let loaded = run.loaded.filter(|l| l.size > 0);
    let mem_vram = loaded.map(|l| l.size_vram.min(l.size));
    let vram_share = loaded.map(|l| l.size_vram.min(l.size) as f64 / l.size as f64);
    let relative = |est: f64, measured: f64| ((est - measured) / measured) as f32;
    let reliable = run.eval_count >= MIN_RELIABLE_TOKENS && tok_s > 0.0;

    let mut notes = Vec::new();
    if !reliable {
        notes.push(format!(
            "Seulement {} tokens générés : mesure peu fiable, non utilisée pour la calibration.",
            run.eval_count
        ));
    }
    match loaded {
        None => notes.push(
            "Mémoire non lue : le modèle n'apparaît pas parmi les modèles chargés par Ollama."
                .into(),
        ),
        Some(l) => {
            if let Some(c) = l.context_length.filter(|&c| c > 0 && c != ctx) {
                notes.push(format!(
                    "Ollama a chargé le modèle avec un contexte de {} tokens (demandé : {}).",
                    thousands(u64::from(c)),
                    thousands(u64::from(ctx))
                ));
            }
            let share = vram_share.unwrap_or(0.0);
            match estimate.mode {
                RunMode::Gpu if share < FULL_VRAM => notes.push(format!(
                    "Ollama a placé {} en RAM alors que l'estimation tenait tout en VRAM : la \
                     génération en est ralentie.",
                    gb(l.size - l.size_vram.min(l.size))
                )),
                RunMode::Hybrid { .. } | RunMode::Cpu if share >= FULL_VRAM => notes.push(
                    "Ollama a tout placé en VRAM, contrairement à l'estimation : la marge de \
                     sécurité VRAM des paramètres est peut-être trop prudente."
                        .into(),
                ),
                _ => {}
            }
            if l.size as f64 > est_total as f64 * 1.3 {
                notes.push(
                    "Mémoire bien plus élevée que l'estimation : avec OLLAMA_NUM_PARALLEL \
                     supérieur à 1, Ollama réserve un cache KV par requête parallèle."
                        .into(),
                );
            }
        }
    }
    if run.others_loaded > 0 {
        notes.push(format!(
            "{} autre{} modèle{} chargé{} dans Ollama pendant la mesure : moins de mémoire \
             disponible.",
            run.others_loaded,
            plural(run.others_loaded),
            plural(run.others_loaded),
            plural(run.others_loaded)
        ));
    }
    if estimate.approximate {
        notes.push(
            "Estimation approximative (taille ou architecture inconnue) : l'écart peut venir de \
             là."
            .into(),
        );
    }

    Measurement {
        repo_id: model.repo_id.clone(),
        display_name: model.display_name.clone(),
        label: variant.label.clone(),
        ollama_model: ollama_model.to_string(),
        ctx,
        measured_at: at,
        tok_s,
        eval_count: run.eval_count,
        prompt_tok_s,
        load_ms: run.load_duration_ns / 1_000_000,
        mem_total: loaded.map(|l| l.size),
        mem_vram,
        ctx_loaded: loaded.and_then(|l| l.context_length),
        estimated_tok_s: estimate.tok_s,
        estimated_mode: estimate.mode,
        estimated_mem_total: est_total,
        estimated_mem_vram: estimate.mem_gpu,
        tok_s_error: (tok_s > 0.0).then(|| relative(f64::from(estimate.tok_s), f64::from(tok_s))),
        mem_error: loaded.map(|l| relative(est_total as f64, l.size as f64)),
        active_bytes: active_bytes(model, variant.size_bytes),
        gpu_share: vram_share.unwrap_or_else(|| gpu_share(estimate.mode)) as f32,
        reliable,
        notes,
    }
}

fn plural(n: u32) -> &'static str {
    if n > 1 { "s" } else { "" }
}

/// Ajoute une mesure en tête ; une mesure précédente du même modèle Ollama au même contexte
/// est remplacée. Au plus [`MAX_MEASUREMENTS`] mesures sont gardées.
pub fn record(list: &mut Vec<Measurement>, m: Measurement) {
    list.retain(|old| {
        !(old.ollama_model.eq_ignore_ascii_case(&m.ollama_model) && old.ctx == m.ctx)
    });
    list.insert(0, m);
    list.truncate(MAX_MEASUREMENTS);
}

/// Réglages de vitesse suggérés par les mesures, pour la machine décrite par `budget`
/// (bandes passantes actuelles, corrections comprises).
pub fn calibrate(
    measurements: Vec<Measurement>,
    budget: &MemoryBudget,
    p: &EstimateParams,
) -> Calibration {
    let current_eff = f64::from(p.efficiency);
    let current_ms = f64::from(p.token_overhead_ms.max(0.0));
    // (transfert à pleine bande passante, temps mesuré par token), en secondes.
    let points: Vec<(f64, f64)> = measurements
        .iter()
        .filter(|m| m.reliable && m.tok_s > 0.0)
        .map(|m| {
            (
                transfer_seconds(m.active_bytes, f64::from(m.gpu_share), budget),
                1.0 / f64::from(m.tok_s),
            )
        })
        .filter(|&(x, t)| x > 0.0 && t.is_finite())
        .collect();

    let fitted = fit_both(&points);
    let (efficiency, overhead_ms) = match fitted {
        Some((eff, ms)) => (Some(eff), Some(ms)),
        None => (fit_efficiency(&points, current_ms / 1000.0), None),
    };
    let efficiency = efficiency.map(|e| round(e.clamp(EFFICIENCY_MIN, EFFICIENCY_MAX), 2));
    let overhead_ms = overhead_ms.map(|ms| round(ms, 1));
    let changed = efficiency.is_some_and(|e| (e - current_eff).abs() >= 0.005)
        || overhead_ms.is_some_and(|ms| (ms - current_ms).abs() >= 0.05);

    let n = points.len();
    let summary = match efficiency {
        None if n == 0 => "Aucune mesure exploitable : mesurer une quantification déjà installée \
                           dans Ollama, depuis la fiche du modèle."
            .to_string(),
        None => format!(
            "Les mesures sont plus rapides que ne le permet le temps fixe par token ({} ms) : le \
             réduire dans les paramètres.",
            num(current_ms, 1)
        ),
        Some(eff) => {
            let mut text = format!(
                "D'après {n} mesure{} : efficacité {} (actuelle {})",
                if n > 1 { "s" } else { "" },
                num(eff, 2),
                num(current_eff, 2)
            );
            match overhead_ms {
                Some(ms) => text.push_str(&format!(
                    ", temps fixe par token {} ms (actuel {} ms).",
                    num(ms, 1),
                    num(current_ms, 1)
                )),
                None => {
                    text.push('.');
                    if spread(&points) < MIN_SPREAD {
                        text.push_str(
                            " Mesurer aussi un modèle de taille très différente (un MoE, un \
                             petit modèle) permettrait d'ajuster le temps fixe par token.",
                        );
                    }
                }
            }
            if !changed {
                text.push_str(" Les réglages actuels correspondent déjà aux mesures.");
            }
            text
        }
    };

    Calibration {
        used: n as u32,
        measurements,
        current_efficiency: p.efficiency,
        current_token_overhead_ms: p.token_overhead_ms,
        efficiency: efficiency.map(|e| e as f32),
        token_overhead_ms: overhead_ms.map(|ms| ms as f32),
        changed,
        summary,
    }
}

/// Rapport entre le plus grand et le plus petit transfert.
fn spread(points: &[(f64, f64)]) -> f64 {
    let (min, max) = points
        .iter()
        .fold((f64::INFINITY, 0.0f64), |(lo, hi), &(x, _)| {
            (lo.min(x), hi.max(x))
        });
    if min > 0.0 && min.is_finite() {
        max / min
    } else {
        1.0
    }
}

/// Droite des moindres carrés `temps = a × transfert + b` : efficacité `1 / a` et temps fixe
/// `b` (en ms), seulement si les tailles mesurées sont assez différentes et le résultat
/// plausible.
fn fit_both(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    if points.len() < 2 || spread(points) < MIN_SPREAD {
        return None;
    }
    let n = points.len() as f64;
    let mean_x = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_t = points.iter().map(|p| p.1).sum::<f64>() / n;
    let sxx: f64 = points.iter().map(|p| (p.0 - mean_x).powi(2)).sum();
    let sxt: f64 = points.iter().map(|p| (p.0 - mean_x) * (p.1 - mean_t)).sum();
    let a = sxt / sxx;
    let b_ms = (mean_t - a * mean_x) * 1000.0;
    let eff = 1.0 / a;
    (a > 0.0
        && (EFFICIENCY_MIN..=EFFICIENCY_MAX).contains(&eff)
        && (0.0..=MAX_FITTED_OVERHEAD_MS).contains(&b_ms))
    .then_some((eff, b_ms))
}

/// Efficacité seule, le temps fixe étant connu : médiane de `transfert / (temps − fixe)`.
fn fit_efficiency(points: &[(f64, f64)], fixed_s: f64) -> Option<f64> {
    let mut values: Vec<f64> = points
        .iter()
        .filter(|&&(_, t)| t > fixed_s)
        .map(|&(x, t)| x / (t - fixed_s))
        .collect();
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    Some(if values.len() % 2 == 1 {
        values[mid]
    } else {
        (values[mid - 1] + values[mid]) / 2.0
    })
}

fn round(x: f64, decimals: i32) -> f64 {
    let f = 10f64.powi(decimals);
    (x * f).round() / f
}

#[cfg(test)]
mod tests;
