//! Description d'un modèle du catalogue (rempli par `gram-hub`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Un modèle de base, avec toutes ses quantifications GGUF.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ModelEntry {
    /// Dépôt canonique (ex. « Qwen/Qwen3-8B-GGUF ») : organisation officielle, sinon uploader
    /// prioritaire, sinon le plus téléchargé.
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
    /// Toujours `General`, plus les cas d'usage déduits du nom, des tags et du pipeline.
    pub use_cases: Vec<UseCase>,
    /// Somme des téléchargements (30 jours) de tous les dépôts du groupe.
    pub downloads: u64,
    pub likes: u64,
    /// Création du plus ancien dépôt du groupe (approximation de la sortie du modèle).
    pub created_at: Option<DateTime<Utc>>,
    pub last_modified: Option<DateTime<Utc>>,
    /// Dépôt protégé : il faut accepter les conditions (et un token) pour télécharger.
    pub gated: bool,
    /// Une entrée par quantification, de la plus petite à la plus grosse, réunies depuis le
    /// dépôt canonique et les alternatives détaillées (le dépôt prioritaire l'emporte pour un
    /// même libellé). Vide tant qu'aucun détail n'a été chargé (tailles réelles inconnues).
    pub variants: Vec<QuantVariant>,
    /// Taille du projecteur vision (`mmproj-*.gguf`), en octets.
    pub mmproj_bytes: Option<u64>,
    /// Autres dépôts GGUF du même modèle de base, par ordre de priorité.
    pub alternatives: Vec<RepoRef>,
}

/// Un dépôt GGUF alternatif d'un modèle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RepoRef {
    pub repo_id: String,
    /// Commit vu dans la liste (clé du cache des détails).
    pub sha: String,
    pub downloads: u64,
    pub gated: bool,
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
    /// Têtes KV (le maximum si le nombre varie selon la couche).
    pub n_kv_heads: u32,
    pub head_dim: u32,
    /// Couches qui ont réellement un cache KV, quand ce n'est pas toutes (modèles hybrides
    /// attention + Mamba / attention linéaire). `None` = toutes les couches.
    pub kv_layers: Option<u32>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum UseCase {
    #[default]
    General,
    Code,
    Reasoning,
    FastChat,
    Vision,
}

impl UseCase {
    pub const ALL: [UseCase; 5] = [
        UseCase::General,
        UseCase::Code,
        UseCase::Reasoning,
        UseCase::FastChat,
        UseCase::Vision,
    ];

    /// Nom affiché.
    pub const fn label(self) -> &'static str {
        match self {
            Self::General => "Polyvalent",
            Self::Code => "Code",
            Self::Reasoning => "Raisonnement",
            Self::FastChat => "Chat rapide",
            Self::Vision => "Vision",
        }
    }
}

/// Une quantification d'un dépôt GGUF.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct QuantVariant {
    /// Ex. « Q4_K_M », « UD-Q4_K_XL », « IQ4_XS ».
    pub label: String,
    /// Dépôt qui fournit ces fichiers : le canonique ou une alternative
    /// (`ollama run hf.co/{repo_id}:{label}`).
    pub repo_id: String,
    /// Chemins dans le dépôt ; plusieurs fichiers si le modèle est découpé.
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
