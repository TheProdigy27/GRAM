//! Runtimes locaux (Ollama, LM Studio, llama.cpp) détectés sur la machine, et modèles déjà
//! installés dans Ollama (phase 7 du plan).

use crate::model::ModelEntry;
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
    /// Modèles installés dans Ollama (`GET /api/tags`) ; vide si son API ne répond pas.
    #[serde(default)]
    pub ollama_models: Vec<OllamaModel>,
}

/// Un modèle installé dans Ollama.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OllamaModel {
    /// Nom dans Ollama : `hf.co/bartowski/Qwen_Qwen3-8B-GGUF:Q4_K_M`, `qwen3:8b`…
    pub name: String,
    /// Dépôt Hugging Face, pour un modèle installé depuis `hf.co/…` (c'est ce que font les
    /// commandes de GRAM) ; absent pour un modèle de la bibliothèque d'Ollama.
    pub repo_id: Option<String>,
    /// Quantification : l'étiquette du nom (`Q4_K_M`), sinon celle qu'annonce Ollama.
    pub quant: Option<String>,
    /// Taille sur le disque, en octets.
    pub size: u64,
}

/// Préfixes des modèles installés depuis Hugging Face (`ollama pull hf.co/{dépôt}:{quant}`).
const HF_PREFIXES: [&str; 2] = ["hf.co/", "huggingface.co/"];

impl OllamaModel {
    /// `quantization_level` : champ `details.quantization_level` de `/api/tags`, utilisé quand
    /// le nom ne précise pas la quantification (étiquette absente ou `latest`).
    pub fn new(name: impl Into<String>, size: u64, quantization_level: Option<&str>) -> Self {
        let name = name.into();
        let (repo_id, tag) = parse_ollama_name(&name);
        let quant = tag
            .filter(|t| !t.eq_ignore_ascii_case("latest"))
            .or(quantization_level)
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(str::to_string);
        Self {
            repo_id: repo_id.map(str::to_string),
            quant,
            name,
            size,
        }
    }

    /// Même quantification que l'étiquette d'une variante (sans tenir compte de la casse :
    /// `ollama pull hf.co/…:q4_k_m` est accepté).
    pub fn is_quant(&self, label: &str) -> bool {
        self.quant
            .as_deref()
            .is_some_and(|q| q.eq_ignore_ascii_case(label))
    }
}

/// `hf.co/owner/repo:TAG` → (`owner/repo`, `TAG`) ; `qwen3:8b` → (aucun dépôt, `8b`).
fn parse_ollama_name(name: &str) -> (Option<&str>, Option<&str>) {
    let name = name.trim();
    // L'étiquette suit le dernier `:` qui n'est pas dans le chemin (`hôte:port/…` reste entier).
    let (path, tag) = match name.rsplit_once(':') {
        Some((p, t)) if !t.contains('/') => (p, Some(t).filter(|t| !t.is_empty())),
        _ => (name, None),
    };
    let repo = HF_PREFIXES.iter().find_map(|prefix| {
        let head = path.get(..prefix.len())?;
        head.eq_ignore_ascii_case(prefix)
            .then(|| &path[prefix.len()..])
    });
    let repo = repo.filter(|r| {
        let mut parts = r.split('/');
        matches!((parts.next(), parts.next(), parts.next()), (Some(o), Some(n), None) if !o.is_empty() && !n.is_empty())
    });
    (repo, tag)
}

/// Modèles Ollama installés depuis un des dépôts GGUF de `model` : son dépôt canonique, ses
/// dépôts alternatifs, ou le dépôt d'une de ses quantifications (sans tenir compte de la casse).
pub fn installed_for<'a>(
    installed: &'a [OllamaModel],
    model: &'a ModelEntry,
) -> impl Iterator<Item = &'a OllamaModel> + 'a {
    let repos = || {
        std::iter::once(model.repo_id.as_str())
            .chain(model.alternatives.iter().map(|a| a.repo_id.as_str()))
            .chain(model.variants.iter().map(|v| v.repo_id.as_str()))
    };
    installed.iter().filter(move |om| {
        om.repo_id
            .as_deref()
            .is_some_and(|r| repos().any(|repo| repo.eq_ignore_ascii_case(r)))
    })
}

/// Nom Ollama d'une quantification de `model` déjà installée ; à défaut d'une installation
/// depuis le dépôt même de la variante, celle d'un autre dépôt du modèle.
pub fn installed_variant<'a>(
    installed: &'a [OllamaModel],
    model: &'a ModelEntry,
    variant_repo: &str,
    label: &str,
) -> Option<&'a OllamaModel> {
    let mut candidates = installed_for(installed, model).filter(|om| om.is_quant(label));
    let first = candidates.next()?;
    let same_repo = |om: &&OllamaModel| {
        om.repo_id
            .as_deref()
            .is_some_and(|r| r.eq_ignore_ascii_case(variant_repo))
    };
    if same_repo(&first) {
        return Some(first);
    }
    Some(candidates.find(same_repo).unwrap_or(first))
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

    use crate::model::{Pipeline, QuantVariant, RepoRef};

    #[test]
    fn champs_en_camel_case() {
        let json = serde_json::to_value(RuntimesStatus::default()).unwrap();
        assert!(json.get("lmStudio").is_some());
        assert!(json.get("llamaCpp").is_some());
        assert_eq!(json["ollama"]["installed"], false);
        assert_eq!(json["ollamaModels"], serde_json::json!([]));
        // Ancien format (sans `ollamaModels`) toujours lisible.
        let old: RuntimesStatus = serde_json::from_str(
            r#"{ "ollama": { "installed": true, "running": false, "path": null, "version": null },
                 "lmStudio": { "installed": false, "running": false, "path": null, "version": null },
                 "llamaCpp": { "installed": false, "running": false, "path": null, "version": null } }"#,
        )
        .unwrap();
        assert!(old.ollama.installed && old.ollama_models.is_empty());
    }

    #[test]
    fn noms_ollama() {
        let m = OllamaModel::new(
            "hf.co/bartowski/Qwen_Qwen3-8B-GGUF:Q4_K_M",
            5,
            Some("Q4_K_M"),
        );
        assert_eq!(m.repo_id.as_deref(), Some("bartowski/Qwen_Qwen3-8B-GGUF"));
        assert_eq!(m.quant.as_deref(), Some("Q4_K_M"));
        assert!(m.is_quant("q4_k_m") && !m.is_quant("Q4_K_S"));

        // Étiquette d'unsloth, casse libre, autre préfixe.
        let m = OllamaModel::new("HuggingFace.co/unsloth/Qwen3-8B-GGUF:ud-q4_k_xl", 5, None);
        assert_eq!(m.repo_id.as_deref(), Some("unsloth/Qwen3-8B-GGUF"));
        assert!(m.is_quant("UD-Q4_K_XL"));

        // Sans étiquette ou `latest` : la quantification annoncée par Ollama.
        let m = OllamaModel::new("hf.co/Qwen/Qwen3-8B-GGUF", 5, Some("Q4_K_M"));
        assert_eq!(m.repo_id.as_deref(), Some("Qwen/Qwen3-8B-GGUF"));
        assert_eq!(m.quant.as_deref(), Some("Q4_K_M"));
        let m = OllamaModel::new("hf.co/Qwen/Qwen3-8B-GGUF:latest", 5, Some("Q8_0"));
        assert_eq!(m.quant.as_deref(), Some("Q8_0"));

        // Bibliothèque d'Ollama : pas de dépôt Hugging Face.
        let m = OllamaModel::new("qwen3:8b", 5, Some("Q4_K_M"));
        assert_eq!((m.repo_id, m.quant.as_deref()), (None, Some("8b")));
        let m = OllamaModel::new("registry.local:5000/team/model", 5, None);
        assert_eq!((m.repo_id, m.quant), (None, None));
        assert_eq!(
            OllamaModel::new("hf.co/seulement-un-nom:Q4_K_M", 5, None).repo_id,
            None
        );
    }

    fn entry() -> ModelEntry {
        let v = |repo: &str, label: &str| QuantVariant {
            label: label.into(),
            repo_id: repo.into(),
            files: vec![format!("{label}.gguf")],
            size_bytes: 1,
            bpw: 4.0,
            exact: true,
        };
        ModelEntry {
            repo_id: "Qwen/Qwen3-8B-GGUF".into(),
            sha: String::new(),
            base_model: Some("Qwen/Qwen3-8B".into()),
            display_name: "Qwen3-8B".into(),
            params_total: 8_190_000_000,
            params_active: None,
            arch: None,
            ctx_train: 40_960,
            pipeline: Pipeline::TextGeneration,
            use_cases: vec![],
            downloads: 0,
            likes: 0,
            created_at: None,
            last_modified: None,
            gated: false,
            variants: vec![
                v("Qwen/Qwen3-8B-GGUF", "Q4_K_M"),
                v("bartowski/Qwen_Qwen3-8B-GGUF", "IQ4_XS"),
            ],
            mmproj_bytes: None,
            alternatives: vec![RepoRef {
                repo_id: "unsloth/Qwen3-8B-GGUF".into(),
                sha: String::new(),
                downloads: 0,
                gated: false,
            }],
        }
    }

    #[test]
    fn rapprochement_avec_le_catalogue() {
        let installed = [
            OllamaModel::new("hf.co/unsloth/qwen3-8b-gguf:Q4_K_M", 1, None),
            OllamaModel::new("hf.co/bartowski/Qwen_Qwen3-8B-GGUF:IQ4_XS", 1, None),
            OllamaModel::new("hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M", 1, None),
            OllamaModel::new("hf.co/Qwen/Qwen3-14B-GGUF:Q4_K_M", 1, None),
            OllamaModel::new("qwen3:8b", 1, Some("Q4_K_M")),
        ];
        let m = entry();
        assert_eq!(installed_for(&installed, &m).count(), 3);
        // Le dépôt de la variante d'abord, sinon un autre dépôt du modèle.
        let name = |repo, label| installed_variant(&installed, &m, repo, label).map(|o| &o.name);
        assert_eq!(
            name("Qwen/Qwen3-8B-GGUF", "Q4_K_M").map(String::as_str),
            Some("hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M")
        );
        assert_eq!(
            name("Qwen/Qwen3-8B-GGUF", "IQ4_XS").map(String::as_str),
            Some("hf.co/bartowski/Qwen_Qwen3-8B-GGUF:IQ4_XS")
        );
        assert_eq!(name("Qwen/Qwen3-8B-GGUF", "Q8_0"), None);
        assert_eq!(installed_for(&installed[3..], &m).count(), 0);
    }
}
