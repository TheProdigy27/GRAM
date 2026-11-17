//! Format des requêtes et réponses de l'API d'Ollama (`https://docs.ollama.com/api`).
//!
//! Seul module qui connaît ces formats : si Ollama les change (§16 du plan), c'est ici qu'il
//! faut les mettre à jour. Les champs absents prennent une valeur par défaut, les champs
//! inconnus sont ignorés.

use crate::error::{OllamaError, Result};
use gram_core::harness::PullFailure;
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
    digest: String,
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

/// Un modèle installé et l'empreinte de son manifeste dans Ollama.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggedModel {
    pub model: OllamaModel,
    /// Change quand le modèle est refait sous le même nom (modèle dérivé réglé à un autre
    /// contexte), même si sa taille ne change pas ; vide si Ollama ne la donne pas.
    pub digest: String,
    /// Quantification annoncée par Ollama (`details.quantization_level`), seule à valoir pour
    /// un modèle de sa bibliothèque : l'étiquette de `qwen3:8b` est une taille.
    pub quantization_level: Option<String>,
}

/// Modèles installés sur cette machine (les modèles distants sont écartés).
pub fn parse_tags(body: &[u8]) -> Result<Vec<OllamaModel>> {
    Ok(parse_tagged(body)?.into_iter().map(|t| t.model).collect())
}

/// Comme [`parse_tags`], avec l'empreinte de chaque modèle.
pub fn parse_tagged(body: &[u8]) -> Result<Vec<TaggedModel>> {
    let tags: TagsResponse = parse(body, "/api/tags")?;
    Ok(tags
        .models
        .into_iter()
        .filter(|t| t.remote_host.as_deref().is_none_or(str::is_empty))
        .filter_map(|t| {
            let name = if t.name.is_empty() { t.model } else { t.name };
            let level = t
                .details
                .quantization_level
                .filter(|q| !q.trim().is_empty());
            (!name.is_empty()).then(|| TaggedModel {
                model: OllamaModel::new(name, t.size, level.as_deref()),
                digest: t.digest,
                quantization_level: level,
            })
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

/// Même modèle, sans tenir compte de la casse (Ollama garde celle du `pull`) ; `:latest`
/// implicite.
pub fn same_model(a: &str, b: &str) -> bool {
    normalize(a) == normalize(b)
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

// ── POST /api/chat : discussion, en flux NDJSON ─────────────────────────────────────────────

/// Requête de discussion, en flux (`stream: true`). L'API est sans état : toute la conversation
/// est envoyée à chaque fois.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChatRequest<'a> {
    pub model: &'a str,
    pub messages: &'a [ChatMessage],
    pub stream: bool,
    /// Réflexion. Absente, Ollama décide : Qwen3 réfléchit, et la réflexion arrive à part, dans
    /// `message.thinking` (constaté avec Ollama 0.40.16, `chat-default.ndjson`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub think: Option<bool>,
    /// Durée de chargement après la réponse (« 30m », « -1m » : toujours) ; absente : celle
    /// d'Ollama (5 min, ou `OLLAMA_KEEP_ALIVE`). Une durée sans unité (« -1 ») est refusée
    /// (HTTP 400, constaté avec Ollama 0.40.16).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_alive: Option<&'a str>,
    /// Absentes : les paramètres du modèle (dont `num_ctx` d'un modèle dérivé de GRAM).
    #[serde(skip_serializing_if = "ChatOptions::is_empty")]
    pub options: ChatOptions,
}

/// Options d'une discussion ; chacune absente garde le réglage du modèle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct ChatOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_ctx: Option<u32>,
    /// Tokens générés au plus.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_predict: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
}

impl ChatOptions {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Auteur d'un message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    /// Instructions, en premier message.
    System,
    User,
    Assistant,
}

/// Un message de la conversation envoyée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    /// Images en base64 (modèles de vision).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    /// Réflexion d'une réponse précédente, que le modèle de conversation peut reprendre.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
}

impl ChatMessage {
    pub fn new(role: ChatRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            images: Vec::new(),
            thinking: None,
        }
    }
}

/// Bilan d'une réponse, sur la dernière ligne du flux (durées en nanosecondes).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct ChatDone {
    /// `stop` (fin naturelle), `length` (limite de tokens atteinte)…
    #[serde(default)]
    pub done_reason: Option<String>,
    #[serde(default)]
    pub total_duration: u64,
    /// Chargement du modèle, s'il n'était pas déjà en mémoire.
    #[serde(default)]
    pub load_duration: u64,
    /// Tokens de la conversation envoyée, y compris ceux repris du cache.
    #[serde(default)]
    pub prompt_eval_count: u32,
    #[serde(default)]
    pub prompt_eval_cached_count: u32,
    #[serde(default)]
    pub prompt_eval_duration: u64,
    /// Tokens générés, réflexion comprise.
    #[serde(default)]
    pub eval_count: u32,
    #[serde(default)]
    pub eval_duration: u64,
}

/// Une ligne du flux de `POST /api/chat`, par exemple :
///
/// ```text
/// {"model":"…","message":{"role":"assistant","content":"","thinking":"Okay"},"done":false}
/// {"model":"…","message":{"role":"assistant","content":"Bonjour"},"done":false}
/// {"model":"…","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop",…}
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatChunk {
    /// Morceau de la réponse (souvent vide pendant la réflexion).
    pub content: String,
    /// Morceau de la réflexion.
    pub thinking: String,
    /// Bilan, sur la dernière ligne seulement.
    pub done: Option<ChatDone>,
}

#[derive(Debug, Deserialize)]
struct ChatLine {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    message: Option<ChatLineMessage>,
    #[serde(default)]
    done: bool,
    #[serde(flatten)]
    stats: ChatDone,
}

#[derive(Debug, Default, Deserialize)]
struct ChatLineMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    thinking: String,
}

/// Une ligne du flux. Une erreur arrive aussi par une ligne (`{"error": "…"}`), après le
/// statut HTTP 200.
pub fn parse_chat_line(line: &[u8]) -> Result<ChatChunk> {
    let line: ChatLine = parse(line, "/api/chat")?;
    if let Some(error) = line.error {
        return Err(OllamaError::Interrupted(error));
    }
    let message = line.message.unwrap_or_default();
    Ok(ChatChunk {
        content: message.content,
        thinking: message.thinking,
        done: line.done.then_some(line.stats),
    })
}

/// Erreur d'Ollama qui dit que le modèle n'est pas installé (`model '…' not found`, HTTP 404).
pub fn is_model_not_found(status: u16, message: &str) -> bool {
    status == 404 && message.contains("not found")
}

/// Erreur d'Ollama qui dit que le modèle n'accepte pas d'images (`image input is not
/// supported…`, HTTP 500), même quand l'image n'est que dans l'historique
/// (`chat-image-unsupported.json`).
pub fn is_image_unsupported(message: &str) -> bool {
    message.contains("image input is not supported")
}

// ── POST /api/pull : téléchargement, en flux NDJSON ─────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PullRequest<'a> {
    pub model: &'a str,
    pub stream: bool,
}

/// Une ligne du flux de `POST /api/pull`, par exemple :
///
/// ```text
/// {"status":"pulling manifest"}
/// {"status":"pulling 2a31195d3769","digest":"sha256:2a31…","total":175054528,"completed":39538779}
/// {"status":"verifying sha256 digest"}
/// {"status":"writing manifest"}
/// {"status":"success"}
/// ```
///
/// `completed` manque tant qu'Ollama n'a rien reçu pour la couche ; la dernière ligne d'une
/// couche n'atteint pas toujours `total`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct PullEvent {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub completed: Option<u64>,
}

impl PullEvent {
    /// Progression d'une couche : empreinte, taille et octets reçus.
    pub fn layer(&self) -> Option<(&str, u64, Option<u64>)> {
        let digest = self.digest.as_deref().filter(|d| !d.is_empty())?;
        Some((digest, self.total?, self.completed))
    }

    /// Ollama vérifie les couches téléchargées puis écrit le manifeste : plus rien n'arrive
    /// pendant la vérification, qui dure environ une minute pour 40 Go.
    pub fn is_verifying(&self) -> bool {
        ["verifying", "writing manifest", "removing"]
            .iter()
            .any(|s| self.status.starts_with(s))
            || self.is_success()
    }

    /// Dernière ligne d'un téléchargement réussi.
    pub fn is_success(&self) -> bool {
        self.status == "success"
    }
}

#[derive(Debug, Deserialize)]
struct PullLine {
    #[serde(default)]
    error: Option<String>,
    #[serde(flatten)]
    event: PullEvent,
}

/// Une ligne du flux. Une erreur arrive aussi par une ligne (`{"error": "…"}`), après le
/// statut HTTP 200 : manifeste introuvable, dépôt protégé, connexion perdue…
pub fn parse_pull_line(line: &[u8]) -> Result<PullEvent> {
    let line: PullLine = parse(line, "/api/pull")?;
    match line.error {
        Some(error) => Err(OllamaError::Interrupted(error)),
        None => Ok(line.event),
    }
}

/// Reconnaît, dans le message d'erreur d'un téléchargement depuis Hugging Face, un échec que
/// l'utilisateur peut corriger (textes d'Ollama 0.40.16, dans `pull-error-*.ndjson`) :
///
/// - `realm host "huggingface.co" does not match original host "hf.co"` (dépôt protégé ou
///   inexistant depuis `hf.co/`) et `401: {"error":"Invalid username or password."}` (depuis
///   `huggingface.co/`, clé d'Ollama non enregistrée) ;
/// - `400: {"error":"The specified tag is not a valid quantization scheme…"}` ;
/// - `400: {"error":"This repository only contains sharded GGUF files…"}` ;
/// - `invalid split GGUF: split GGUF has 2 remaining shards, expected 5`, renvoyé par
///   `/api/create` (HTTP 500) quand le fichier téléchargé n'est que la première partie d'un
///   modèle découpé, sous un nom qui ne le dit pas (`create-error-split.json`).
pub fn pull_failure(message: &str) -> Option<PullFailure> {
    if message.contains("does not match original host") || message.contains("manifest: 401") {
        Some(PullFailure::Unauthorized)
    } else if message.contains("not a valid quantization scheme") {
        Some(PullFailure::UnknownQuant)
    } else if message.contains("sharded GGUF") {
        Some(PullFailure::Sharded)
    } else if message.contains("invalid split GGUF") {
        Some(PullFailure::SplitPart)
    } else {
        None
    }
}

// ── POST /api/create : modèle dérivé ────────────────────────────────────────────────────────

/// Nouveau modèle `model` qui reprend les couches de `from` (sans les copier : seul un petit
/// manifeste s'ajoute) avec ses paramètres, plus ceux de `parameters`.
///
/// Si `from` n'est pas installé, Ollama le **télécharge** sans rien dire (constaté avec
/// Ollama 0.40.16) : vérifier sa présence dans `/api/tags` avant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateRequest<'a> {
    pub model: &'a str,
    pub from: &'a str,
    pub parameters: CreateParameters,
    pub stream: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CreateParameters {
    /// Contexte en tokens, utilisé par tout client qui ne le précise pas.
    pub num_ctx: u32,
}

#[derive(Debug, Deserialize)]
struct StatusResponse {
    #[serde(default)]
    status: String,
}

/// Réponse sans flux de `/api/create` : `{"status":"success"}`.
pub fn parse_create(body: &[u8]) -> Result<()> {
    let r: StatusResponse = parse(body, "/api/create")?;
    if r.status == "success" {
        Ok(())
    } else {
        Err(OllamaError::Invalid(format!(
            "/api/create : statut « {} » au lieu de « success »",
            r.status
        )))
    }
}

// ── POST /api/show, DELETE /api/delete ──────────────────────────────────────────────────────

/// Corps de `/api/show` et de `/api/delete`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelRequest<'a> {
    pub model: &'a str,
}

/// Capacité d'un modèle d'après Ollama. Les valeurs inconnues sont gardées telles quelles.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Capability {
    /// Génère du texte.
    Completion,
    /// Accepte des images (projecteur `mmproj` installé avec le modèle).
    Vision,
    /// Réfléchit avant de répondre (`think`).
    Thinking,
    /// Appelle des outils.
    Tools,
    /// Remplit un trou dans du code (`suffix`).
    Insert,
    /// Modèle de plongements, sans génération.
    Embedding,
    Other(String),
}

impl From<String> for Capability {
    fn from(s: String) -> Self {
        match s.as_str() {
            "completion" => Self::Completion,
            "vision" => Self::Vision,
            "thinking" => Self::Thinking,
            "tools" => Self::Tools,
            "insert" => Self::Insert,
            "embedding" => Self::Embedding,
            _ => Self::Other(s),
        }
    }
}

impl From<Capability> for String {
    fn from(c: Capability) -> Self {
        match c {
            Capability::Completion => "completion".into(),
            Capability::Vision => "vision".into(),
            Capability::Thinking => "thinking".into(),
            Capability::Tools => "tools".into(),
            Capability::Insert => "insert".into(),
            Capability::Embedding => "embedding".into(),
            Capability::Other(s) => s,
        }
    }
}

/// Ce que GRAM lit dans `/api/show`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelShow {
    pub capabilities: Vec<Capability>,
    /// Contexte réglé dans le modèle (`num_ctx` de ses paramètres) ; absent s'il n'est pas
    /// réglé, Ollama choisit alors.
    pub num_ctx: Option<u32>,
}

impl ModelShow {
    pub fn has(&self, capability: &Capability) -> bool {
        self.capabilities.contains(capability)
    }
}

#[derive(Debug, Deserialize)]
struct ShowResponse {
    #[serde(default)]
    capabilities: Vec<Capability>,
    /// Une ligne par paramètre, nom puis valeur séparés par des espaces :
    /// `"stop  \"<|im_end|>\"\nnum_ctx  16384\n…"` ; absent si le modèle n'en a aucun.
    #[serde(default)]
    parameters: Option<String>,
}

pub fn parse_show(body: &[u8]) -> Result<ModelShow> {
    let r: ShowResponse = parse(body, "/api/show")?;
    Ok(ModelShow {
        capabilities: r.capabilities,
        num_ctx: r.parameters.as_deref().and_then(num_ctx),
    })
}

fn num_ctx(parameters: &str) -> Option<u32> {
    parameters.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        (words.next() == Some("num_ctx"))
            .then(|| words.next()?.parse().ok())
            .flatten()
    })
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
        // Avec l'empreinte du manifeste.
        let tagged = parse_tagged(fixture!("tags.json")).unwrap();
        assert_eq!(tagged.len(), 3);
        assert_eq!(tagged[0].model, models[0]);
        assert_eq!(
            tagged[0].digest,
            "b14ffb635b5bbdb62bbb1aed050de1c32b12a5b139eb5adeb529497c6622bc1b"
        );
        assert_eq!(tagged[1].quantization_level.as_deref(), Some("Q4_K_M"));
        let bare =
            &parse_tagged(br#"{"models":[{"name":"a:b","details":{"quantization_level":""}}]}"#)
                .unwrap()[0];
        assert_eq!(
            (bare.digest.as_str(), bare.quantization_level.as_deref()),
            ("", None)
        );

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

    /// Réponse et réflexion d'un flux de `/api/chat`, et son bilan.
    fn chat_stream(text: &[u8]) -> (String, String, ChatDone, usize) {
        let mut content = String::new();
        let mut thinking = String::new();
        let mut done = None;
        let mut lines = 0;
        for line in text.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
            let chunk = parse_chat_line(line).unwrap();
            assert!(done.is_none(), "rien après le bilan");
            content.push_str(&chunk.content);
            thinking.push_str(&chunk.thinking);
            done = chunk.done;
            lines += 1;
        }
        (content, thinking, done.expect("bilan"), lines)
    }

    #[test]
    fn discussion() {
        // Sans `think`, Qwen3 réfléchit, et la réflexion arrive à part.
        let (content, thinking, done, lines) = chat_stream(fixture!("chat-default.ndjson"));
        assert_eq!(lines, 100);
        assert_eq!(content, "Bonjour ! Comment puis-je vous aider ?");
        assert!(thinking.starts_with("Okay, the user said"), "{thinking}");
        assert_eq!(done.done_reason.as_deref(), Some("stop"));
        assert_eq!(
            (done.prompt_eval_count, done.eval_count),
            (18, 104),
            "réflexion comptée dans eval_count"
        );
        assert_eq!(done.eval_duration, 252_300_000);
        // Le modèle était à charger.
        assert_eq!(done.load_duration, 1_467_873_600);
        assert_eq!(done.total_duration, 1_784_520_700);

        // `think: true` : même format.
        let (content, thinking, done, _) = chat_stream(fixture!("chat-think.ndjson"));
        assert_eq!(content, "12 × 3 = 36.");
        assert!(thinking.contains("12 times 3 is 36"), "{thinking}");
        assert_eq!(done.prompt_eval_cached_count, 3);
        // `think: false` : aucune réflexion.
        let (content, thinking, done, lines) = chat_stream(fixture!("chat-no-think.ndjson"));
        assert_eq!((content.as_str(), thinking.as_str()), ("12 × 3 = 36.", ""));
        assert_eq!((lines, done.eval_count), (11, 11));
        // Limite de tokens atteinte (`num_predict` 5), avec des instructions et un historique.
        let (content, _, done, _) = chat_stream(fixture!("chat-length.ndjson"));
        assert_eq!(content, "Tu t'appelles L");
        assert_eq!(done.done_reason.as_deref(), Some("length"));
        assert_eq!(done.prompt_eval_count, 61);

        // Erreur au milieu du flux.
        assert!(matches!(
            parse_chat_line(br#"{"error":"llama runner process has terminated"}"#),
            Err(OllamaError::Interrupted(m)) if m == "llama runner process has terminated"
        ));
        assert!(matches!(
            parse_chat_line(b"<html>"),
            Err(OllamaError::Invalid(m)) if m.starts_with("/api/chat")
        ));
        // Modèle absent : HTTP 404.
        let message = error_message(fixture!("chat-not-found.json"));
        assert_eq!(message, "model 'gram/absent:q4_k_m' not found");
        assert!(is_model_not_found(404, &message));
        assert!(!is_model_not_found(500, &message));
        // Image pour un modèle sans vision : HTTP 500, avant tout flux.
        let message = error_message(fixture!("chat-image-unsupported.json"));
        assert!(
            message.starts_with("image input is not supported"),
            "{message}"
        );
        assert!(is_image_unsupported(&message));
        assert!(!is_image_unsupported("model 'x' not found"));
    }

    #[test]
    fn requete_de_discussion() {
        let messages = [
            ChatMessage::new(ChatRole::System, "Réponds brièvement."),
            ChatMessage::new(ChatRole::User, "Bonjour"),
            ChatMessage {
                images: vec!["aGVsbG8=".into()],
                ..ChatMessage::new(ChatRole::User, "Et ça ?")
            },
        ];
        let req = ChatRequest {
            model: "gram/qwen3-1.7b:q4_k_m",
            messages: &messages,
            stream: true,
            think: None,
            keep_alive: None,
            options: ChatOptions::default(),
        };
        // Sans réglage : ni `think`, ni `keep_alive`, ni `options` (ceux du modèle valent).
        assert_eq!(
            serde_json::to_value(&req).unwrap(),
            serde_json::json!({
                "model": "gram/qwen3-1.7b:q4_k_m",
                "messages": [
                    { "role": "system", "content": "Réponds brièvement." },
                    { "role": "user", "content": "Bonjour" },
                    { "role": "user", "content": "Et ça ?", "images": ["aGVsbG8="] },
                ],
                "stream": true,
            })
        );
        let req = ChatRequest {
            think: Some(false),
            keep_alive: Some("30m"),
            options: ChatOptions {
                num_predict: Some(5),
                seed: Some(42),
                ..ChatOptions::default()
            },
            ..req
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["think"], false);
        assert_eq!(json["keep_alive"], "30m");
        assert_eq!(
            json["options"],
            serde_json::json!({ "num_predict": 5, "seed": 42 })
        );
    }

    fn pull_lines(text: &[u8]) -> Vec<Result<PullEvent>> {
        text.split(|&b| b == b'\n')
            .filter(|l| !l.is_empty())
            .map(parse_pull_line)
            .collect()
    }

    fn events(text: &[u8]) -> Vec<PullEvent> {
        pull_lines(text).into_iter().map(Result::unwrap).collect()
    }

    #[test]
    fn telechargement_deja_installe() {
        let events = events(fixture!("pull-installed.ndjson"));
        let statuses: Vec<&str> = events.iter().map(|e| e.status.as_str()).collect();
        assert_eq!(statuses.len(), 9);
        assert_eq!(statuses[0], "pulling manifest");
        assert_eq!(
            statuses[5..],
            [
                "verifying sha256 digest",
                "writing manifest",
                "removing unused layers",
                "success"
            ]
        );
        // Couches déjà présentes : une seule ligne chacune, complète.
        let layers: Vec<_> = events.iter().filter_map(PullEvent::layer).collect();
        assert_eq!(layers.len(), 4);
        assert_eq!(
            layers[0],
            (
                "sha256:b139949c5bd74937ad8ed8c8cf3d9ffb1e99c866c823204dc42c0d91fa181897",
                1_107_409_472,
                Some(1_107_409_472)
            )
        );
        assert!(!events[0].is_verifying() && events[0].layer().is_none());
        assert!(!events[4].is_verifying());
        assert!(events[5..].iter().all(PullEvent::is_verifying));
        assert!(events[8].is_success() && !events[7].is_success());
    }

    #[test]
    fn telechargement_repris() {
        let events = events(fixture!("pull-resume.ndjson"));
        let layers: Vec<_> = events.iter().filter_map(PullEvent::layer).collect();
        // Reprise après annulation : 39,5 Mo déjà reçus.
        assert_eq!(layers[0].2, Some(39_538_779));
        let mut digests: Vec<&str> = layers.iter().map(|l| l.0).collect();
        digests.dedup();
        assert_eq!(digests.len(), 3, "modèle, projecteur mmproj, paramètres");
        // Le projecteur apparaît sans `completed`.
        let projector: Vec<_> = layers.iter().filter(|l| l.0 == digests[1]).collect();
        assert_eq!(projector[0].1, 103_769_856);
        assert_eq!(projector[0].2, None);
        // La dernière ligne du modèle n'atteint pas `total`.
        let model = layers.iter().rfind(|l| l.0 == digests[0]).unwrap();
        assert!(model.2.unwrap() < model.1);
        assert!(events.last().unwrap().is_success());
    }

    #[test]
    fn erreurs_de_telechargement() {
        let error = |text: &[u8]| {
            let lines = pull_lines(text);
            assert_eq!(lines.len(), 2);
            assert_eq!(lines[0].as_ref().unwrap().status, "pulling manifest");
            match &lines[1] {
                Err(OllamaError::Interrupted(m)) => m.clone(),
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(
            error(fixture!("pull-error-quant.ndjson")),
            r#"pull model manifest: 400: {"error":"The specified tag is not a valid quantization scheme. Please use another tag or \"latest\""}"#
        );
        // Dépôt protégé ou inexistant depuis `hf.co/` : Ollama ne suit pas l'authentification.
        assert_eq!(
            error(fixture!("pull-error-gated.ndjson")),
            r#"pull model manifest: realm host "huggingface.co" does not match original host "hf.co""#
        );
        // Depuis `huggingface.co/`, sans la clé d'Ollama enregistrée sur Hugging Face.
        assert_eq!(
            error(fixture!("pull-error-gated-key.ndjson")),
            r#"pull model manifest: 401: {"error":"Invalid username or password."}"#
        );
        assert!(
            error(fixture!("pull-error-sharded.ndjson"))
                .contains("This repository only contains sharded GGUF files")
        );
        // Échecs reconnus, pour un message qui dit quoi faire.
        let failure = |text: &[u8]| pull_failure(&error(text));
        assert_eq!(
            failure(fixture!("pull-error-quant.ndjson")),
            Some(PullFailure::UnknownQuant)
        );
        assert_eq!(
            failure(fixture!("pull-error-gated.ndjson")),
            Some(PullFailure::Unauthorized)
        );
        assert_eq!(
            failure(fixture!("pull-error-gated-key.ndjson")),
            Some(PullFailure::Unauthorized)
        );
        assert_eq!(
            failure(fixture!("pull-error-sharded.ndjson")),
            Some(PullFailure::Sharded)
        );
        let split: serde_json::Value =
            serde_json::from_slice(fixture!("create-error-split.json")).unwrap();
        assert_eq!(
            pull_failure(split["error"].as_str().unwrap()),
            Some(PullFailure::SplitPart)
        );
        assert_eq!(
            pull_failure("pull model manifest: file does not exist"),
            None
        );
        assert!(matches!(
            parse_pull_line(b"<html>"),
            Err(OllamaError::Invalid(m)) if m.starts_with("/api/pull")
        ));
        let req = serde_json::to_value(PullRequest {
            model: "hf.co/a/b:Q4_K_M",
            stream: true,
        })
        .unwrap();
        assert_eq!(
            req,
            serde_json::json!({ "model": "hf.co/a/b:Q4_K_M", "stream": true })
        );
    }

    #[test]
    fn creation() {
        parse_create(fixture!("create.json")).unwrap();
        assert!(matches!(
            parse_create(br#"{"status":"writing manifest"}"#),
            Err(OllamaError::Invalid(_))
        ));
        let req = CreateRequest {
            model: "gram/qwen3-1.7b:q4_k_m",
            from: "hf.co/unsloth/Qwen3-1.7B-GGUF:Q4_K_M",
            parameters: CreateParameters { num_ctx: 16384 },
            stream: false,
        };
        assert_eq!(
            serde_json::to_value(&req).unwrap(),
            serde_json::json!({
                "model": "gram/qwen3-1.7b:q4_k_m",
                "from": "hf.co/unsloth/Qwen3-1.7B-GGUF:Q4_K_M",
                "parameters": { "num_ctx": 16384 },
                "stream": false,
            })
        );
    }

    #[test]
    fn capacites_et_contexte() {
        use Capability::*;
        // Base : des paramètres, mais pas de contexte réglé.
        let base = parse_show(fixture!("show-base.json")).unwrap();
        assert_eq!(base.capabilities, [Tools, Thinking, Completion]);
        assert_eq!(base.num_ctx, None);
        // Modèle dérivé : paramètres de la base, plus `num_ctx`.
        let alias = parse_show(fixture!("show-alias.json")).unwrap();
        assert_eq!(alias.num_ctx, Some(16384));
        assert_eq!(alias.capabilities, base.capabilities);
        // Vision (projecteur mmproj récupéré depuis hf.co), sans aucun paramètre.
        let vision = parse_show(fixture!("show-vision.json")).unwrap();
        assert_eq!(vision.capabilities, [Completion, Vision]);
        assert!(vision.has(&Vision) && !base.has(&Vision));
        assert_eq!(vision.num_ctx, None);

        // Capacité inconnue : gardée.
        let other = parse_show(br#"{"capabilities":["completion","audio"]}"#).unwrap();
        assert_eq!(other.capabilities, [Completion, Other("audio".into())]);
        assert_eq!(
            serde_json::to_value(&other.capabilities).unwrap(),
            serde_json::json!(["completion", "audio"])
        );
        assert_eq!(parse_show(b"{}").unwrap(), ModelShow::default());
        assert_eq!(num_ctx("num_ctx  abc\nnum_ctxx 5"), None);
        assert_eq!(num_ctx("top_k 20\r\nnum_ctx 4096\r\n"), Some(4096));
    }

    #[test]
    fn noms_equivalents() {
        assert!(same_model(
            "hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M",
            "hf.co/qwen/qwen3-8b-gguf:q4_k_m"
        ));
        assert!(same_model("qwen3", "Qwen3:latest"));
        assert!(!same_model(
            "gram/qwen3-8b:q4_k_m",
            "hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M"
        ));
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
