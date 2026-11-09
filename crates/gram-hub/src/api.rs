//! Requêtes et réponses de l'API Hugging Face (§7.1 du plan).
//!
//! Les paramètres de `/api/models` ne sont pas formellement documentés : tout ce qui dépend de
//! leur format est regroupé ici et testé sur des réponses réelles (`tests/fixtures/hf/`).

use chrono::{DateTime, Utc};
use gram_core::Pipeline;
use serde::{Deserialize, Deserializer};
use url::Url;

pub const HF_BASE: &str = "https://huggingface.co";

/// Champs demandés dans les listes (`expand[]`). `tags` donne la relation au modèle de base
/// (`base_model:quantized:…` / `base_model:finetune:…`), plus fiable que `cardData`.
const LIST_EXPAND: &[&str] = &[
    "gguf",
    "downloads",
    "likes",
    "trendingScore",
    "lastModified",
    "createdAt",
    "pipeline_tag",
    "cardData",
    "sha",
    "gated",
    "tags",
];

/// Tri des listes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sort {
    Downloads,
    /// Tendance : capte les sorties récentes, encore peu téléchargées.
    Trending,
}

impl Sort {
    fn param(self) -> &'static str {
        match self {
            Self::Downloads => "downloads",
            Self::Trending => "trendingScore",
        }
    }
}

/// Tranche de taille, en milliards de paramètres (`num_parameters=min:5B,max:10B`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SizeBucket {
    pub min_b: u32,
    pub max_b: Option<u32>,
}

/// Tranches du §7.3 : `0–2B`, `2–5B`, `5–10B`, `10–20B`, `20–40B`, `40–80B`, `80B+`.
pub const SIZE_BUCKETS: [SizeBucket; 7] = [
    SizeBucket::new(0, Some(2)),
    SizeBucket::new(2, Some(5)),
    SizeBucket::new(5, Some(10)),
    SizeBucket::new(10, Some(20)),
    SizeBucket::new(20, Some(40)),
    SizeBucket::new(40, Some(80)),
    SizeBucket::new(80, None),
];

const BILLION: u64 = 1_000_000_000;

impl SizeBucket {
    pub const fn new(min_b: u32, max_b: Option<u32>) -> Self {
        Self { min_b, max_b }
    }

    /// Valeur du paramètre `num_parameters` (formats vérifiés sur l'API réelle).
    pub fn param(&self) -> String {
        match (self.min_b, self.max_b) {
            (0, Some(max)) => format!("min:0,max:{max}B"),
            (min, Some(max)) => format!("min:{min}B,max:{max}B"),
            (min, None) => format!("min:{min}B"),
        }
    }

    /// La tranche qui contient `params` (bornes basses incluses).
    pub fn of(params: u64) -> SizeBucket {
        SIZE_BUCKETS
            .iter()
            .rev()
            .find(|b| params >= u64::from(b.min_b) * BILLION)
            .copied()
            .unwrap_or(SIZE_BUCKETS[0])
    }

    /// Tranches utiles pour une machine qui ne peut pas dépasser `max_params` paramètres.
    pub fn up_to(max_params: Option<u64>) -> Vec<SizeBucket> {
        SIZE_BUCKETS
            .iter()
            .filter(|b| max_params.is_none_or(|max| u64::from(b.min_b) * BILLION < max))
            .copied()
            .collect()
    }
}

/// `pipeline_tag` Hugging Face.
pub fn pipeline_tag(p: Pipeline) -> &'static str {
    match p {
        Pipeline::TextGeneration => "text-generation",
        Pipeline::ImageTextToText => "image-text-to-text",
    }
}

/// Une liste de modèles GGUF : un pipeline, une tranche de taille, un tri.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListQuery {
    pub pipeline: Pipeline,
    pub bucket: SizeBucket,
    pub sort: Sort,
    pub limit: u32,
}

fn base_url(base: &str) -> Url {
    Url::parse(base).unwrap_or_else(|_| Url::parse(HF_BASE).expect("URL Hugging Face valide"))
}

/// `GET /api/models?filter=gguf&…` (et non `library=gguf`, qui renvoie des modèles sans
/// rapport quand il est combiné à `expand`).
pub fn list_url(base: &str, q: &ListQuery) -> String {
    let mut url = base_url(base);
    url.set_path("/api/models");
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("filter", "gguf")
            .append_pair("pipeline_tag", pipeline_tag(q.pipeline))
            .append_pair("num_parameters", &q.bucket.param())
            .append_pair("sort", q.sort.param())
            .append_pair("limit", &q.limit.to_string());
        for e in LIST_EXPAND {
            qp.append_pair("expand[]", e);
        }
    }
    url.into()
}

fn push_repo(url: &mut Url, repo: &str) {
    if let Ok(mut seg) = url.path_segments_mut() {
        seg.extend(repo.split('/'));
    }
}

/// `GET /api/models/{repo}?blobs=true` : détail d'un dépôt avec la taille de chaque fichier.
pub fn detail_url(base: &str, repo: &str) -> String {
    let mut url = base_url(base);
    url.set_path("/api/models");
    push_repo(&mut url, repo);
    url.set_query(Some("blobs=true"));
    url.into()
}

/// `GET /{repo}/resolve/{revision}/{fichier}` : fichier brut (quota « resolvers », bien plus
/// large que celui de l'API).
pub fn resolve_url(base: &str, repo: &str, revision: &str, path: &str) -> String {
    let mut url = base_url(base);
    url.set_path("/");
    push_repo(&mut url, repo);
    if let Ok(mut seg) = url.path_segments_mut() {
        seg.push("resolve").push(revision).extend(path.split('/'));
    }
    url.into()
}

/// Un dépôt, tel que renvoyé par la liste ou par le détail (`siblings` seulement dans le détail).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiModel {
    pub id: String,
    #[serde(default)]
    pub sha: Option<String>,
    #[serde(default)]
    pub downloads: Option<u64>,
    #[serde(default)]
    pub likes: Option<u64>,
    /// Peut être décimal (ex. 0.4).
    #[serde(default)]
    pub trending_score: Option<f64>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_modified: Option<DateTime<Utc>>,
    #[serde(default, rename = "pipeline_tag")]
    pub pipeline_tag: Option<String>,
    #[serde(default)]
    pub gguf: Option<ApiGguf>,
    #[serde(default)]
    pub card_data: Option<serde_json::Value>,
    /// `false`, `"auto"` ou `"manual"`.
    #[serde(default, deserialize_with = "de_gated")]
    pub gated: bool,
    #[serde(default, deserialize_with = "de_null_vec")]
    pub tags: Vec<String>,
    #[serde(default, deserialize_with = "de_null_vec")]
    pub siblings: Vec<ApiSibling>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ApiGguf {
    /// Nombre total de paramètres. Peut valoir 0 (dépôt mal décrit).
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub architecture: Option<String>,
    /// Contexte d'entraînement.
    #[serde(default)]
    pub context_length: Option<u64>,
    // `totalFileSize` est ignoré exprès : c'est la taille d'un seul fichier de référence.
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiSibling {
    pub rfilename: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub lfs: Option<ApiLfs>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiLfs {
    pub size: u64,
}

impl ApiSibling {
    pub fn size(&self) -> Option<u64> {
        self.size.or(self.lfs.as_ref().map(|l| l.size))
    }
}

/// Relation déclarée avec le modèle de base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseRelation {
    /// Tag `base_model:quantized:X` : ce dépôt est une quantification de X.
    Quantized(String),
    /// Tag `base_model:finetune:X` (ou `merge`, `adapter`) : un modèle dérivé, distinct de X.
    Derived,
    /// Seulement `cardData.base_model`, sans tag de relation.
    Declared(String),
    None,
}

impl ApiModel {
    /// Paramètres totaux (`gguf.total`), `None` si absent ou nul.
    pub fn params_total(&self) -> Option<u64> {
        self.gguf.as_ref()?.total.filter(|&t| t > 0)
    }

    pub fn ctx_train(&self) -> Option<u32> {
        let c = self.gguf.as_ref()?.context_length?;
        u32::try_from(c).ok().filter(|&c| c > 0)
    }

    pub fn architecture(&self) -> Option<&str> {
        self.gguf.as_ref()?.architecture.as_deref()
    }

    /// `cardData.base_model`, qui est une chaîne ou une liste.
    pub fn card_base_models(&self) -> Vec<String> {
        match self.card_data.as_ref().and_then(|c| c.get("base_model")) {
            Some(serde_json::Value::String(s)) => vec![s.clone()],
            Some(serde_json::Value::Array(a)) => a
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect(),
            _ => vec![],
        }
    }

    /// Relation au modèle de base, d'après les tags puis `cardData`. Les tags suivent les
    /// renommages de dépôts, contrairement à `cardData`.
    pub fn base_relation(&self) -> BaseRelation {
        let mut derived = false;
        for tag in &self.tags {
            let Some(rest) = tag.strip_prefix("base_model:") else {
                continue;
            };
            if let Some(repo) = rest.strip_prefix("quantized:") {
                return BaseRelation::Quantized(repo.to_owned());
            }
            if ["finetune:", "merge:", "adapter:"]
                .iter()
                .any(|p| rest.starts_with(p))
            {
                derived = true;
            }
        }
        if derived {
            return BaseRelation::Derived;
        }
        match self.card_base_models().into_iter().next() {
            Some(base) => BaseRelation::Declared(base),
            None => BaseRelation::None,
        }
    }
}

fn de_gated<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::Bool(b) => b,
        serde_json::Value::String(s) => !s.is_empty() && s != "false",
        _ => false,
    })
}

fn de_null_vec<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(d)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture;

    #[test]
    fn tranches() {
        assert_eq!(SIZE_BUCKETS[0].param(), "min:0,max:2B");
        assert_eq!(SIZE_BUCKETS[2].param(), "min:5B,max:10B");
        assert_eq!(SIZE_BUCKETS[6].param(), "min:80B");
        assert_eq!(SizeBucket::of(8_190_735_360), SIZE_BUCKETS[2]);
        assert_eq!(SizeBucket::of(5_000_000_000), SIZE_BUCKETS[2]);
        assert_eq!(SizeBucket::of(600_000_000), SIZE_BUCKETS[0]);
        assert_eq!(SizeBucket::of(400_000_000_000), SIZE_BUCKETS[6]);
        // 15 milliards : inutile de lister les tranches qui commencent à 20B ou plus.
        assert_eq!(SizeBucket::up_to(Some(15_000_000_000)).len(), 4);
        assert_eq!(SizeBucket::up_to(Some(10_000_000_000)).len(), 3);
        assert_eq!(SizeBucket::up_to(None).len(), 7);
    }

    #[test]
    fn urls() {
        let q = ListQuery {
            pipeline: Pipeline::ImageTextToText,
            bucket: SIZE_BUCKETS[2],
            sort: Sort::Trending,
            limit: 50,
        };
        let url = list_url(HF_BASE, &q);
        assert!(url.starts_with("https://huggingface.co/api/models?filter=gguf&pipeline_tag=image-text-to-text&num_parameters=min%3A5B%2Cmax%3A10B&sort=trendingScore&limit=50&expand%5B%5D=gguf&"), "{url}");
        assert_eq!(
            detail_url(HF_BASE, "bartowski/Qwen_Qwen3-8B-GGUF"),
            "https://huggingface.co/api/models/bartowski/Qwen_Qwen3-8B-GGUF?blobs=true"
        );
        assert_eq!(
            resolve_url(
                HF_BASE,
                "unsloth/Ornith-1.5-122B-GGUF",
                "745bd742",
                "UD-Q4_K_XL/Ornith-1.5-122B-UD-Q4_K_XL-00001-of-00003.gguf"
            ),
            "https://huggingface.co/unsloth/Ornith-1.5-122B-GGUF/resolve/745bd742/UD-Q4_K_XL/Ornith-1.5-122B-UD-Q4_K_XL-00001-of-00003.gguf"
        );
        assert_eq!(
            resolve_url(HF_BASE, "a/b", "main", "dossier avec espace/x#1.gguf"),
            "https://huggingface.co/a/b/resolve/main/dossier%20avec%20espace/x%231.gguf"
        );
    }

    #[test]
    fn liste_reelle() {
        let list: Vec<ApiModel> =
            serde_json::from_slice(&fixture("list/text-generation_5-10B_downloads.json")).unwrap();
        assert_eq!(list.len(), 100);
        let qwen = list.iter().find(|m| m.id == "Qwen/Qwen3-8B-GGUF").unwrap();
        assert_eq!(qwen.params_total(), Some(8_190_735_360));
        assert_eq!(qwen.ctx_train(), Some(40960));
        assert_eq!(qwen.architecture(), Some("qwen3"));
        assert_eq!(
            qwen.sha.as_deref(),
            Some("7c41481f57cb95916b40956ab2f0b139b296d974")
        );
        assert!(!qwen.gated);
        assert!(qwen.downloads.unwrap() > 500_000);
        assert!(qwen.created_at.is_some());
        assert_eq!(
            qwen.base_relation(),
            BaseRelation::Quantized("Qwen/Qwen3-8B".into())
        );

        // Fine-tune qui déclare Qwen3-8B comme modèle de base : c'est un autre modèle.
        let parable = list
            .iter()
            .find(|m| m.id == "AnkitAI/Parable-Qwen3-8B-Claude-Fable-5-GGUF")
            .unwrap();
        assert_eq!(parable.card_base_models(), vec!["Qwen/Qwen3-8B"]);
        assert_eq!(parable.base_relation(), BaseRelation::Derived);

        // `cardData.base_model` en liste, et tag qui suit le renommage du dépôt de base.
        let ornith = list
            .iter()
            .find(|m| m.id == "unsloth/Ornith-1.0-9B-GGUF")
            .unwrap();
        assert_eq!(
            ornith.card_base_models(),
            vec!["deepreinforce-ai/Ornith-1.0-9B"]
        );
        assert_eq!(
            ornith.base_relation(),
            BaseRelation::Quantized("ornith-ai/Ornith-1.0-9B".into())
        );
    }

    #[test]
    fn champs_atypiques() {
        let m: ApiModel = serde_json::from_str(
            r#"{ "id": "a/b", "gated": "manual", "downloads": null, "tags": null,
                 "trendingScore": 0.4, "gguf": { "total": 0 },
                 "cardData": { "base_model": { "bizarre": true } } }"#,
        )
        .unwrap();
        assert!(m.gated);
        assert_eq!(m.downloads, None);
        assert!(m.tags.is_empty());
        assert_eq!(m.params_total(), None);
        assert!(m.card_base_models().is_empty());
        assert_eq!(m.base_relation(), BaseRelation::None);
    }
}
