//! Description d'un modèle du catalogue (rempli par `gram-hub`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Un modèle de base, avec toutes ses quantifications GGUF.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ModelEntry {
    /// Dépôt canonique (ex. « bartowski/Qwen_Qwen3-8B-GGUF »).
    pub repo_id: String,
    pub sha: String,
    /// `cardData.base_model` (ex. « Qwen/Qwen3-8B »).
    pub base_model: Option<String>,
    pub display_name: String,
    pub params_total: u64,
    /// Paramètres actifs par token (MoE).
    pub params_active: Option<u64>,
    pub arch: Option<ArchInfo>,
    /// Contexte d'entraînement, en tokens.
    pub ctx_train: u32,
    pub pipeline: Pipeline,
    pub use_cases: Vec<UseCase>,
    pub downloads: u64,
    pub likes: u64,
    pub created_at: Option<DateTime<Utc>>,
    pub last_modified: Option<DateTime<Utc>>,
    /// Une entrée par quantification.
    pub variants: Vec<QuantVariant>,
    /// Taille du projecteur vision (`mmproj-*.gguf`), en octets.
    pub mmproj_bytes: Option<u64>,
    /// Autres dépôts GGUF du même modèle de base.
    pub alternatives: Vec<String>,
}

/// Métadonnées d'architecture, nécessaires au calcul du cache KV.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ArchInfo {
    /// `general.architecture` (ex. « qwen3 »).
    pub architecture: String,
    pub n_layers: u32,
    pub n_heads: u32,
    pub n_kv_heads: u32,
    pub head_dim: u32,
    pub embedding_length: Option<u32>,
    pub expert_count: Option<u32>,
    pub expert_used_count: Option<u32>,
    pub sliding_window: Option<u32>,
    pub source: ArchSource,
}

/// D'où viennent les métadonnées d'architecture (§7.4 du plan).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ArchSource {
    GgufHeader,
    ConfigJson,
    NameHeuristic,
    /// Estimation prudente : badge « estimation approximative » dans l'interface.
    Fallback,
}

/// `pipeline_tag` Hugging Face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum Pipeline {
    TextGeneration,
    ImageTextToText,
}

/// Cas d'usage : polyvalent, code, raisonnement, chat rapide, vision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum UseCase {
    General,
    Code,
    Reasoning,
    FastChat,
    Vision,
}

/// Une quantification d'un dépôt GGUF.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct QuantVariant {
    /// Ex. « Q4_K_M », « UD-Q4_K_XL », « IQ4_XS ».
    pub label: String,
    /// Plusieurs fichiers si le modèle est découpé.
    pub files: Vec<String>,
    pub size_bytes: u64,
    /// Bits par poids réels = taille × 8 / paramètres.
    pub bpw: f32,
    /// `true` si la taille vient des fichiers réels, `false` si elle est estimée.
    pub exact: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_au_format_hugging_face() {
        assert_eq!(
            serde_json::to_value(Pipeline::ImageTextToText).unwrap(),
            "image-text-to-text"
        );
        let p: Pipeline = serde_json::from_str("\"text-generation\"").unwrap();
        assert_eq!(p, Pipeline::TextGeneration);
    }

    #[test]
    fn use_case_en_camel_case() {
        assert_eq!(serde_json::to_value(UseCase::FastChat).unwrap(), "fastChat");
    }
}
