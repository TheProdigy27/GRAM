//! Catalogue de modèles : construction des [`ModelEntry`] à partir des groupes de dépôts, du
//! détail des fichiers et des métadonnées d'architecture.

use crate::api::ApiModel;
use crate::dedup::Group;
use crate::files::{self, RepoFile};
use chrono::{DateTime, Utc};
use gram_core::{ArchInfo, ModelEntry, Pipeline, QuantVariant, RepoRef, UseCase};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Version du format de `catalog.json` et de `seed_catalog.json`. Un cache d'un autre format
/// est ignoré.
pub const CATALOG_FORMAT: u32 = 2;

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
        alternatives: g.repos[1..]
            .iter()
            .map(|r| RepoRef {
                repo_id: r.model.id.clone(),
                sha: r.model.sha.clone().unwrap_or_default(),
                downloads: r.downloads(),
                gated: r.model.gated,
            })
            .collect(),
    }
}

/// Quantifications suffisantes pour se passer des dépôts alternatifs : au moins
/// [`COMPLETE_MIN_VARIANTS`] variantes, dont une sous [`COMPLETE_MAX_LOW_BPW`] bits par poids
/// (pour les machines à petite mémoire). Les dépôts officiels n'en proposent souvent que
/// quelques-unes, à partir de Q4_K_M.
pub const COMPLETE_MIN_VARIANTS: usize = 8;
pub const COMPLETE_MAX_LOW_BPW: f32 = 3.5;

pub fn variants_complete(variants: &[QuantVariant]) -> bool {
    variants.len() >= COMPLETE_MIN_VARIANTS
        && variants
            .iter()
            .any(|v| v.bpw > 0.0 && v.bpw <= COMPLETE_MAX_LOW_BPW)
}

/// Ajoute les tailles réelles des fichiers, réunies depuis plusieurs dépôts du groupe.
/// `details` est trié par priorité (dépôt canonique d'abord) : pour un même libellé, la
/// variante du dépôt prioritaire l'emporte ; le projecteur vision vient du premier dépôt qui
/// en a un.
///
/// Renvoie `false` si aucune quantification n'est exploitable, ou si le nombre de paramètres
/// du dépôt canonique est manifestement faux (modèle écarté). Une alternative incohérente est
/// simplement ignorée.
pub fn apply_details(e: &mut ModelEntry, details: &[&RepoDetail]) -> bool {
    if let Some(d) = details.iter().find(|d| d.repo_id == e.repo_id) {
        e.sha.clone_from(&d.sha);
        e.gated = d.gated;
        if let Some(p) = d.params_total {
            e.params_total = p;
        }
    }
    let mut variants: Vec<QuantVariant> = Vec::new();
    let mut mmproj = None;
    let mut vision_tag = false;
    for d in details {
        if e.ctx_train == 0
            && let Some(c) = d.ctx_train
        {
            e.ctx_train = c;
        }
        let analysis = files::analyze(&d.repo_id, &d.files, e.params_total);
        if analysis.params_suspect() {
            tracing::debug!(
                repo = d.repo_id,
                "nombre de paramètres incohérent avec les fichiers"
            );
            if d.repo_id == e.repo_id {
                e.variants.clear();
                return false;
            }
            continue;
        }
        vision_tag |= d.pipeline_tag.as_deref() == Some("image-text-to-text");
        mmproj = mmproj.or(analysis.mmproj.map(|m| m.size));
        for v in analysis.variants {
            if !variants.iter().any(|known| known.label == v.label) {
                variants.push(v);
            }
        }
    }
    variants.sort_by(|a, b| a.size_bytes.cmp(&b.size_bytes).then(a.label.cmp(&b.label)));
    e.variants = variants;
    e.mmproj_bytes = mmproj;
    if e.mmproj_bytes.is_some() || vision_tag {
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
    use crate::files::RepoFile;
    use crate::test_support::fixture;

    fn detail(name: &str) -> RepoDetail {
        let m: ApiModel =
            serde_json::from_slice(&fixture(&format!("details/{name}.json"))).unwrap();
        RepoDetail::from_api(&m).unwrap()
    }

    fn entry(repo: &str, params_total: u64) -> ModelEntry {
        ModelEntry {
            repo_id: repo.into(),
            sha: String::new(),
            base_model: None,
            display_name: "test".into(),
            params_total,
            params_active: None,
            arch: None,
            ctx_train: 0,
            pipeline: Pipeline::TextGeneration,
            use_cases: vec![UseCase::General],
            downloads: 0,
            likes: 0,
            created_at: None,
            last_modified: None,
            gated: false,
            variants: vec![],
            mmproj_bytes: None,
            alternatives: vec![],
        }
    }

    #[test]
    fn quantifications_reunies() {
        let official = detail("Qwen--Qwen3-8B-GGUF");
        let bartowski = detail("bartowski--Qwen_Qwen3-8B-GGUF");
        let mut e = entry("Qwen/Qwen3-8B-GGUF", 8_190_735_360);

        assert!(apply_details(&mut e, &[&official]));
        assert_eq!(e.variants.len(), 5);
        assert_eq!(e.sha, official.sha);
        assert_eq!(e.ctx_train, 40960);
        // Rien sous Q4_K_M : il faut un dépôt alternatif.
        assert!(!variants_complete(&e.variants));

        assert!(apply_details(&mut e, &[&official, &bartowski]));
        assert_eq!(e.variants.len(), 25);
        assert!(variants_complete(&e.variants));
        // Même libellé dans les deux dépôts : le dépôt prioritaire (officiel) l'emporte.
        let q4 = e.variants.iter().find(|v| v.label == "Q4_K_M").unwrap();
        assert_eq!(q4.repo_id, "Qwen/Qwen3-8B-GGUF");
        assert_eq!(q4.files, vec!["Qwen3-8B-Q4_K_M.gguf"]);
        // Libellés propres à chaque dépôt.
        let q5_0 = e.variants.iter().find(|v| v.label == "Q5_0").unwrap();
        assert_eq!(q5_0.repo_id, "Qwen/Qwen3-8B-GGUF");
        assert_eq!(e.variants[0].label, "IQ2_M");
        assert_eq!(e.variants[0].repo_id, "bartowski/Qwen_Qwen3-8B-GGUF");
        assert!(
            e.variants
                .windows(2)
                .all(|w| w[0].size_bytes <= w[1].size_bytes)
        );
        // Le sha reste celui du dépôt canonique.
        assert_eq!(e.sha, official.sha);

        // Ordre inverse : bartowski prioritaire pour les libellés communs.
        let mut e = entry("bartowski/Qwen_Qwen3-8B-GGUF", 8_190_735_360);
        apply_details(&mut e, &[&bartowski, &official]);
        let q4 = e.variants.iter().find(|v| v.label == "Q4_K_M").unwrap();
        assert_eq!(q4.repo_id, "bartowski/Qwen_Qwen3-8B-GGUF");
    }

    #[test]
    fn projecteur_vision() {
        let unsloth = detail("unsloth--Qwen3.5-9B-GGUF");
        let mut e = entry("Qwen/Qwen3.5-9B-GGUF", 8_953_803_264);
        assert!(apply_details(&mut e, &[&unsloth]));
        assert_eq!(e.mmproj_bytes, Some(918_166_080));
        assert_eq!(e.pipeline, Pipeline::ImageTextToText);
        assert!(e.use_cases.contains(&UseCase::Vision));
    }

    #[test]
    fn depot_incoherent() {
        let official = detail("Qwen--Qwen3-8B-GGUF");
        // Fichiers bien trop gros pour 8 G paramètres (plus de 40 bits par poids).
        let weird = RepoDetail {
            repo_id: "x/weird-GGUF".into(),
            sha: "0123456789abcdef".into(),
            gated: false,
            pipeline_tag: None,
            params_total: None,
            ctx_train: None,
            files: ["Q2_K", "Q3_K_M", "Q4_K_M"]
                .iter()
                .map(|q| RepoFile {
                    path: format!("m-{q}.gguf"),
                    size: 60_000_000_000,
                })
                .collect(),
        };
        // Alternative incohérente : ignorée.
        let mut e = entry("Qwen/Qwen3-8B-GGUF", 8_190_735_360);
        assert!(apply_details(&mut e, &[&official, &weird]));
        assert_eq!(e.variants.len(), 5);
        // Dépôt canonique incohérent : modèle écarté.
        let mut e = entry("x/weird-GGUF", 8_190_735_360);
        assert!(!apply_details(&mut e, &[&weird, &official]));
        assert!(e.variants.is_empty());
        // Aucun détail : rien d'exploitable.
        assert!(!apply_details(&mut entry("a/b", 1), &[]));
    }

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
