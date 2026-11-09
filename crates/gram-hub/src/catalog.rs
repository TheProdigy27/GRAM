//! Catalogue de modèles : construction des [`ModelEntry`] à partir des groupes de dépôts, du
//! détail des fichiers et des métadonnées d'architecture.

use crate::api::ApiModel;
use crate::dedup::Group;
use crate::files::{self, RepoFile};
use chrono::{DateTime, Utc};
use gram_core::{ArchInfo, ModelEntry, Pipeline, UseCase};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Version du format de `catalog.json` et de `seed_catalog.json`. Un cache d'un autre format
/// est ignoré.
pub const CATALOG_FORMAT: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub format: u32,
    pub fetched_at: DateTime<Utc>,
    /// Borne haute en paramètres utilisée pour les listes ; `None` : toutes les tranches.
    pub max_params: Option<u64>,
    /// Ordre du pré-classement (le classement final est calculé par `gram-core`).
    pub models: Vec<ModelEntry>,
}

impl Catalog {
    /// Le catalogue contient-il les modèles jusqu'à `max_params` paramètres ?
    pub fn covers(&self, max_params: Option<u64>) -> bool {
        match (self.max_params, max_params) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(have), Some(want)) => have >= want,
        }
    }

    /// Nombre de dépôts GGUF (canoniques et alternatives).
    pub fn repo_count(&self) -> usize {
        self.models.iter().map(|m| 1 + m.alternatives.len()).sum()
    }
}

/// Détail d'un dépôt, mis en cache par `repo@sha` (immuable pour un commit donné).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoDetail {
    pub repo_id: String,
    pub sha: String,
    pub gated: bool,
    pub pipeline_tag: Option<String>,
    pub params_total: Option<u64>,
    pub ctx_train: Option<u32>,
    pub files: Vec<RepoFile>,
}

impl RepoDetail {
    pub fn from_api(m: &ApiModel) -> Option<Self> {
        Some(Self {
            repo_id: m.id.clone(),
            sha: m.sha.clone()?,
            gated: m.gated,
            pipeline_tag: m.pipeline_tag.clone(),
            params_total: m.params_total(),
            ctx_train: m.ctx_train(),
            files: m
                .siblings
                .iter()
                .filter_map(|s| {
                    Some(RepoFile {
                        path: s.rfilename.clone(),
                        size: s.size()?,
                    })
                })
                .collect(),
        })
    }
}

/// Architecture d'un dépôt, mise en cache par `repo@sha`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchRecord {
    pub arch: ArchInfo,
    /// Paramètres actifs calculés depuis l'en-tête (MoE).
    pub params_active: Option<u64>,
    pub ctx_train: Option<u32>,
}

fn active_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(?:^|[-_.])A(\d{1,3}(?:\.\d+)?)B(?:$|[-_.])").expect("regex A3B valide")
    })
}

/// Paramètres actifs d'après le nom : « Qwen3-30B-A3B » → 3 milliards (§7.4, 3ᵉ source).
pub fn active_params_from_name(name: &str) -> Option<u64> {
    let c = active_re().captures(name)?;
    let b: f64 = c[1].parse().ok()?;
    (b > 0.0).then_some((b * 1e9) as u64)
}

/// Cas d'usage déduits du nom, des tags et du pipeline (§9). `General` est toujours présent.
pub fn infer_use_cases(names: &[&str], tags: &[String], vision: bool) -> Vec<UseCase> {
    let tokens: Vec<String> = names
        .iter()
        .flat_map(|n| n.split(|c: char| !c.is_ascii_alphanumeric()))
        .filter(|t| !t.is_empty())
        .map(str::to_ascii_lowercase)
        .chain(tags.iter().map(|t| t.to_ascii_lowercase()))
        .collect();
    let has = |pred: &dyn Fn(&str) -> bool| tokens.iter().any(|t| pred(t));
    let mut out = vec![UseCase::General];
    // « coder », « starcoder », « codestral », « codegemma »… mais pas « decoder » / « encoder ».
    if has(&|t| {
        t.starts_with("code")
            || (t.ends_with("coder") && !t.ends_with("ecoder") && !t.ends_with("ncoder"))
    }) {
        out.push(UseCase::Code);
    }
    if has(&|t| {
        matches!(
            t,
            "thinking" | "reasoning" | "reasoner" | "r1" | "qwq" | "magistral"
        )
    }) {
        out.push(UseCase::Reasoning);
    }
    if vision || has(&|t| t == "image-text-to-text") {
        out.push(UseCase::Vision);
    }
    out
}

/// Entrée de catalogue construite à partir des listes seules (sans tailles réelles).
pub fn entry_from_group(g: &Group) -> ModelEntry {
    let c = &g.canonical().model;
    let names: Vec<&str> = [Some(g.display_name.as_str()), g.base_model.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    let vision = g.vision();
    ModelEntry {
        repo_id: c.id.clone(),
        sha: c.sha.clone().unwrap_or_default(),
        base_model: g.base_model.clone(),
        display_name: g.display_name.clone(),
        params_total: c.params_total().unwrap_or(0),
        params_active: names.iter().find_map(|n| active_params_from_name(n)),
        arch: None,
        ctx_train: g
            .repos
            .iter()
            .find_map(|r| r.model.ctx_train())
            .unwrap_or(0),
        pipeline: if vision {
            Pipeline::ImageTextToText
        } else {
            Pipeline::TextGeneration
        },
        use_cases: infer_use_cases(&names, &c.tags, vision),
        downloads: g.downloads(),
        likes: g.repos.iter().map(|r| r.model.likes.unwrap_or(0)).sum(),
        created_at: g.repos.iter().filter_map(|r| r.model.created_at).min(),
        last_modified: g.repos.iter().filter_map(|r| r.model.last_modified).max(),
        gated: c.gated,
        variants: Vec::new(),
        mmproj_bytes: None,
        alternatives: g.repos[1..].iter().map(|r| r.model.id.clone()).collect(),
    }
}

/// Ajoute les tailles réelles des fichiers. Renvoie `false` si le dépôt ne contient aucune
/// quantification exploitable, ou si son nombre de paramètres est manifestement faux.
pub fn apply_detail(e: &mut ModelEntry, d: &RepoDetail) -> bool {
    e.sha.clone_from(&d.sha);
    e.gated = d.gated;
    if let Some(p) = d.params_total {
        e.params_total = p;
    }
    if e.ctx_train == 0
        && let Some(c) = d.ctx_train
    {
        e.ctx_train = c;
    }
    let analysis = files::analyze(&d.files, e.params_total);
    if analysis.params_suspect() {
        tracing::debug!(
            repo = e.repo_id,
            "nombre de paramètres incohérent avec les fichiers : modèle écarté"
        );
        e.variants.clear();
        return false;
    }
    e.variants = analysis.variants;
    e.mmproj_bytes = analysis.mmproj.map(|m| m.size);
    let vision =
        e.mmproj_bytes.is_some() || d.pipeline_tag.as_deref() == Some("image-text-to-text");
    if vision {
        e.pipeline = Pipeline::ImageTextToText;
        if !e.use_cases.contains(&UseCase::Vision) {
            e.use_cases.push(UseCase::Vision);
        }
    }
    !e.variants.is_empty()
}

pub fn apply_arch(e: &mut ModelEntry, a: &ArchRecord) {
    e.arch = Some(a.arch.clone());
    if a.params_active.is_some() {
        e.params_active = a.params_active;
    }
    if e.ctx_train == 0
        && let Some(c) = a.ctx_train
    {
        e.ctx_train = c;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actifs_par_le_nom() {
        assert_eq!(
            active_params_from_name("Qwen3.6-35B-A3B"),
            Some(3_000_000_000)
        );
        assert_eq!(
            active_params_from_name("gemma-4-26B-A4B-it"),
            Some(4_000_000_000)
        );
        assert_eq!(active_params_from_name("Ling-A1.5B-x"), Some(1_500_000_000));
        assert_eq!(active_params_from_name("Qwen3-8B"), None);
        assert_eq!(active_params_from_name("LLaMA-3B"), None);
    }

    #[test]
    fn cas_d_usage() {
        use UseCase::*;
        assert_eq!(
            infer_use_cases(&["Qwen2.5-Coder-7B-Instruct"], &[], false),
            vec![General, Code]
        );
        assert_eq!(
            infer_use_cases(&["DeepSeek-R1-0528-Qwen3-8B"], &[], false),
            vec![General, Reasoning]
        );
        assert_eq!(
            infer_use_cases(&["Qwen3-4B-Thinking-2507"], &[], true),
            vec![General, Reasoning, Vision]
        );
        assert_eq!(
            infer_use_cases(&["Qwen3-8B"], &["image-text-to-text".into()], false),
            vec![General, Vision]
        );
        assert_eq!(
            infer_use_cases(&["StarCoder2-15B", "codegemma-7b"], &[], false),
            vec![General, Code]
        );
        // Pas de faux positif sur « R10 » ou « Decoder ».
        assert_eq!(
            infer_use_cases(&["Model-R10", "Encoder-Decoder"], &[], false),
            vec![General]
        );
    }

    #[test]
    fn couverture() {
        let c = |max| Catalog {
            format: CATALOG_FORMAT,
            fetched_at: Utc::now(),
            max_params: max,
            models: vec![],
        };
        assert!(c(None).covers(Some(1)));
        assert!(c(None).covers(None));
        assert!(!c(Some(10)).covers(None));
        assert!(c(Some(10)).covers(Some(10)));
        assert!(!c(Some(10)).covers(Some(11)));
    }
}
