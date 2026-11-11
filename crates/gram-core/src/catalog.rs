//! Synchronisation du catalogue Hugging Face : résumé, progression et origine des données
//! (commande `sync_catalog` et événements `sync-progress` / `rate-limit`, §5.3 du plan).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// D'où vient le catalogue affiché (barre d'état : « en ligne / cache / snapshot »).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DataSourceKind {
    /// Synchronisé à l'instant depuis Hugging Face.
    Online,
    /// Lu dans le cache local (`fr.gram.desktop/cache` des données locales).
    Cache,
    /// Catalogue de démarrage embarqué dans l'application.
    Snapshot,
}

impl DataSourceKind {
    /// Nom affiché dans la barre d'état.
    pub const fn label(self) -> &'static str {
        match self {
            DataSourceKind::Online => "en ligne",
            DataSourceKind::Cache => "cache",
            DataSourceKind::Snapshot => "catalogue embarqué",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DataSource {
    pub kind: DataSourceKind,
    /// Date de la synchronisation qui a produit ce catalogue.
    pub fetched_at: DateTime<Utc>,
}

/// Étape de la synchronisation, pour la barre de progression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SyncPhase {
    /// Listes de modèles GGUF par tranche de taille.
    Listing,
    /// Détail des dépôts (tailles réelles des fichiers).
    Details,
    /// Métadonnées d'architecture (en-têtes GGUF).
    Architecture,
}

/// Événement `sync-progress`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncProgress {
    pub phase: SyncPhase,
    pub done: u32,
    pub total: u32,
}

/// Événement `rate-limit` : quota restant de l'API Hub sur la fenêtre en cours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RateLimitInfo {
    pub remaining: u32,
    /// Secondes avant la remise à zéro du quota.
    pub reset_s: u32,
}

/// Résultat de `sync_catalog`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncSummary {
    pub source: DataSource,
    /// Dépôts GGUF retenus après filtrage.
    pub repos: u32,
    /// Modèles (groupes de dépôts d'un même modèle de base).
    pub models: u32,
    /// Modèles dont les tailles réelles par quantification sont connues.
    pub detailed: u32,
    /// Modèles dont l'architecture est connue (calcul du cache KV).
    pub with_arch: u32,
    /// Appels à l'API Hub pendant cette synchronisation.
    pub api_calls: u32,
    /// Appels aux resolvers (en-têtes GGUF, `config.json`).
    pub resolver_calls: u32,
    /// Dernier quota connu de l'API Hub.
    pub rate_limit: Option<RateLimitInfo>,
    pub duration_ms: u64,
    /// Problèmes non bloquants, en français, affichés tels quels.
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn progression_en_camel_case() {
        let p = SyncProgress {
            phase: SyncPhase::Architecture,
            done: 3,
            total: 150,
        };
        assert_eq!(
            serde_json::to_value(p).unwrap(),
            json!({ "phase": "architecture", "done": 3, "total": 150 })
        );
        let r = RateLimitInfo {
            remaining: 420,
            reset_s: 12,
        };
        assert_eq!(
            serde_json::to_value(r).unwrap(),
            json!({ "remaining": 420, "resetS": 12 })
        );
    }
}
