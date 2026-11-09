//! Dédoublonnage par modèle de base (§7.5 du plan).
//!
//! Un même modèle est publié en GGUF par plusieurs dépôts (officiel, unsloth, bartowski…).
//! Clé de regroupement : le nom normalisé du modèle de base déclaré par le tag
//! `base_model:quantized:X` (ou, à défaut, `cardData.base_model`), à condition que le nom du
//! dépôt lui corresponde ; sinon le nom normalisé du dépôt lui-même. Un fine-tune
//! (`base_model:finetune:X`) ou un dérivé (« …-abliterated ») forme donc son propre groupe.

use crate::api::{ApiModel, BaseRelation};
use crate::files::strip_quant_suffix;
use serde::Deserialize;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const EMBEDDED_UPLOADERS: &str = include_str!("../../../data/uploaders.json");

/// Ordre de priorité des uploaders (`data/uploaders.json`).
#[derive(Debug, Clone)]
pub struct Uploaders {
    order: Vec<String>,
}

#[derive(Deserialize)]
struct UploadersFile {
    uploaders: Vec<String>,
}

impl Uploaders {
    pub fn embedded() -> &'static Uploaders {
        static U: OnceLock<Uploaders> = OnceLock::new();
        U.get_or_init(|| {
            Uploaders::from_json(EMBEDDED_UPLOADERS).expect("data/uploaders.json invalide")
        })
    }

    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        let f: UploadersFile = serde_json::from_str(json)?;
        Ok(Self {
            order: f
                .uploaders
                .into_iter()
                .map(|u| u.to_ascii_lowercase())
                .collect(),
        })
    }

    fn rank(&self, owner: &str) -> usize {
        let owner = owner.to_ascii_lowercase();
        self.order
            .iter()
            .position(|u| *u == owner)
            .unwrap_or(self.order.len())
    }
}

/// Un dépôt trouvé dans les listes.
#[derive(Debug, Clone)]
pub struct ListedRepo {
    pub model: ApiModel,
    /// Présent dans une liste triée par tendance, avec un score non nul.
    pub trending: bool,
    /// Listé avec le pipeline `image-text-to-text`.
    pub vision: bool,
}

impl ListedRepo {
    pub fn downloads(&self) -> u64 {
        self.model.downloads.unwrap_or(0)
    }

    pub fn trending_score(&self) -> f64 {
        self.model.trending_score.unwrap_or(0.0)
    }
}

/// Les dépôts d'un même modèle, dépôt canonique en premier.
#[derive(Debug, Clone)]
pub struct Group {
    /// Nom normalisé (minuscules), ex. « qwen3-8b ».
    pub key: String,
    /// Dépôt du modèle de base, ex. « Qwen/Qwen3-8B ».
    pub base_model: Option<String>,
    /// Nom affiché, ex. « Qwen3-8B ».
    pub display_name: String,
    pub repos: Vec<ListedRepo>,
}

impl Group {
    pub fn canonical(&self) -> &ListedRepo {
        &self.repos[0]
    }

    pub fn trending_score(&self) -> f64 {
        self.repos
            .iter()
            .map(ListedRepo::trending_score)
            .fold(0.0, f64::max)
    }

    pub fn downloads(&self) -> u64 {
        self.repos.iter().map(ListedRepo::downloads).sum()
    }

    pub fn vision(&self) -> bool {
        self.repos.iter().any(|r| r.vision)
    }
}

fn owner(repo: &str) -> &str {
    repo.split_once('/').map_or("", |(o, _)| o)
}

fn name(repo: &str) -> &str {
    repo.split_once('/').map_or(repo, |(_, n)| n)
}

const SUFFIXES: &[&str] = &[
    "-gguf", "_gguf", ".gguf", " gguf", "-i1", "-imat", "-imatrix",
];

fn strip_suffix_ci<'a>(s: &'a str, suffix: &str) -> Option<&'a str> {
    let cut = s.len().checked_sub(suffix.len())?;
    let (head, tail) = (s.get(..cut)?, s.get(cut..)?);
    (cut > 0 && tail.eq_ignore_ascii_case(suffix)).then_some(head)
}

/// Nom sans suffixe `-GGUF` / `-i1` / quantification, ni préfixe d'organisation
/// (« Qwen_Qwen3-8B-GGUF » → « Qwen3-8B » si « qwen » est une organisation connue).
fn clean_name<'a>(name: &'a str, orgs: &HashSet<String>) -> &'a str {
    let mut n = name.trim();
    loop {
        let len = n.len();
        for suffix in SUFFIXES {
            if let Some(head) = strip_suffix_ci(n, suffix) {
                n = head;
            }
        }
        n = strip_quant_suffix(n);
        if n.len() == len {
            break;
        }
    }
    if let Some((prefix, rest)) = n.split_once('_')
        && !rest.is_empty()
        && orgs.contains(&prefix.to_ascii_lowercase())
    {
        n = rest;
    }
    n
}

/// Le nom du dépôt correspond-il à celui du modèle de base ? Égalité, ou l'un finit par
/// l'autre après un séparateur (« meta-llama-3.1-8b-instruct » / « llama-3.1-8b-instruct »).
fn consistent(repo: &str, base: &str) -> bool {
    let ends = |long: &str, short: &str| {
        long.len() > short.len()
            && long.ends_with(short)
            && matches!(
                long.as_bytes()[long.len() - short.len() - 1],
                b'-' | b'_' | b'.'
            )
    };
    repo == base || ends(repo, base) || ends(base, repo)
}

/// (clé de regroupement, modèle de base, nom affiché) d'un dépôt.
fn identity(m: &ApiModel, orgs: &HashSet<String>) -> (String, Option<String>, String) {
    let repo_name = clean_name(name(&m.id), orgs);
    let declared = match m.base_relation() {
        BaseRelation::Quantized(b) | BaseRelation::Declared(b) => Some(b),
        BaseRelation::Derived | BaseRelation::None => None,
    };
    if let Some(base) = declared {
        let base_name = clean_name(name(&base), orgs);
        if !base_name.is_empty()
            && consistent(
                &repo_name.to_ascii_lowercase(),
                &base_name.to_ascii_lowercase(),
            )
        {
            return (
                base_name.to_ascii_lowercase(),
                Some(base.clone()),
                base_name.to_owned(),
            );
        }
    }
    (repo_name.to_ascii_lowercase(), None, repo_name.to_owned())
}

/// Regroupe les dépôts par modèle et choisit le dépôt canonique de chaque groupe.
pub fn group_repos(repos: Vec<ListedRepo>, uploaders: &Uploaders) -> Vec<Group> {
    // Organisations connues : propriétaires des dépôts et des modèles de base.
    let mut orgs: HashSet<String> = HashSet::new();
    for r in &repos {
        orgs.insert(owner(&r.model.id).to_ascii_lowercase());
        let bases = r.model.card_base_models().into_iter().chain(
            r.model
                .tags
                .iter()
                .filter_map(|t| t.strip_prefix("base_model:"))
                .map(|t| t.rsplit(':').next().unwrap_or(t).to_owned()),
        );
        for b in bases {
            orgs.insert(owner(&b).to_ascii_lowercase());
        }
    }
    orgs.remove("");

    let mut by_key: HashMap<String, Vec<(ListedRepo, Option<String>, String)>> = HashMap::new();
    for r in repos {
        let (key, base, display) = identity(&r.model, &orgs);
        by_key.entry(key).or_default().push((r, base, display));
    }

    let mut groups: Vec<Group> = by_key
        .into_iter()
        .map(|(key, mut members)| {
            members.sort_by_key(|(r, _, _)| Reverse(r.downloads()));
            // Modèle de base du groupe : le plus souvent déclaré (le plus téléchargé à égalité).
            let mut counts: Vec<(&str, usize)> = Vec::new();
            for (_, base, _) in &members {
                if let Some(b) = base.as_deref() {
                    match counts.iter_mut().find(|(c, _)| c.eq_ignore_ascii_case(b)) {
                        Some((_, n)) => *n += 1,
                        None => counts.push((b, 1)),
                    }
                }
            }
            let max = counts.iter().map(|(_, n)| *n).max();
            let base_model = max
                .and_then(|m| counts.iter().find(|(_, n)| *n == m))
                .map(|(b, _)| b.to_string());

            // Dépôt canonique : organisation officielle > uploaders.json > le plus téléchargé.
            let official = base_model.as_deref().map(owner).unwrap_or("");
            let is_official = |r: &ListedRepo| {
                !official.is_empty() && owner(&r.model.id).eq_ignore_ascii_case(official)
            };
            let rank = |r: &ListedRepo| uploaders.rank(owner(&r.model.id));
            members.sort_by(|(a, _, _), (b, _, _)| {
                is_official(b)
                    .cmp(&is_official(a))
                    .then(rank(a).cmp(&rank(b)))
                    .then(b.downloads().cmp(&a.downloads()))
                    .then(a.model.id.cmp(&b.model.id))
            });
            let display_name = match &base_model {
                Some(b) => clean_name(name(b), &orgs).to_owned(),
                None => members[0].2.clone(),
            };
            Group {
                key,
                base_model,
                display_name,
                repos: members.into_iter().map(|(r, _, _)| r).collect(),
            }
        })
        .collect();
    groups.sort_by(|a, b| a.key.cmp(&b.key));
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture;

    fn listed(file: &str) -> Vec<ListedRepo> {
        let list: Vec<ApiModel> =
            serde_json::from_slice(&fixture(&format!("list/{file}.json"))).unwrap();
        list.into_iter()
            .map(|model| ListedRepo {
                model,
                trending: false,
                vision: file.starts_with("image"),
            })
            .collect()
    }

    fn all_groups() -> Vec<Group> {
        let mut repos = listed("text-generation_5-10B_downloads");
        repos.extend(listed("image-text-to-text_5-10B_downloads"));
        // Un dépôt peut apparaître dans les deux listes : on garde la première occurrence.
        let mut seen = HashSet::new();
        repos.retain(|r| seen.insert(r.model.id.clone()));
        group_repos(repos, Uploaders::embedded())
    }

    fn find<'a>(groups: &'a [Group], key: &str) -> &'a Group {
        groups
            .iter()
            .find(|g| g.key == key)
            .unwrap_or_else(|| panic!("groupe {key} absent"))
    }

    fn ids(g: &Group) -> Vec<&str> {
        g.repos.iter().map(|r| r.model.id.as_str()).collect()
    }

    #[test]
    fn noms_normalises() {
        let orgs: HashSet<String> = ["qwen", "deepreinforce-ai", "mlabonne"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(clean_name("Qwen_Qwen3-8B-GGUF", &orgs), "Qwen3-8B");
        assert_eq!(
            clean_name("deepreinforce-ai_Ornith-1.0-9B-GGUF", &orgs),
            "Ornith-1.0-9B"
        );
        assert_eq!(
            clean_name("Huihui-Qwen3-Coder-Next-abliterated-i1-GGUF", &orgs),
            "Huihui-Qwen3-Coder-Next-abliterated"
        );
        assert_eq!(
            clean_name("gemma-4-e4b-pruned-ru-en-Q4_K_M", &orgs),
            "gemma-4-e4b-pruned-ru-en"
        );
        // « Meta » n'est pas une organisation connue ici : préfixe conservé.
        assert_eq!(clean_name("Meta_Llama-GGUF", &orgs), "Meta_Llama");
        assert!(consistent(
            "meta-llama-3.1-8b-instruct",
            "llama-3.1-8b-instruct"
        ));
        assert!(!consistent(
            "qwen2.5-7b-instruct-abliterated-v2",
            "qwen2.5-7b-instruct"
        ));
        assert!(!consistent("qwen3-80b", "qwen3-8b"));
    }

    #[test]
    fn officiel_puis_uploaders() {
        let groups = all_groups();
        let qwen3 = find(&groups, "qwen3-8b");
        assert_eq!(qwen3.base_model.as_deref(), Some("Qwen/Qwen3-8B"));
        assert_eq!(qwen3.display_name, "Qwen3-8B");
        // Dépôt officiel d'abord, puis l'ordre de uploaders.json.
        let i = ids(qwen3);
        assert_eq!(i[0], "Qwen/Qwen3-8B-GGUF");
        assert_eq!(i[1], "unsloth/Qwen3-8B-GGUF");
        assert_eq!(i[2], "bartowski/Qwen_Qwen3-8B-GGUF");
        assert_eq!(i[3], "lmstudio-community/Qwen3-8B-GGUF");
        assert!(i.contains(&"MaziyarPanahi/Qwen3-8B-GGUF"));
        // Le fine-tune qui déclare Qwen3-8B comme base reste à part.
        assert!(!i.iter().any(|r| r.contains("Parable")));
        find(&groups, "parable-qwen3-8b-claude-fable-5");
    }

    #[test]
    fn tag_qui_suit_le_renommage() {
        let groups = all_groups();
        let ornith = find(&groups, "ornith-1.0-9b");
        // Le tag pointe vers ornith-ai (nom actuel) : son dépôt est l'officiel.
        assert_eq!(
            ornith.base_model.as_deref(),
            Some("ornith-ai/Ornith-1.0-9B")
        );
        assert_eq!(ornith.canonical().model.id, "ornith-ai/Ornith-1.0-9B-GGUF");
        let i = ids(ornith);
        assert!(i.contains(&"unsloth/Ornith-1.0-9B-GGUF"));
        // Préfixe d'organisation de bartowski (« deepreinforce-ai_ »).
        assert!(i.contains(&"bartowski/deepreinforce-ai_Ornith-1.0-9B-GGUF"));
    }

    #[test]
    fn uploader_sans_officiel_et_vision() {
        let groups = all_groups();
        let q35 = find(&groups, "qwen3.5-9b");
        assert_eq!(q35.canonical().model.id, "unsloth/Qwen3.5-9B-GGUF");
        assert!(q35.vision());
        // Somme des téléchargements de tous les dépôts du groupe.
        assert!(q35.downloads() > q35.canonical().downloads());
    }

    #[test]
    fn derives_separes() {
        let groups = all_groups();
        let base = find(&groups, "qwen2.5-7b-instruct");
        assert_eq!(base.canonical().model.id, "Qwen/Qwen2.5-7B-Instruct-GGUF");
        // Quantification « abliterated » qui déclare la base officielle : groupe à part.
        assert!(!ids(base).iter().any(|r| r.contains("abliterated")));
        // Même modèle publié par plusieurs uploaders : regroupé.
        let r1 = find(&groups, "deepseek-r1-0528-qwen3-8b");
        assert!(r1.repos.len() >= 3);
        assert_eq!(
            r1.canonical().model.id,
            "unsloth/DeepSeek-R1-0528-Qwen3-8B-GGUF"
        );
        // Chaque dépôt est dans exactement un groupe.
        let total: usize = groups.iter().map(|g| g.repos.len()).sum();
        let mut all: Vec<&str> = groups.iter().flat_map(ids).collect();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), total);
    }
}
