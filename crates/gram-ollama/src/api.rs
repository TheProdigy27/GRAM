//! Format des requêtes et réponses de l'API d'Ollama (`https://docs.ollama.com/api`).
//!
//! Seul module qui connaît ces formats : si Ollama les change (§16 du plan), c'est ici qu'il
//! faut les mettre à jour. Les champs absents prennent une valeur par défaut, les champs
//! inconnus sont ignorés.

use crate::error::{OllamaError, Result};
use gram_core::{LoadedModel, OllamaModel};
use serde::{Deserialize, Serialize};

// ── GET /api/tags : modèles installés ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<TagEntry>,
}

#[derive(Debug, Deserialize)]
struct TagEntry {
    #[serde(default)]
    name: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    details: Details,
    /// Modèle hébergé à distance (`ollama.com`) : il n'occupe pas cette machine.
    #[serde(default)]
    remote_host: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Details {
    #[serde(default)]
    quantization_level: Option<String>,
}

/// Modèles installés sur cette machine (les modèles distants sont écartés).
pub fn parse_tags(body: &[u8]) -> Result<Vec<OllamaModel>> {
    let tags: TagsResponse = parse(body, "/api/tags")?;
    Ok(tags
        .models
        .into_iter()
        .filter(|t| t.remote_host.as_deref().is_none_or(str::is_empty))
        .filter_map(|t| {
            let name = if t.name.is_empty() { t.model } else { t.name };
            (!name.is_empty())
                .then(|| OllamaModel::new(name, t.size, t.details.quantization_level.as_deref()))
        })
        .collect())
}

// ── GET /api/ps : modèles chargés en mémoire ────────────────────────────────────────────────

/// Un modèle chargé par Ollama.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PsEntry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub model: String,
    /// Mémoire occupée, en octets.
    #[serde(default)]
    pub size: u64,
    /// Dont VRAM, en octets.
    #[serde(default)]
    pub size_vram: u64,
    /// Contexte du modèle chargé (versions récentes d'Ollama).
    #[serde(default)]
    pub context_length: Option<u32>,
}

impl PsEntry {
    /// Est-ce le modèle `name` (sans tenir compte de la casse ; `:latest` implicite) ?
    pub fn is(&self, name: &str) -> bool {
        let wanted = normalize(name);
        [&self.name, &self.model]
            .iter()
            .any(|n| !n.is_empty() && normalize(n) == wanted)
    }

    pub fn loaded(&self) -> LoadedModel {
        LoadedModel {
            size: self.size,
            size_vram: self.size_vram,
            context_length: self.context_length.filter(|&c| c > 0),
        }
    }
}

/// Nom en minuscules, avec l'étiquette `latest` quand elle est implicite.
fn normalize(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    let last = lower.rsplit('/').next().unwrap_or("");
    if last.contains(':') {
        lower
    } else {
        format!("{lower}:latest")
    }
}

#[derive(Debug, Deserialize)]
struct PsResponse {
    #[serde(default)]
    models: Vec<PsEntry>,
}

pub fn parse_ps(body: &[u8]) -> Result<Vec<PsEntry>> {
    Ok(parse::<PsResponse>(body, "/api/ps")?.models)
}

// ── POST /api/generate ──────────────────────────────────────────────────────────────────────

/// Requête de génération, sans flux (`stream: false`) : une seule réponse JSON à la fin.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GenerateRequest<'a> {
    pub model: &'a str,
    pub prompt: &'a str,
    pub stream: bool,
    /// Durée de chargement après la requête (« 5m ») : le modèle reste visible dans `/api/ps`.
    pub keep_alive: &'a str,
    pub options: GenerateOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct GenerateOptions {
    /// Contexte en tokens (change la mémoire occupée : le modèle est rechargé s'il diffère).
    pub num_ctx: u32,
    /// Tokens générés au plus.
    pub num_predict: u32,
    pub temperature: f32,
    pub seed: i64,
}

/// Statistiques de fin de génération (toutes les durées sont en nanosecondes).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct GenerateResponse {
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub done_reason: Option<String>,
    #[serde(default)]
    pub total_duration: u64,
    #[serde(default)]
    pub load_duration: u64,
    /// Tokens du prompt, y compris ceux repris du cache.
    #[serde(default)]
    pub prompt_eval_count: u32,
    /// Dont repris du cache du modèle chargé, sans calcul (Ollama 0.40 et suivants).
    #[serde(default)]
    pub prompt_eval_cached_count: u32,
    #[serde(default)]
    pub prompt_eval_duration: u64,
    #[serde(default)]
    pub eval_count: u32,
    #[serde(default)]
    pub eval_duration: u64,
}

pub fn parse_generate(body: &[u8]) -> Result<GenerateResponse> {
    let r: GenerateResponse = parse(body, "/api/generate")?;
    if !r.done {
        return Err(OllamaError::Invalid(
            "génération inachevée (/api/generate)".into(),
        ));
    }
    Ok(r)
}

// ── GET /api/version, erreurs ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct VersionResponse {
    #[serde(default)]
    version: String,
}

pub fn parse_version(body: &[u8]) -> Result<String> {
    let v: VersionResponse = parse(body, "/api/version")?;
    Ok(v.version)
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
}

/// Message d'une réponse d'erreur (`{"error": "model 'x' not found"}`), sinon le début du corps.
pub fn error_message(body: &[u8]) -> String {
    match serde_json::from_slice::<ErrorResponse>(body) {
        Ok(e) => e.error,
        Err(_) => {
            let text = String::from_utf8_lossy(body);
            let text = text.trim();
            match text.char_indices().nth(200) {
                Some((i, _)) => format!("{}…", &text[..i]),
                None if text.is_empty() => "réponse vide".into(),
                None => text.to_string(),
            }
        }
    }
}

fn parse<T: for<'de> Deserialize<'de>>(body: &[u8], what: &str) -> Result<T> {
    serde_json::from_slice(body).map_err(|e| OllamaError::Invalid(format!("{what} : {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! fixture {
        ($name:literal) => {
            include_bytes!(concat!("../../../tests/fixtures/ollama/", $name))
        };
    }

    #[test]
    fn modeles_installes() {
        let models = parse_tags(fixture!("tags.json")).unwrap();
        let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M",
                "hf.co/unsloth/Qwen3-30B-A3B-GGUF:UD-Q4_K_XL",
                "qwen3:8b",
            ],
            "le modèle distant est écarté"
        );
        // Ollama garde la casse du nom donné à `ollama pull`.
        assert_eq!(models[0].repo_id.as_deref(), Some("Qwen/Qwen3-8B-GGUF"));
        assert_eq!(models[0].size, 5_027_797_508);
        // L'étiquette du nom l'emporte sur `quantization_level`.
        assert_eq!(models[1].quant.as_deref(), Some("UD-Q4_K_XL"));
        assert_eq!(models[2].repo_id, None);

        assert!(parse_tags(br#"{"models":[]}"#).unwrap().is_empty());
        assert!(parse_tags(b"{}").unwrap().is_empty());
        assert!(matches!(
            parse_tags(b"<html>"),
            Err(OllamaError::Invalid(m)) if m.starts_with("/api/tags")
        ));
    }

    #[test]
    fn modeles_charges() {
        let ps = parse_ps(fixture!("ps.json")).unwrap();
        assert_eq!(ps.len(), 1);
        let e = &ps[0];
        assert!(e.is("hf.co/qwen/qwen3-8b-gguf:q4_k_m"));
        assert!(!e.is("hf.co/Qwen/Qwen3-8B-GGUF:Q8_0"));
        assert_eq!(
            e.loaded(),
            LoadedModel {
                size: 6_114_634_628,
                size_vram: 6_114_634_628,
                context_length: Some(8192),
            }
        );
        // `:latest` implicite.
        let latest = PsEntry {
            name: "qwen3:latest".into(),
            model: String::new(),
            size: 1,
            size_vram: 0,
            context_length: Some(0),
        };
        assert!(latest.is("qwen3") && latest.is("Qwen3:latest"));
        assert_eq!(latest.loaded().context_length, None);
        assert!(parse_ps(br#"{"models":[]}"#).unwrap().is_empty());
    }

    #[test]
    fn generation() {
        let g = parse_generate(fixture!("generate.json")).unwrap();
        assert_eq!(g.eval_count, 128);
        assert_eq!(g.eval_duration, 946_333_000);
        assert_eq!(g.prompt_eval_count, 45);
        // Début du modèle de conversation, en cache depuis l'échauffement.
        assert_eq!(g.prompt_eval_cached_count, 3);
        assert_eq!(g.prompt_eval_duration, 18_157_000);
        assert_eq!(g.load_duration, 2_126_300);
        assert_eq!(g.done_reason.as_deref(), Some("length"));
        // L'échauffement charge le modèle.
        let w = parse_generate(fixture!("generate-warmup.json")).unwrap();
        assert_eq!((w.eval_count, w.load_duration), (8, 1_843_339_600));
        assert_eq!(w.prompt_eval_cached_count, 0);
        assert!(matches!(
            parse_generate(br#"{"done": false}"#),
            Err(OllamaError::Invalid(_))
        ));
    }

    #[test]
    fn requete_de_generation() {
        let req = GenerateRequest {
            model: "hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M",
            prompt: "Bonjour",
            stream: false,
            keep_alive: "5m",
            options: GenerateOptions {
                num_ctx: 8192,
                num_predict: 128,
                temperature: 0.0,
                seed: 42,
            },
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["stream"], false);
        assert_eq!(json["keep_alive"], "5m");
        assert_eq!(json["options"]["num_ctx"], 8192);
        assert_eq!(json["options"]["num_predict"], 128);
    }

    #[test]
    fn version_et_erreurs() {
        assert_eq!(
            parse_version(br#"{"version":"0.40.16"}"#).unwrap(),
            "0.40.16"
        );
        assert_eq!(
            error_message(fixture!("error-not-found.json")),
            "model 'hf.co/Qwen/Qwen3-8B-GGUF:Q8_0' not found"
        );
        assert_eq!(error_message(b"  Bad Gateway \n"), "Bad Gateway");
        assert_eq!(error_message(b""), "réponse vide");
        assert_eq!(error_message(&[b'x'; 500]).chars().count(), 201);
    }
}
