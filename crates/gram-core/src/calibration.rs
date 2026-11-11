//! Calibration sur des mesures réelles (phase 7 du plan).
//!
//! Ollama mesure la vitesse de génération (`eval_count / eval_duration`) et la mémoire occupée
//! (`/api/ps`) ; [`measurement`] les compare à l'estimation de GRAM au même contexte.
//! [`calibrate`] en déduit les réglages de la formule de vitesse (§8.4) :
//!
//! `temps par token = transfert GPU / efficacité GPU + transfert RAM / efficacité RAM + temps
//! fixe`, où les transferts sont les temps de lecture des poids actifs à pleine bande passante,
//! calculés avec la répartition VRAM / RAM **mesurée** (une estimation fausse du placement ne
//! fausse pas la calibration). Les termes propres aux MoE ([`token_cost`]) restent fixes : leurs
//! octets sont ramenés à l'efficacité d'un modèle dense et leur temps fixe propre est retiré du
//! temps mesuré.
//!
//! Chaque mesure compte pour le côté dont la lecture domine son temps : la VRAM pour un modèle
//! qui y tient, la RAM pour un modèle hybride ou sur CPU. Un côté sans mesure garde son
//! efficacité.
//!
//! - Mesures de tailles proches : l'efficacité de chaque côté mesuré, médiane des valeurs
//!   `transfert du côté / (temps mesuré − temps fixe actuel − temps de l'autre côté)`.
//! - Au moins deux mesures d'un même côté dont les octets lus par token diffèrent d'un facteur 2
//!   (un petit et un gros modèle, un dense et un MoE) : moindres carrés sur l'erreur relative,
//!   d'où les efficacités des côtés mesurés et le temps fixe, s'ils restent plausibles.
//!
//! Pur : aucune I/O. L'appel à Ollama est dans `gram-ollama`.

use crate::estimate::{
    Estimate, EstimateParams, MemoryBudget, RunMode, Transfer, gpu_share, token_cost,
    transfer_seconds,
};
use crate::fmt::{gb, num, thousands};
use crate::model::{ModelEntry, QuantVariant, UseCase};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Tokens générés au moins pour qu'une mesure serve à la calibration.
pub const MIN_RELIABLE_TOKENS: u32 = 32;

/// Tokens du prompt réellement calculés (hors cache) au moins pour afficher sa vitesse.
pub const MIN_PROMPT_TOKENS: u32 = 16;

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
    /// Tokens du prompt, y compris ceux repris du cache.
    pub prompt_eval_count: u32,
    /// Dont repris du cache d'Ollama : ils ne comptent pas dans `prompt_eval_duration_ns`.
    pub prompt_eval_cached_count: u32,
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

    /// Octets lus par token (poids actifs, ramenés à l'efficacité d'un modèle dense), pour la
    /// calibration.
    pub active_bytes: u64,
    /// Temps fixe par token propre au modèle (MoE), en ms : retiré du temps mesuré avant la
    /// calibration.
    #[serde(default)]
    pub model_overhead_ms: f32,
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
    pub current_gpu_efficiency: f32,
    pub current_ram_efficiency: f32,
    pub current_token_overhead_ms: f32,
    /// Efficacité GPU proposée (arrondie au centième) ; absente sans mesure exploitable d'un
    /// modèle tenant en VRAM : la valeur actuelle est gardée.
    pub gpu_efficiency: Option<f32>,
    /// Efficacité RAM proposée ; absente sans mesure exploitable d'un modèle hybride ou sur CPU.
    pub ram_efficiency: Option<f32>,
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
        if self.gpu_efficiency.is_none() && self.ram_efficiency.is_none() {
            return false;
        }
        if let Some(e) = self.gpu_efficiency {
            p.gpu_efficiency = e;
        }
        if let Some(e) = self.ram_efficiency {
            p.ram_efficiency = e;
        }
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
    // Ollama garde en cache le début du dernier prompt du modèle chargé : ces tokens comptent
    // dans `prompt_eval_count` sans avoir été calculés. Remesurer dans les 5 minutes reprend
    // presque tout le prompt du cache, et la vitesse n'a alors plus de sens.
    let computed = run
        .prompt_eval_count
        .saturating_sub(run.prompt_eval_cached_count);
    let prompt_tok_s =
        rate(computed, run.prompt_eval_duration_ns).filter(|_| computed >= MIN_PROMPT_TOKENS);
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

    let cost = token_cost(model, variant.size_bytes);
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
        active_bytes: cost.bytes,
        model_overhead_ms: cost.extra_ms,
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
    let current = Speed {
        gpu: f64::from(p.gpu_efficiency),
        ram: f64::from(p.ram_efficiency),
        fixed_s: f64::from(p.token_overhead_ms.max(0.0)) / 1000.0,
    };
    let points: Vec<Point> = measurements
        .iter()
        .filter(|m| m.reliable && m.tok_s > 0.0)
        .map(|m| Point {
            x: transfer_seconds(m.active_bytes, f64::from(m.gpu_share), budget),
            t: 1.0 / f64::from(m.tok_s) - f64::from(m.model_overhead_ms.max(0.0)) / 1000.0,
        })
        .filter(|pt| pt.x.total() > 0.0 && pt.t.is_finite() && pt.t > 0.0)
        .collect();
    let (gpu_points, ram_points): (Vec<Point>, Vec<Point>) = points
        .iter()
        .copied()
        .partition(|pt| pt.x.gpu / current.gpu >= pt.x.ram / current.ram);

    let fit = fit_with_fixed_time(&gpu_points, &ram_points, &current)
        .unwrap_or_else(|| fit_efficiencies(&gpu_points, &ram_points, &current));
    let efficiency = |e: Option<f64>| e.map(|e| round(e.clamp(EFFICIENCY_MIN, EFFICIENCY_MAX), 2));
    let gpu = efficiency(fit.gpu);
    let ram = efficiency(fit.ram);
    let overhead_ms = fit.fixed_ms.map(|ms| round(ms, 1));
    let current_ms = current.fixed_s * 1000.0;
    let differs =
        |new: Option<f64>, old: f64, step: f64| new.is_some_and(|x| (x - old).abs() >= step);
    let changed = differs(gpu, current.gpu, 0.005)
        || differs(ram, current.ram, 0.005)
        || differs(overhead_ms, current_ms, 0.05);

    let n = points.len();
    let too_fast = |who: &str| {
        format!(
            "{who} plus rapides que ne le permet le temps fixe par token ({} ms) : le réduire dans \
             les paramètres.",
            num(current_ms, 1)
        )
    };
    let summary = if n == 0 {
        "Aucune mesure exploitable : mesurer une quantification déjà installée dans Ollama, \
         depuis la fiche du modèle."
            .to_string()
    } else if gpu.is_none() && ram.is_none() {
        too_fast("Les mesures sont")
    } else {
        let mut parts = Vec::new();
        if let Some(e) = gpu {
            parts.push(format!(
                "efficacité GPU {} (actuelle {})",
                num(e, 2),
                num(current.gpu, 2)
            ));
        }
        if let Some(e) = ram {
            parts.push(format!(
                "efficacité RAM {} (actuelle {})",
                num(e, 2),
                num(current.ram, 2)
            ));
        }
        if let Some(ms) = overhead_ms {
            parts.push(format!(
                "temps fixe par token {} ms (actuel {} ms)",
                num(ms, 1),
                num(current_ms, 1)
            ));
        }
        let mut text = format!(
            "D'après {n} mesure{} : {}.",
            plural(n as u32),
            parts.join(", ")
        );
        if fit.too_fast {
            text.push(' ');
            text.push_str(&too_fast("Certaines mesures sont"));
        }
        // Sans GPU discret, tout passe par la RAM : l'efficacité GPU ne sert pas.
        if budget.gpu_offload() {
            if gpu_points.is_empty() {
                text.push_str(
                    " Efficacité GPU inchangée : aucune mesure d'un modèle tenant en VRAM.",
                );
            }
            if ram_points.is_empty() {
                text.push_str(
                    " Efficacité RAM inchangée : aucune mesure d'un modèle en partie en RAM.",
                );
            }
        }
        if overhead_ms.is_none()
            && spread(&gpu_points, |x| x.gpu) < MIN_SPREAD
            && spread(&ram_points, |x| x.ram) < MIN_SPREAD
        {
            text.push_str(
                " Mesurer aussi un modèle de taille très différente (un MoE, un petit modèle) \
                 permettrait d'ajuster le temps fixe par token.",
            );
        }
        if !changed {
            text.push_str(" Les réglages actuels correspondent déjà aux mesures.");
        }
        text
    };

    Calibration {
        used: n as u32,
        measurements,
        current_gpu_efficiency: p.gpu_efficiency,
        current_ram_efficiency: p.ram_efficiency,
        current_token_overhead_ms: p.token_overhead_ms,
        gpu_efficiency: gpu.map(|e| e as f32),
        ram_efficiency: ram.map(|e| e as f32),
        token_overhead_ms: overhead_ms.map(|ms| ms as f32),
        changed,
        summary,
    }
}

/// Une mesure pour la calibration : lecture des poids à pleine bande passante et temps mesuré
/// par token, en secondes.
#[derive(Debug, Clone, Copy)]
struct Point {
    x: Transfer,
    t: f64,
}

/// Réglages actuels de la formule de vitesse.
struct Speed {
    gpu: f64,
    ram: f64,
    fixed_s: f64,
}

/// Réglages ajustés, avant arrondi ; `None` : réglage actuel gardé.
struct Fit {
    gpu: Option<f64>,
    ram: Option<f64>,
    fixed_ms: Option<f64>,
    /// Les mesures d'un côté sont toutes plus rapides que ne le permet le temps fixe.
    too_fast: bool,
}

/// Rapport entre le plus grand et le plus petit transfert d'un côté (1 sans mesure).
fn spread(points: &[Point], side: impl Fn(Transfer) -> f64) -> f64 {
    let (min, max) = points
        .iter()
        .map(|pt| side(pt.x))
        .fold((f64::INFINITY, 0.0f64), |(lo, hi), x| {
            (lo.min(x), hi.max(x))
        });
    if min > 0.0 && min.is_finite() {
        max / min
    } else {
        1.0
    }
}

/// Moindres carrés sur l'erreur relative (poids `1 / temps²`) de
/// `temps = transfert GPU × g + transfert RAM × r + fixe`, où `g` et `r` sont les inverses des
/// efficacités des côtés mesurés : seulement si un côté a des mesures de tailles assez
/// différentes, et si le résultat est plausible.
fn fit_with_fixed_time(gpu: &[Point], ram: &[Point], current: &Speed) -> Option<Fit> {
    if spread(gpu, |x| x.gpu) < MIN_SPREAD && spread(ram, |x| x.ram) < MIN_SPREAD {
        return None;
    }
    let (fit_gpu, fit_ram) = (!gpu.is_empty(), !ram.is_empty());
    // Inconnues : g, r et le temps fixe. Un côté sans mesure garde son efficacité : son temps
    // de lecture est retiré du temps mesuré, et son inconnue vaut 0 (ligne identité).
    let mut a = [[0.0f64; 3]; 3];
    let mut v = [0.0f64; 3];
    for pt in gpu.iter().chain(ram) {
        let mut target = pt.t;
        let mut row = [pt.x.gpu, pt.x.ram, 1.0];
        if !fit_gpu {
            target -= pt.x.gpu / current.gpu;
            row[0] = 0.0;
        }
        if !fit_ram {
            target -= pt.x.ram / current.ram;
            row[1] = 0.0;
        }
        let w = 1.0 / (pt.t * pt.t);
        for (i, ri) in row.iter().enumerate() {
            v[i] += w * ri * target;
            for (j, rj) in row.iter().enumerate() {
                a[i][j] += w * ri * rj;
            }
        }
    }
    if !fit_gpu {
        a[0][0] = 1.0;
    }
    if !fit_ram {
        a[1][1] = 1.0;
    }
    let [g, r, fixed_s] = solve3(a, v)?;
    let plausible = |fitted: bool, inverse: f64| -> Option<Option<f64>> {
        if !fitted {
            return Some(None);
        }
        let eff = 1.0 / inverse;
        (inverse > 0.0 && (EFFICIENCY_MIN..=EFFICIENCY_MAX).contains(&eff)).then_some(Some(eff))
    };
    let fixed_ms = fixed_s * 1000.0;
    if !(0.0..=MAX_FITTED_OVERHEAD_MS).contains(&fixed_ms) {
        return None;
    }
    Some(Fit {
        gpu: plausible(fit_gpu, g)?,
        ram: plausible(fit_ram, r)?,
        fixed_ms: Some(fixed_ms),
        too_fast: false,
    })
}

/// Système linéaire `a × x = v` à trois inconnues, par la règle de Cramer ; `None` s'il est
/// (presque) singulier : déterminant négligeable devant la borne d'Hadamard (produit des
/// normes des lignes).
fn solve3(a: [[f64; 3]; 3], v: [f64; 3]) -> Option<[f64; 3]> {
    let det = |m: &[[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(&a);
    let bound: f64 = a
        .iter()
        .map(|row| row.iter().map(|x| x * x).sum::<f64>().sqrt())
        .product();
    if !d.is_finite() || d.abs() <= bound * 1e-12 {
        return None;
    }
    let mut x = [0.0; 3];
    for (i, xi) in x.iter_mut().enumerate() {
        let mut m = a;
        for (row, vi) in m.iter_mut().zip(v) {
            row[i] = vi;
        }
        *xi = det(&m) / d;
    }
    x.iter().all(|x| x.is_finite()).then_some(x)
}

/// Efficacités seules, le temps fixe étant connu : pour chaque côté mesuré, médiane de
/// `transfert du côté / (temps − fixe − temps de l'autre côté)`. Le côté GPU d'abord : la
/// petite part en RAM de ses mesures est lue à l'efficacité RAM actuelle ; celle en VRAM des
/// mesures du côté RAM l'est à l'efficacité GPU tout juste ajustée.
fn fit_efficiencies(gpu: &[Point], ram: &[Point], current: &Speed) -> Fit {
    let side = |pt: &Point, own: f64, other: f64| {
        let left = pt.t - current.fixed_s - other;
        (left > 0.0).then(|| own / left)
    };
    let gpu_eff = median(
        gpu.iter()
            .filter_map(|pt| side(pt, pt.x.gpu, pt.x.ram / current.ram)),
    );
    let gpu_now = gpu_eff.unwrap_or(current.gpu);
    let ram_eff = median(
        ram.iter()
            .filter_map(|pt| side(pt, pt.x.ram, pt.x.gpu / gpu_now)),
    );
    Fit {
        too_fast: (!gpu.is_empty() && gpu_eff.is_none()) || (!ram.is_empty() && ram_eff.is_none()),
        gpu: gpu_eff,
        ram: ram_eff,
        fixed_ms: None,
    }
}

fn median(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut values: Vec<f64> = values.collect();
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
