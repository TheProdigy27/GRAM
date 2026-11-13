//! Liste des modèles de GRAM Harness.
//!
//! Ollama fait foi : la liste vient de `/api/tags`, avec les capacités et le contexte de
//! `/api/show`, lus une fois par modèle tant que son nom, son empreinte et sa taille ne changent
//! pas. Le manifeste de GRAM (`models.json`) l'enrichit : nom lisible, contexte, vitesses
//! estimée et mesurée. Sont masqués les modèles qui ne génèrent pas de texte (plongements) et
//! les bases des modèles dérivés de GRAM, qui font double emploi (mêmes poids, contexte non
//! réglé).

use gram_core::{OllamaModel, UseCase};
use gram_link::{SharedManifest, SharedModel};
use gram_ollama::api::same_model;
use gram_ollama::{Capability, ModelShow, OllamaClient, Result, TaggedModel};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use tokio::task::JoinSet;
use ts_rs::TS;

/// Un modèle d'Ollama avec lequel discuter.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HarnessModel {
    /// Nom dans Ollama, clé du modèle : `gram/qwen3-8b:q4_k_m`, `qwen3:8b`.
    pub name: String,
    /// Nom lisible du manifeste (« Qwen3 8B »), sinon le nom technique.
    pub display_name: String,
    /// Ajouté depuis GRAM (entrée du manifeste) : listé d'abord, sous son nom lisible.
    pub from_gram: bool,
    /// Quantification : celle du manifeste, sinon l'étiquette du nom (Hugging Face) ou celle
    /// qu'annonce Ollama (sa bibliothèque).
    pub quant: Option<String>,
    /// Taille sur le disque, en octets.
    pub size: u64,
    /// Contexte réglé dans le modèle (`num_ctx`), sinon celui du manifeste, en tokens ; absent :
    /// Ollama choisit.
    pub ctx: Option<u32>,
    /// Accepte des images.
    pub vision: bool,
    /// Sait réfléchir avant de répondre.
    pub thinking: bool,
    /// Sait appeler des outils.
    pub tools: bool,
    /// Vitesse estimée par GRAM sur cette machine, en tok/s.
    pub estimated_tok_s: Option<f32>,
    /// Dernière mesure réelle dans GRAM, en tok/s.
    pub measured_tok_s: Option<f32>,
    /// Usage choisi dans GRAM lors de l'ajout.
    pub use_case: Option<UseCase>,
}

/// État d'Ollama vu par GRAM Harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum OllamaState {
    /// L'API répond.
    Running,
    /// Installé, mais l'API ne répond pas.
    Stopped,
    /// Introuvable sur cette machine.
    Absent,
}

/// Réponse de `list_models`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ModelList {
    pub ollama: OllamaState,
    /// Modèles de GRAM d'abord (par nom lisible), puis les autres (par nom technique) ; vide si
    /// Ollama ne répond pas.
    pub models: Vec<HarnessModel>,
}

/// Modèles avec lesquels discuter : `/api/tags`, capacités de `/api/show` (en cache), manifeste.
pub async fn list_models(
    client: &OllamaClient,
    manifest: &SharedManifest,
    shows: &ShowCache,
) -> Result<Vec<HarnessModel>> {
    let tagged = client.tagged().await?;
    let known = shows.fill(client, &tagged).await;
    Ok(merge(&tagged, &known, manifest))
}

/// Croise les modèles installés, leurs capacités (absentes si `/api/show` a échoué) et le
/// manifeste. Pur.
pub fn merge(
    tags: &[TaggedModel],
    shows: &HashMap<String, ModelShow>,
    manifest: &SharedManifest,
) -> Vec<HarnessModel> {
    let installed = |name: &str| tags.iter().any(|t| same_model(&t.model.name, name));
    // Bases des modèles dérivés présents dans Ollama ; celle d'une entrée dont le modèle dérivé a
    // disparu reste visible.
    let hidden: Vec<&str> = manifest
        .models
        .iter()
        .filter(|m| installed(&m.ollama_name))
        .map(|m| m.base_name.as_str())
        .collect();

    let mut models: Vec<HarnessModel> = tags
        .iter()
        .filter(|t| !hidden.iter().any(|b| same_model(b, &t.model.name)))
        .filter(|t| shows.get(&t.model.name).is_none_or(generates_text))
        .map(|t| {
            let name = &t.model.name;
            model(t, shows.get(name), manifest.get(name))
        })
        .collect();
    models.sort_by_cached_key(|m| {
        (
            !m.from_gram,
            m.display_name.to_lowercase(),
            m.quant.as_deref().map(str::to_lowercase),
            m.name.to_lowercase(),
        )
    });
    models
}

/// Génère du texte. Sans capacité connue (Ollama ancien), le modèle est gardé.
fn generates_text(show: &ModelShow) -> bool {
    show.capabilities.is_empty() || show.has(&Capability::Completion)
}

fn model(t: &TaggedModel, show: Option<&ModelShow>, entry: Option<&SharedModel>) -> HarnessModel {
    let has = |c: Capability| show.is_some_and(|s| s.has(&c));
    let OllamaModel {
        name,
        repo_id,
        quant,
        size,
    } = &t.model;
    // L'étiquette d'un modèle de Hugging Face est sa quantification ; celle d'un modèle de la
    // bibliothèque d'Ollama, une taille (`qwen3:8b`).
    let quant = match repo_id {
        Some(_) => quant.clone(),
        None => t.quantization_level.clone(),
    };
    HarnessModel {
        name: name.clone(),
        display_name: entry.map_or_else(|| name.clone(), |e| e.display_name.clone()),
        from_gram: entry.is_some(),
        quant: entry.map(|e| e.quant.clone()).or(quant),
        size: *size,
        ctx: show.and_then(|s| s.num_ctx).or(entry.map(|e| e.ctx)),
        // Sans `/api/show`, le catalogue de GRAM dit si le modèle accepte des images.
        vision: has(Capability::Vision) || (show.is_none() && entry.is_some_and(|e| e.vision)),
        thinking: has(Capability::Thinking),
        tools: has(Capability::Tools),
        estimated_tok_s: entry.map(|e| e.estimated_tok_s),
        measured_tok_s: entry.and_then(|e| e.measured_tok_s),
        use_case: entry.map(|e| e.use_case),
    }
}

/// Clé du cache : nom, empreinte et taille du modèle.
type ShowKey = (String, String, u64);

fn show_key(t: &TaggedModel) -> ShowKey {
    (t.model.name.clone(), t.digest.clone(), t.model.size)
}

/// Réponses de `/api/show` déjà lues : `/api/show` lit l'en-tête du GGUF, inutile de le refaire
/// à chaque rafraîchissement de la liste.
#[derive(Default)]
pub struct ShowCache(Mutex<HashMap<ShowKey, ModelShow>>);

impl ShowCache {
    /// Capacités de chaque modèle installé, par nom ; `/api/show` n'est appelé (en parallèle)
    /// que pour ceux qui ne sont pas en cache. Un échec est journalisé : le modèle reste sans
    /// capacités connues, et sera relu la fois suivante. Les modèles disparus sortent du cache.
    pub async fn fill(
        &self,
        client: &OllamaClient,
        tags: &[TaggedModel],
    ) -> HashMap<String, ModelShow> {
        let missing: Vec<ShowKey> = {
            let cache = self.0.lock().expect("verrou du cache");
            tags.iter()
                .map(show_key)
                .filter(|k| !cache.contains_key(k))
                .collect()
        };
        let mut tasks = JoinSet::new();
        for key in missing {
            let client = client.clone();
            tasks.spawn(async move {
                let show = client.show(&key.0).await;
                (key, show)
            });
        }
        let mut fetched = Vec::new();
        while let Some(done) = tasks.join_next().await {
            match done {
                Ok((key, Ok(show))) => fetched.push((key, show)),
                Ok((key, Err(e))) => tracing::warn!(model = %key.0, "/api/show : {e}"),
                Err(e) => tracing::warn!("/api/show interrompu : {e}"),
            }
        }

        let mut cache = self.0.lock().expect("verrou du cache");
        cache.extend(fetched);
        let current: HashSet<ShowKey> = tags.iter().map(show_key).collect();
        cache.retain(|k, _| current.contains(k));
        tags.iter()
            .filter_map(|t| {
                let show = cache.get(&show_key(t))?;
                Some((t.model.name.clone(), show.clone()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gram_ollama::testing::{FakeOllama, Recorded, SHOW_ALIAS, TAGS, tags_json};
    use serde_json::json;

    const SHOW_BASE: &str = include_str!("../../../tests/fixtures/ollama/show-base.json");
    const SHOW_VISION: &str = include_str!("../../../tests/fixtures/ollama/show-vision.json");
    const EMBEDDING: &str = r#"{"capabilities":["embedding"]}"#;

    const ALIAS: &str = "gram/qwen3-8b:q4_k_m";
    const BASE: &str = "hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M";
    /// Base d'une entrée du manifeste dont le modèle dérivé a été supprimé d'Ollama.
    const ORPHAN_BASE: &str = "hf.co/unsloth/Qwen3-1.7B-GGUF:Q4_K_M";

    fn entry(ollama_name: &str, base: &str, display: &str, measured: Option<f32>) -> SharedModel {
        serde_json::from_value(json!({
            "ollamaName": ollama_name,
            "baseName": base,
            "baseInstalledByGram": false,
            "displayName": display,
            "repoId": "Qwen/Qwen3-8B-GGUF",
            "quant": "Q4_K_M",
            "size": 5_027_797_508_u64,
            "ctx": 16384,
            "kvCache": "f16",
            "useCase": "code",
            "vision": false,
            "estimatedTokS": 45.0,
            "measuredTokS": measured,
            "addedAt": "2026-11-13T10:00:00Z",
        }))
        .unwrap()
    }

    fn manifest() -> SharedManifest {
        SharedManifest {
            models: vec![
                entry(ALIAS, BASE, "Qwen3 8B", Some(43.0)),
                entry("gram/qwen3-1.7b:q4_k_m", ORPHAN_BASE, "Qwen3 1.7B", None),
            ],
            ..SharedManifest::default()
        }
    }

    /// Modèles installés : dérivé et sa base, base orpheline, bibliothèque d'Ollama, plongements,
    /// vision, et un modèle dont `/api/show` échoue.
    const INSTALLED: [&str; 7] = [
        "qwen3:8b",
        BASE,
        ALIAS,
        "nomic-embed-text:latest",
        ORPHAN_BASE,
        "Llava:7b",
        "abime:1b",
    ];

    fn respond(r: &Recorded) -> (u16, String) {
        match (r.path.as_str(), r.json()["model"].as_str().unwrap_or("")) {
            ("/api/tags", _) => (200, tags_json(&INSTALLED)),
            ("/api/show", ALIAS) => (200, SHOW_ALIAS.into()),
            ("/api/show", "nomic-embed-text:latest") => (200, EMBEDDING.into()),
            ("/api/show", "Llava:7b") => (200, SHOW_VISION.into()),
            ("/api/show", "abime:1b") => (500, r#"{"error":"fichier illisible"}"#.into()),
            ("/api/show", _) => (200, SHOW_BASE.into()),
            _ => (404, r#"{"error":"404 page not found"}"#.into()),
        }
    }

    fn names(models: &[HarnessModel]) -> Vec<&str> {
        models.iter().map(|m| m.name.as_str()).collect()
    }

    fn shows(fake: &FakeOllama) -> usize {
        fake.requests()
            .iter()
            .filter(|r| r.path == "/api/show")
            .count()
    }

    #[tokio::test]
    async fn fusion_et_masquage() {
        let fake = FakeOllama::start(respond);
        let client = OllamaClient::new(fake.url()).unwrap();
        let cache = ShowCache::default();
        let models = list_models(&client, &manifest(), &cache).await.unwrap();

        // GRAM d'abord ; base du modèle dérivé et plongements masqués ; la base orpheline et le
        // modèle sans capacités connues restent.
        assert_eq!(
            names(&models),
            [
                "gram/qwen3-8b:q4_k_m",
                "abime:1b",
                ORPHAN_BASE,
                "Llava:7b",
                "qwen3:8b"
            ]
        );
        let gram = &models[0];
        assert_eq!(
            *gram,
            HarnessModel {
                name: ALIAS.into(),
                display_name: "Qwen3 8B".into(),
                from_gram: true,
                quant: Some("Q4_K_M".into()),
                size: 1,
                ctx: Some(16384),
                vision: false,
                thinking: true,
                tools: true,
                estimated_tok_s: Some(45.0),
                measured_tok_s: Some(43.0),
                use_case: Some(UseCase::Code),
            }
        );
        let other = |name: &str| models.iter().find(|m| m.name == name).unwrap();
        let orphan = other(ORPHAN_BASE);
        assert!(!orphan.from_gram);
        assert_eq!(orphan.display_name, ORPHAN_BASE);
        assert_eq!(orphan.quant.as_deref(), Some("Q4_K_M"));
        assert_eq!((orphan.ctx, orphan.estimated_tok_s), (None, None));
        let llava = other("Llava:7b");
        assert!(llava.vision && !llava.thinking && !llava.tools);
        let broken = other("abime:1b");
        assert!(!broken.vision && !broken.thinking);
        assert_eq!(shows(&fake), INSTALLED.len());

        // Sérialisation en camelCase, comme la lit l'interface.
        let json = serde_json::to_value(gram).unwrap();
        assert_eq!(json["displayName"], "Qwen3 8B");
        assert_eq!(json["fromGram"], true);
        assert_eq!(json["useCase"], "code");
        assert_eq!(json["measuredTokS"], 43.0);
    }

    #[tokio::test]
    async fn manifeste_absent() {
        let fake = FakeOllama::start(respond);
        let client = OllamaClient::new(fake.url()).unwrap();
        let models = list_models(&client, &SharedManifest::default(), &ShowCache::default())
            .await
            .unwrap();
        // Tout sous le nom technique, la base comprise.
        assert!(
            models
                .iter()
                .all(|m| !m.from_gram && m.display_name == m.name)
        );
        assert_eq!(
            names(&models),
            ["abime:1b", ALIAS, BASE, ORPHAN_BASE, "Llava:7b", "qwen3:8b"]
        );
        let alias = models.iter().find(|m| m.name == ALIAS).unwrap();
        assert_eq!(alias.ctx, Some(16384), "contexte lu dans /api/show");
        // Ni manifeste, ni quantification annoncée par Ollama (`details` vide).
        assert_eq!(alias.quant, None);
    }

    #[tokio::test]
    async fn capacites_en_cache() {
        let fake = FakeOllama::start(respond);
        let client = OllamaClient::new(fake.url()).unwrap();
        let cache = ShowCache::default();
        list_models(&client, &manifest(), &cache).await.unwrap();
        let first = shows(&fake);
        let again = list_models(&client, &manifest(), &cache).await.unwrap();
        // Seul le modèle en échec est relu.
        assert_eq!(shows(&fake), first + 1);
        assert_eq!(again.len(), 5);

        // Modèle refait sous le même nom (autre empreinte) : relu.
        let fake = FakeOllama::start(|r| match r.path.as_str() {
            "/api/tags" => (200, TAGS.into()),
            _ => (200, SHOW_BASE.into()),
        });
        let client = OllamaClient::new(fake.url()).unwrap();
        let cache = ShowCache::default();
        let real = list_models(&client, &SharedManifest::default(), &cache)
            .await
            .unwrap();
        assert_eq!(shows(&fake), 3);
        // Quantification : l'étiquette pour Hugging Face, `details` pour la bibliothèque.
        let quants: Vec<_> = real.iter().map(|m| m.quant.as_deref()).collect();
        assert_eq!(
            quants,
            [Some("Q4_K_M"), Some("UD-Q4_K_XL"), Some("Q4_K_M")],
            "{:?}",
            names(&real)
        );
        assert_eq!(real[2].name, "qwen3:8b");
        let redone = TAGS.replacen("b14ffb635b5b", "c0ffee000000", 1);
        let tagged = gram_ollama::api::parse_tagged(redone.as_bytes()).unwrap();
        cache.fill(&client, &tagged).await;
        assert_eq!(shows(&fake), 4);
        // Les modèles disparus sortent du cache.
        cache.fill(&client, &tagged[..1]).await;
        assert_eq!(cache.0.lock().unwrap().len(), 1);
    }

    #[test]
    fn modele_sans_show() {
        // `/api/show` en échec : le contexte et la vision viennent du manifeste.
        let tags = [TaggedModel {
            model: OllamaModel::new(ALIAS, 7, None),
            digest: String::new(),
            quantization_level: None,
        }];
        let mut m = manifest();
        m.models[0].vision = true;
        let models = merge(&tags, &HashMap::new(), &m);
        assert_eq!(models[0].ctx, Some(16384));
        assert!(models[0].vision && !models[0].thinking);
        // Capacités inconnues (Ollama ancien) : gardé.
        let empty = HashMap::from([(ALIAS.to_string(), ModelShow::default())]);
        assert_eq!(merge(&tags, &empty, &m).len(), 1);
        assert!(!merge(&tags, &empty, &m)[0].vision);
    }

    #[tokio::test]
    async fn ollama_absent() {
        let addr = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let client = OllamaClient::for_addr(addr).unwrap();
        let err = list_models(&client, &manifest(), &ShowCache::default())
            .await
            .unwrap_err();
        assert!(matches!(err, gram_ollama::OllamaError::Unreachable(_)));
    }
}
