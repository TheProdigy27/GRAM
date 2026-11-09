//! Scoring (§9 du plan) : quatre axes notés de 0 à 100, combinés par des poids qui dépendent
//! du cas d'usage, plus un petit bonus de popularité.
//!
//! `total = (moyenne pondérée × 0,95 + popularité) × facteur d'usage × confiance`, toujours
//! entre 0 et 100 sans écrêtage :
//!
//! - la popularité (0 à 5 points) départage sans saturer le score ;
//! - un modèle qui ne correspond pas au cas d'usage Code ou Raisonnement est pénalisé (× 0,92) ;
//! - une estimation approximative (tailles ou architecture inconnues) est pénalisée (× 0,95) :
//!   à mérite comparable, les modèles dont les fichiers ont été détaillés passent devant.
//!
//! - **Qualité Q** : heuristique (taille effective, quantification, récence, curation) ;
//!   Hugging Face n'expose pas de score de qualité unifié ;
//! - **Vitesse V** : `100 × ln(tok/s ÷ 2) / ln(20)` (2 tok/s → 0, 40 tok/s → 100) ;
//! - **Marge F** : place laissée aux autres applications ;
//! - **Contexte C** : part du contexte souhaité atteignable.

use crate::estimate::{Estimate, QuantTable, RunMode};
use crate::model::ModelEntry;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use ts_rs::TS;

const CURATION_JSON: &str = include_str!("../../../data/quality_overrides.json");

/// Bonus de popularité maximal, en points. La moyenne pondérée est ramenée sur
/// `100 − MAX_POPULARITY_BONUS` pour que le total ne dépasse jamais 100.
pub const MAX_POPULARITY_BONUS: f32 = 5.0;

/// Facteur appliqué à un modèle qui ne correspond pas au cas d'usage choisi (Code,
/// Raisonnement) : il reste affiché, mais ceux qui correspondent sont favorisés (§9).
pub const USE_CASE_MISMATCH_FACTOR: f32 = 0.92;

/// Facteur appliqué à une estimation approximative (taille estimée ou architecture inconnue).
pub const APPROXIMATE_FACTOR: f32 = 0.95;

/// Récence (§9) : +5 points pour un modèle de moins de 6 mois, −10 au-delà de 18 mois.
const RECENT_DAYS: i64 = 183;
const OLD_DAYS: i64 = 548;
const RECENT_BONUS: f32 = 5.0;
const OLD_MALUS: f32 = -10.0;

/// Poids des quatre axes. Ils n'ont pas besoin de faire 1 : le score est une moyenne pondérée.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScoreWeights {
    pub quality: f32,
    pub speed: f32,
    pub fit: f32,
    pub context: f32,
}

impl ScoreWeights {
    pub const fn new(quality: f32, speed: f32, fit: f32, context: f32) -> Self {
        Self {
            quality,
            speed,
            fit,
            context,
        }
    }

    fn values(&self) -> [f32; 4] {
        [self.quality, self.speed, self.fit, self.context]
    }

    pub fn sum(&self) -> f32 {
        self.values().iter().sum()
    }

    /// Tous les poids sont finis, positifs ou nuls, et l'un au moins est non nul.
    pub fn is_valid(&self) -> bool {
        self.values().iter().all(|w| w.is_finite() && *w >= 0.0) && self.sum() > 0.0
    }

    /// Moyenne pondérée des quatre axes (0 à 100).
    pub fn combine(&self, axes: &Axes) -> f32 {
        let sum = self.sum();
        if !self.is_valid() {
            return 0.0;
        }
        (self.quality * axes.quality
            + self.speed * axes.speed
            + self.fit * axes.fit
            + self.context * axes.context)
            / sum
    }
}

/// Les quatre axes, de 0 à 100.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axes {
    pub quality: f32,
    pub speed: f32,
    pub fit: f32,
    pub context: f32,
}

/// Détail du score d'une quantification (infobulle du classement).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Score {
    /// Qualité, de 0 à 100.
    pub quality: f32,
    /// Vitesse, de 0 à 100.
    pub speed: f32,
    /// Marge mémoire, de 0 à 100.
    pub fit: f32,
    /// Contexte, de 0 à 100.
    pub context: f32,
    /// Poids du cas d'usage.
    pub weights: ScoreWeights,
    /// Moyenne pondérée des quatre axes.
    pub weighted: f32,
    /// Popularité, de 0 à 5 points.
    pub popularity: f32,
    /// 1 si le modèle correspond au cas d'usage (ou si le cas d'usage est général), 0,92 sinon.
    pub use_case_factor: f32,
    /// 1 pour une estimation sur tailles et architecture réelles, 0,95 si elle est approximative.
    pub confidence: f32,
    /// Score affiché, de 0 à 100.
    pub total: f32,
}

/// Paramètres effectifs (§9) : le total pour un modèle dense, `√(total × actifs)` pour un MoE.
pub fn params_eff(params_total: u64, params_active: Option<u64>) -> f64 {
    let total = params_total as f64;
    match params_active.filter(|&a| a > 0 && a < params_total) {
        Some(active) => (total * active as f64).sqrt(),
        None => total,
    }
}

/// `clamp(40 + 12 × log2(P en milliards), 0, 100)` : 1B → 40, 8B → 76, 32B → 100.
pub fn size_factor(params_eff: f64) -> f32 {
    if params_eff <= 0.0 {
        return 0.0;
    }
    (40.0 + 12.0 * (params_eff / 1e9).log2()).clamp(0.0, 100.0) as f32
}

/// +5 points sous 6 mois, −10 au-delà de 18 mois, 0 entre les deux ou si la date est inconnue.
pub fn recency_bonus(created_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> f32 {
    let Some(created) = created_at else {
        return 0.0;
    };
    match (now - created).num_days() {
        d if d < RECENT_DAYS => RECENT_BONUS,
        d if d > OLD_DAYS => OLD_MALUS,
        _ => 0.0,
    }
}

/// Bits par poids d'une quantification : la valeur mesurée, sinon `taille × 8 / paramètres`.
pub fn effective_bpw(bpw: f32, size_bytes: u64, params_total: u64) -> Option<f32> {
    if bpw > 0.0 && bpw.is_finite() {
        Some(bpw)
    } else if params_total > 0 && size_bytes > 0 {
        Some((size_bytes as f64 * 8.0 / params_total as f64) as f32)
    } else {
        None
    }
}

/// Qualité Q = `clamp(f_taille × f_quant + récence + curation, 0, 100)`. Sans bits par poids
/// connus, `f_quant` prend la valeur d'une quantification moyenne (Q4_K_M).
pub fn quality(
    model: &ModelEntry,
    bpw: Option<f32>,
    curation: f32,
    now: DateTime<Utc>,
    table: &QuantTable,
) -> f32 {
    let size = size_factor(params_eff(model.params_total, model.params_active));
    let f_quant = table.quality_factor(bpw.unwrap_or(4.8));
    (size * f_quant + recency_bonus(model.created_at, now) + curation).clamp(0.0, 100.0)
}

/// Vitesse V = `clamp(100 × ln(tok/s ÷ 2) / ln(20), 0, 100)`.
pub fn speed_score(tok_s: f32) -> f32 {
    if tok_s.is_nan() || tok_s <= 2.0 {
        return 0.0;
    }
    (100.0 * (f64::from(tok_s) / 2.0).ln() / 20f64.ln()).clamp(0.0, 100.0) as f32
}

/// Marge F, avec `u` la part de la mémoire du mode utilisée : 100 jusqu'à 0,8, puis de 100 à
/// 60 jusqu'à 0,95, puis 40. Un modèle trop gros obtient 0.
pub fn fit_score(mode: RunMode, mem_usage: f32) -> f32 {
    match mem_usage {
        _ if mode == RunMode::TooBig => 0.0,
        u if u <= 0.80 => 100.0,
        u if u <= 0.95 => 100.0 - (u - 0.80) / 0.15 * 40.0,
        _ => 40.0,
    }
}

/// Contexte C = `100 × min(1, contexte max ÷ contexte souhaité)`.
pub fn context_score(ctx_max: u32, ctx_wanted: u32) -> f32 {
    if ctx_wanted == 0 {
        return 100.0;
    }
    (100.0 * f64::from(ctx_max) / f64::from(ctx_wanted)).min(100.0) as f32
}

/// Popularité : de 0 point (100 téléchargements ou moins) à 5 points (1 million ou plus),
/// linéaire en `log10(téléchargements)`.
pub fn popularity_bonus(downloads: u64) -> f32 {
    if downloads == 0 {
        return 0.0;
    }
    let x = ((downloads as f64).log10() - 2.0) / 4.0;
    (x.clamp(0.0, 1.0) as f32) * MAX_POPULARITY_BONUS
}

/// Les quatre axes d'une estimation.
pub fn axes(
    model: &ModelEntry,
    bpw: Option<f32>,
    estimate: &Estimate,
    ctx_wanted: u32,
    curation: f32,
    now: DateTime<Utc>,
    table: &QuantTable,
) -> Axes {
    Axes {
        quality: quality(model, bpw, curation, now, table),
        speed: speed_score(estimate.tok_s),
        fit: fit_score(estimate.mode, estimate.mem_usage),
        context: context_score(estimate.ctx_max, ctx_wanted),
    }
}

/// Ce qui module le score en plus des quatre axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Modifiers {
    pub downloads: u64,
    /// `false` pour un modèle qui ne correspond pas au cas d'usage Code ou Raisonnement choisi.
    pub use_case_match: bool,
    /// Estimation approximative (taille estimée ou architecture inconnue).
    pub approximate: bool,
}

/// Score total = `(moyenne pondérée × 0,95 + popularité) × facteur d'usage × confiance`, de 0
/// à 100.
pub fn score(axes: Axes, weights: ScoreWeights, m: Modifiers) -> Score {
    let weighted = weights.combine(&axes);
    let popularity = popularity_bonus(m.downloads);
    let use_case_factor = if m.use_case_match {
        1.0
    } else {
        USE_CASE_MISMATCH_FACTOR
    };
    let confidence = if m.approximate {
        APPROXIMATE_FACTOR
    } else {
        1.0
    };
    let room = (100.0 - MAX_POPULARITY_BONUS) / 100.0;
    Score {
        quality: axes.quality,
        speed: axes.speed,
        fit: axes.fit,
        context: axes.context,
        weights,
        weighted,
        popularity,
        use_case_factor,
        confidence,
        total: ((weighted * room + popularity) * use_case_factor * confidence).clamp(0.0, 100.0),
    }
}

#[derive(Deserialize)]
struct CurationFile {
    families: std::collections::BTreeMap<String, f32>,
}

/// Bonus manuels par famille (`data/quality_overrides.json`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Curation {
    /// Motifs en minuscules, du plus long au plus court.
    families: Vec<(String, f32)>,
}

impl Curation {
    /// Table embarquée dans le binaire.
    pub fn embedded() -> &'static Curation {
        static TABLE: OnceLock<Curation> = OnceLock::new();
        TABLE.get_or_init(|| {
            Curation::from_json(CURATION_JSON).expect("data/quality_overrides.json invalide")
        })
    }

    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        let file: CurationFile = serde_json::from_str(json)?;
        let mut families: Vec<(String, f32)> = file
            .families
            .into_iter()
            .map(|(k, v)| (k.trim().to_lowercase(), v))
            .filter(|(k, v)| !k.is_empty() && v.is_finite())
            .collect();
        families.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
        Ok(Self { families })
    }

    /// Bonus du motif le plus long trouvé dans le modèle de base, le nom ou le dépôt.
    pub fn bonus(&self, model: &ModelEntry) -> f32 {
        if self.families.is_empty() {
            return 0.0;
        }
        let haystacks: Vec<String> = [
            model.base_model.as_deref(),
            Some(model.display_name.as_str()),
            Some(model.repo_id.as_str()),
        ]
        .into_iter()
        .flatten()
        .map(str::to_lowercase)
        .collect();
        self.families
            .iter()
            .find(|(pattern, _)| haystacks.iter().any(|h| h.contains(pattern.as_str())))
            .map_or(0.0, |(_, bonus)| *bonus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn facteur_de_taille() {
        assert!(close(size_factor(1e9), 40.0));
        assert!(close(size_factor(8e9), 76.0));
        assert!(close(size_factor(32e9), 100.0));
        assert_eq!(size_factor(70e9), 100.0);
        assert!(close(size_factor(0.5e9), 28.0));
        assert_eq!(size_factor(0.0), 0.0);
        // MoE : √(30 × 3) ≈ 9,5B effectifs.
        let eff = params_eff(30_000_000_000, Some(3_000_000_000));
        assert!((eff / 1e9 - 90f64.sqrt()).abs() < 1e-6);
        assert_eq!(params_eff(8_000_000_000, None), 8e9);
        // Actifs incohérents (≥ total) : modèle dense.
        assert_eq!(params_eff(8, Some(8)), 8.0);
    }

    #[test]
    fn facteur_de_quantification() {
        let t = QuantTable::embedded();
        assert_eq!(t.quality_factor(16.0), 1.0);
        assert_eq!(t.quality_factor(8.0), 1.0);
        assert!(close(t.quality_factor(6.5), 0.99));
        assert!(close(t.quality_factor(4.8), 0.96));
        assert!(close(t.quality_factor(3.45), 0.86), "milieu de 3,0 et 3,9");
        assert_eq!(t.quality_factor(2.6), 0.70);
        assert_eq!(t.quality_factor(1.5), 0.70);
        assert_eq!(t.quality_factor(f32::NAN), 0.70);
        // Croissant.
        let mut prev = 0.0;
        for i in 0..200 {
            let f = t.quality_factor(i as f32 * 0.05);
            assert!(f >= prev);
            prev = f;
        }
    }

    #[test]
    fn recence() {
        let now = Utc.with_ymd_and_hms(2026, 11, 9, 0, 0, 0).unwrap();
        let ago = |days| Some(now - chrono::Duration::days(days));
        assert_eq!(recency_bonus(ago(30), now), 5.0);
        assert_eq!(recency_bonus(ago(182), now), 5.0);
        assert_eq!(recency_bonus(ago(300), now), 0.0);
        assert_eq!(recency_bonus(ago(549), now), -10.0);
        assert_eq!(recency_bonus(None, now), 0.0);
    }

    #[test]
    fn vitesse() {
        assert_eq!(speed_score(0.0), 0.0);
        assert_eq!(speed_score(2.0), 0.0);
        assert!(
            close(speed_score(2.0 * 20f32.sqrt()), 50.0),
            "~9 tok/s → 50"
        );
        assert!(close(speed_score(40.0), 100.0));
        assert_eq!(speed_score(300.0), 100.0);
        assert_eq!(speed_score(f32::NAN), 0.0);
    }

    #[test]
    fn marge() {
        assert_eq!(fit_score(RunMode::Gpu, 0.5), 100.0);
        assert_eq!(fit_score(RunMode::Gpu, 0.8), 100.0);
        assert!(close(fit_score(RunMode::Gpu, 0.875), 80.0));
        assert!(close(fit_score(RunMode::Cpu, 0.95), 60.0));
        assert_eq!(fit_score(RunMode::Cpu, 0.96), 40.0);
        assert_eq!(fit_score(RunMode::TooBig, 0.5), 0.0);
    }

    #[test]
    fn contexte() {
        assert_eq!(context_score(32_768, 16_384), 100.0);
        assert_eq!(context_score(8192, 16_384), 50.0);
        assert_eq!(context_score(0, 8192), 0.0);
        assert_eq!(context_score(0, 0), 100.0);
    }

    #[test]
    fn popularite() {
        assert_eq!(popularity_bonus(0), 0.0);
        assert_eq!(popularity_bonus(100), 0.0);
        assert!(close(popularity_bonus(10_000), 2.5));
        assert_eq!(popularity_bonus(1_000_000), 5.0);
        assert_eq!(popularity_bonus(50_000_000), 5.0);
    }

    #[test]
    fn combinaison() {
        let axes = Axes {
            quality: 80.0,
            speed: 60.0,
            fit: 100.0,
            context: 50.0,
        };
        let w = ScoreWeights::new(0.45, 0.30, 0.15, 0.10);
        let m = |downloads, use_case_match, approximate| Modifiers {
            downloads,
            use_case_match,
            approximate,
        };
        let s = score(axes, w, m(1_000_000, true, false));
        assert!(close(
            s.weighted,
            0.45 * 80.0 + 0.30 * 60.0 + 0.15 * 100.0 + 0.10 * 50.0
        ));
        assert!(close(s.total, s.weighted * 0.95 + 5.0));
        // Les poids n'ont pas besoin de faire 1.
        let doubled = ScoreWeights::new(0.9, 0.6, 0.3, 0.2);
        assert!(close(doubled.combine(&axes), s.weighted));
        // Un modèle qui ne correspond pas au cas d'usage est pénalisé.
        let other = score(axes, w, m(1_000_000, false, false));
        assert!(close(other.total, s.total * 0.92));
        assert_eq!(other.use_case_factor, 0.92);
        // Une estimation approximative aussi.
        let approx = score(axes, w, m(1_000_000, true, true));
        assert!(close(approx.total, s.total * 0.95));
        assert_eq!(approx.confidence, 0.95);
        // Le maximum vaut exactement 100, sans écrêtage ; la popularité départage.
        let top = Axes {
            quality: 100.0,
            speed: 100.0,
            fit: 100.0,
            context: 100.0,
        };
        assert!(close(score(top, w, m(1_000_000, true, false)).total, 100.0));
        assert!(score(top, w, m(10_000, true, false)).total < 100.0);
        assert!(!ScoreWeights::new(0.0, 0.0, 0.0, 0.0).is_valid());
        assert!(!ScoreWeights::new(-1.0, 1.0, 1.0, 1.0).is_valid());
    }

    #[test]
    fn curation() {
        let c = Curation::from_json(
            r#"{ "families": { "qwen3": 2, "Qwen3-Coder": 4, "llama-2": -10, " ": 9 } }"#,
        )
        .unwrap();
        let mut m: ModelEntry = serde_json::from_str(include_str!(
            "../../../tests/fixtures/models/Qwen--Qwen3-8B-GGUF.json"
        ))
        .unwrap();
        assert_eq!(c.bonus(&m), 2.0);
        m.display_name = "Qwen3-Coder-30B-A3B-Instruct".into();
        assert_eq!(c.bonus(&m), 4.0, "le motif le plus long l'emporte");
        m.display_name = "Mistral".into();
        m.repo_id = "x/Mistral-GGUF".into();
        m.base_model = None;
        assert_eq!(c.bonus(&m), 0.0);
        assert_eq!(Curation::embedded().bonus(&m), 0.0, "vide au départ");
    }
}
