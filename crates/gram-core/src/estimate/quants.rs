//! Quantifications à évaluer pour un modèle (§8.5) et table `data/quant_quality.json`
//! (échelle de repli, et facteur de qualité du §9).

use super::EstimateParams;
use crate::model::{ModelEntry, QuantVariant};
use serde::Deserialize;
use std::sync::OnceLock;

const EMBEDDED: &str = include_str!("../../../../data/quant_quality.json");

/// Une quantification de l'échelle de repli, avec ses bits par poids typiques.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TypicalQuant {
    pub label: String,
    pub bpw: f32,
}

/// Un point de la courbe de qualité : facteur `f_quant` pour un nombre de bits par poids.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct QualityPoint {
    pub bpw: f32,
    pub factor: f32,
}

#[derive(Deserialize)]
struct QuantFile {
    fallback: Vec<TypicalQuant>,
    quality: Vec<QualityPoint>,
}

/// Contenu de `data/quant_quality.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct QuantTable {
    fallback: Vec<TypicalQuant>,
    /// Triée par bits par poids croissants.
    quality: Vec<QualityPoint>,
}

impl QuantTable {
    /// Table embarquée dans le binaire.
    pub fn embedded() -> &'static QuantTable {
        static TABLE: OnceLock<QuantTable> = OnceLock::new();
        TABLE.get_or_init(|| {
            QuantTable::from_json(EMBEDDED).expect("data/quant_quality.json invalide")
        })
    }

    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        let mut file: QuantFile = serde_json::from_str(json)?;
        if file.fallback.is_empty() || file.fallback.iter().any(|q| q.bpw <= 0.0) {
            return Err(serde::de::Error::custom(
                "échelle de repli vide, ou bits par poids non positifs",
            ));
        }
        let valid =
            |p: &QualityPoint| p.bpw.is_finite() && p.bpw > 0.0 && (0.0..=1.0).contains(&p.factor);
        if file.quality.is_empty() || !file.quality.iter().all(valid) {
            return Err(serde::de::Error::custom(
                "courbe de qualité vide, ou points hors bornes (facteur entre 0 et 1)",
            ));
        }
        file.quality.sort_by(|a, b| a.bpw.total_cmp(&b.bpw));
        Ok(Self {
            fallback: file.fallback,
            quality: file.quality,
        })
    }

    /// Facteur de qualité `f_quant` (§9) : interpolation linéaire sur les bits par poids
    /// réels, constant au-delà des extrémités de la courbe.
    pub fn quality_factor(&self, bpw: f32) -> f32 {
        let (first, last) = (self.quality[0], self.quality[self.quality.len() - 1]);
        if bpw.is_nan() || bpw <= first.bpw {
            return first.factor;
        }
        if bpw >= last.bpw {
            return last.factor;
        }
        let i = self.quality.partition_point(|p| p.bpw <= bpw);
        let (a, b) = (self.quality[i - 1], self.quality[i]);
        a.factor + (b.factor - a.factor) * (bpw - a.bpw) / (b.bpw - a.bpw)
    }

    /// Échelle de repli, de la quantification la plus fidèle à la plus compacte.
    pub fn fallback(&self) -> &[TypicalQuant] {
        &self.fallback
    }
}

/// Variantes à évaluer, de la plus petite à la plus grosse :
///
/// - les variantes réelles du dépôt, sauf celles sous le minimum de bits par poids (qualité trop
///   dégradée ; une variante dont les bits par poids sont inconnus est gardée) ;
/// - à défaut (fichiers pas encore détaillés), l'échelle de repli de `quant_quality.json`, avec
///   des tailles estimées (`exact: false`) à partir du nombre de paramètres.
///
/// Vide si le modèle n'a ni variantes ni nombre de paramètres.
pub fn candidate_variants(
    model: &ModelEntry,
    p: &EstimateParams,
    table: &QuantTable,
) -> Vec<QuantVariant> {
    let keep = |bpw: f32| bpw <= 0.0 || bpw >= p.min_bpw;
    if !model.variants.is_empty() {
        return model
            .variants
            .iter()
            .filter(|v| keep(v.bpw))
            .cloned()
            .collect();
    }
    if model.params_total == 0 {
        return Vec::new();
    }
    table
        .fallback()
        .iter()
        .rev()
        .filter(|q| keep(q.bpw))
        .map(|q| QuantVariant {
            label: q.label.clone(),
            repo_id: model.repo_id.clone(),
            files: Vec::new(),
            size_bytes: (model.params_total as f64 * f64::from(q.bpw) / 8.0).ceil() as u64,
            bpw: q.bpw,
            exact: false,
        })
        .collect()
}
