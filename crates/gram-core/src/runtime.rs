//! Runtimes locaux (Ollama, LM Studio, llama.cpp) détectés sur la machine.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// État des runtimes pris en charge (§6.4 du plan).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RuntimesStatus {
    pub ollama: RuntimeStatus,
    pub lm_studio: RuntimeStatus,
    pub llama_cpp: RuntimeStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RuntimeStatus {
    pub installed: bool,
    /// Processus en cours d'exécution, ou API locale qui répond.
    pub running: bool,
    /// Exécutable trouvé (CLI ou application).
    pub path: Option<String>,
    /// Version, quand le runtime l'expose (API locale d'Ollama).
    pub version: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn champs_en_camel_case() {
        let json = serde_json::to_value(RuntimesStatus::default()).unwrap();
        assert!(json.get("lmStudio").is_some());
        assert!(json.get("llamaCpp").is_some());
        assert_eq!(json["ollama"]["installed"], false);
    }
}
