//! Résultat de l'estimation pour un couple (modèle, quantification).
//!
//! Les formules arrivent en phase 3 (§8 du plan).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
    /// Une partie des couches sur le GPU (option `-ngl` de llama.cpp), le reste en RAM.
    Hybrid { gpu_layers: u32 },
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
    /// Autres cas hybrides, ou CPU.
    Marginal,
    /// Ne tient pas.
    TooTight,
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
    /// Contexte retenu pour l'estimation, en tokens.
    pub ctx_used: u32,
    /// Contexte maximal atteignable dans ce mode, en tokens.
    pub ctx_max: u32,
    /// Vitesse de génération estimée.
    pub tok_s: f32,
    pub fit: FitLevel,
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn run_mode_etiquete() {
        assert_eq!(
            serde_json::to_value(RunMode::Hybrid { gpu_layers: 20 }).unwrap(),
            json!({ "kind": "hybrid", "gpuLayers": 20 })
        );
        assert_eq!(
            serde_json::to_value(RunMode::TooBig).unwrap(),
            json!({ "kind": "tooBig" })
        );
    }
}
