//! Requêtes HTTP vers l'API locale d'Ollama.

use crate::addr::ollama_addr_from_env;
use crate::api::{
    self, ChatChunk, ChatDone, ChatRequest, CreateParameters, CreateRequest, GenerateRequest,
    GenerateResponse, ModelRequest, ModelShow, PsEntry, PullEvent, PullRequest, TaggedModel,
};
use crate::error::{OllamaError, Result};
use crate::ndjson::Lines;
use gram_core::OllamaModel;
use serde::Serialize;
use std::net::SocketAddr;
use std::time::Duration;

/// L'API est locale : elle accepte la connexion tout de suite si elle tourne. Sous Windows, une
/// connexion refusée met environ 2 s à échouer (le SYN est retenté) : inutile d'attendre plus.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
/// Listes (`/api/tags`, `/api/ps`, `/api/version`).
const LIST_TIMEOUT: Duration = Duration::from_secs(10);
/// Génération : chargement d'un gros modèle depuis un disque lent, puis génération.
const GENERATE_TIMEOUT: Duration = Duration::from_secs(600);
/// `/api/show` lit l'en-tête du GGUF ; `/api/delete` efface des fichiers de plusieurs Go.
const MODEL_TIMEOUT: Duration = Duration::from_secs(60);
/// Modèle dérivé : un petit manifeste, écrit en moins d'une seconde.
const CREATE_TIMEOUT: Duration = Duration::from_secs(120);
/// Délai maximal entre deux morceaux d'un flux. Pas de délai total : télécharger 40 Go peut
/// durer une heure. Ollama envoie une ligne toutes les 60 ms pendant un téléchargement.
pub const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// Pendant la vérification, Ollama relit les couches téléchargées sans rien envoyer (650 Mo/s
/// mesurés sur un SSD NVMe) : le délai s'allonge d'une seconde par tranche de 50 Mo
/// annoncés, pour un disque dur lent.
const VERIFY_BYTES_PER_S: u64 = 50_000_000;

/// Client de l'API d'Ollama sur cette machine.
#[derive(Debug, Clone)]
pub struct OllamaClient {
    base: String,
    http: reqwest::Client,
    idle_timeout: Duration,
}

impl OllamaClient {
    /// `base_url` : `http://127.0.0.1:11434` (sans `/` final).
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("GRAM/", env!("CARGO_PKG_VERSION")))
            // Requêtes locales : jamais de proxy, même configuré dans Windows.
            .no_proxy()
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(|e| OllamaError::Client(e.to_string()))?;
        Ok(Self {
            base: base_url.into().trim_end_matches('/').to_string(),
            http,
            idle_timeout: STREAM_IDLE_TIMEOUT,
        })
    }

    /// Autre délai maximal entre deux morceaux d'un flux ([`STREAM_IDLE_TIMEOUT`] par défaut).
    pub fn with_idle_timeout(mut self, timeout: Duration) -> Self {
        self.idle_timeout = timeout;
        self
    }

    pub fn for_addr(addr: SocketAddr) -> Result<Self> {
        Self::new(format!("http://{addr}"))
    }

    /// D'après `OLLAMA_HOST` (`127.0.0.1:11434` par défaut) ; `None` si l'adresse n'est pas
    /// utilisable (HTTPS, hôte inconnu).
    pub fn from_env() -> Option<Self> {
        ollama_addr_from_env().and_then(|a| Self::for_addr(a).ok())
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    pub async fn version(&self) -> Result<String> {
        let body = self.get("/api/version").await?;
        api::parse_version(&body)
    }

    /// Modèles installés (`GET /api/tags`).
    pub async fn tags(&self) -> Result<Vec<OllamaModel>> {
        let body = self.get("/api/tags").await?;
        api::parse_tags(&body)
    }

    /// Modèles installés, avec l'empreinte de chacun (`GET /api/tags`).
    pub async fn tagged(&self) -> Result<Vec<TaggedModel>> {
        let body = self.get("/api/tags").await?;
        api::parse_tagged(&body)
    }

    /// Modèles chargés en mémoire (`GET /api/ps`).
    pub async fn ps(&self) -> Result<Vec<PsEntry>> {
        let body = self.get("/api/ps").await?;
        api::parse_ps(&body)
    }

    /// Génération sans flux (`POST /api/generate`) : charge le modèle si besoin.
    pub async fn generate(&self, req: &GenerateRequest<'_>) -> Result<GenerateResponse> {
        let rb = self.post("/api/generate", req)?.timeout(GENERATE_TIMEOUT);
        let body = self.send(rb, "génération").await?;
        api::parse_generate(&body)
    }

    /// Téléchargement (`POST /api/pull`), suivi ligne par ligne jusqu'à « success ».
    ///
    /// Abandonner le futur ferme la connexion, ce qui arrête le téléchargement côté Ollama ;
    /// le suivant reprend là où il s'était arrêté (vérifié avec Ollama 0.40.16).
    pub async fn pull(&self, name: &str, on_event: impl FnMut(&PullEvent)) -> Result<()> {
        let request = PullRequest {
            model: name,
            stream: true,
        };
        let mut reader = PullReader {
            state: PullState::default(),
            on_event,
        };
        self.stream("/api/pull", &request, "téléchargement", &mut reader)
            .await?;
        if reader.state.success {
            Ok(())
        } else {
            Err(OllamaError::Invalid(
                "le téléchargement s'est arrêté avant la fin (pas de « success »)".into(),
            ))
        }
    }

    /// Discussion en flux (`POST /api/chat`) : `on_chunk` reçoit chaque morceau de la réponse
    /// ou de la réflexion ; renvoie le bilan de la dernière ligne. Le modèle est chargé s'il ne
    /// l'est pas : la première ligne peut tarder (chargement, puis lecture de la conversation).
    ///
    /// Abandonner le futur ferme la connexion, ce qui arrête la génération côté Ollama
    /// (vérifié avec Ollama 0.40.16 : « cancel task » dans son journal, requête suivante servie
    /// aussitôt).
    pub async fn chat(
        &self,
        request: &ChatRequest<'_>,
        on_chunk: impl FnMut(&ChatChunk),
    ) -> Result<ChatDone> {
        let mut reader = ChatReader {
            on_chunk,
            lines: 0,
            done: None,
        };
        self.stream("/api/chat", request, "réponse", &mut reader)
            .await?;
        reader.done.ok_or_else(|| {
            OllamaError::Invalid("la réponse s'est arrêtée avant la fin (pas de bilan)".into())
        })
    }

    /// Envoie une requête dont la réponse est un flux NDJSON, et passe chaque ligne à `reader`.
    /// Pas de délai total : seulement un délai entre deux morceaux, fixé par `reader`.
    async fn stream(
        &self,
        path: &str,
        body: &impl Serialize,
        what: &'static str,
        reader: &mut impl LineReader,
    ) -> Result<()> {
        // Les en-têtes peuvent attendre le chargement du modèle (`/api/chat`).
        let send = self.post(path, body)?.send();
        let mut resp = tokio::time::timeout(GENERATE_TIMEOUT, send)
            .await
            .map_err(|_| OllamaError::Timeout(what))?
            .map_err(|e| self.error(e, what))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.bytes().await.map_err(|e| self.error(e, what))?;
            return Err(OllamaError::Api {
                status: status.as_u16(),
                message: api::error_message(&body),
            });
        }
        let mut lines = Lines::default();
        loop {
            let idle = reader.idle_timeout(self.idle_timeout);
            let chunk = tokio::time::timeout(idle, resp.chunk())
                .await
                .map_err(|_| OllamaError::Timeout(what))?
                .map_err(|e| self.stream_error(e, what))?;
            let Some(chunk) = chunk else { break };
            for line in lines.push(&chunk)? {
                reader.line(&line)?;
            }
        }
        if let Some(line) = lines.finish() {
            reader.line(&line)?;
        }
        Ok(())
    }

    /// Modèle dérivé `alias` de `from`, au contexte `num_ctx` (`POST /api/create`). Un modèle
    /// du même nom est remplacé. `from` doit être installé : sinon Ollama le télécharge sans
    /// progression ni moyen d'annuler.
    pub async fn create(&self, alias: &str, from: &str, num_ctx: u32) -> Result<()> {
        let request = CreateRequest {
            model: alias,
            from,
            parameters: CreateParameters { num_ctx },
            stream: false,
        };
        let rb = self.post("/api/create", &request)?.timeout(CREATE_TIMEOUT);
        let body = self.send(rb, "création du modèle").await?;
        api::parse_create(&body)
    }

    /// Supprime un modèle (`DELETE /api/delete`) ; `false` s'il était déjà absent. Ollama
    /// efface les couches que plus aucun modèle n'utilise.
    pub async fn delete(&self, name: &str) -> Result<bool> {
        let rb = self.http.delete(self.url("/api/delete"));
        let rb = with_json(rb, &ModelRequest { model: name })?.timeout(MODEL_TIMEOUT);
        match self.send(rb, "suppression").await {
            Ok(_) => Ok(true),
            Err(OllamaError::Api { status: 404, .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Capacités et contexte réglé d'un modèle installé (`POST /api/show`).
    pub async fn show(&self, name: &str) -> Result<ModelShow> {
        let rb = self
            .post("/api/show", &ModelRequest { model: name })?
            .timeout(MODEL_TIMEOUT);
        let body = self.send(rb, "/api/show").await?;
        api::parse_show(&body)
    }

    fn post(&self, path: &str, body: &impl Serialize) -> Result<reqwest::RequestBuilder> {
        with_json(self.http.post(self.url(path)), body)
    }

    async fn get(&self, path: &'static str) -> Result<Vec<u8>> {
        let rb = self.http.get(self.url(path)).timeout(LIST_TIMEOUT);
        self.send(rb, path).await
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    async fn send(&self, rb: reqwest::RequestBuilder, what: &'static str) -> Result<Vec<u8>> {
        let resp = rb.send().await.map_err(|e| self.error(e, what))?;
        let status = resp.status();
        let body = resp
            .bytes()
            .await
            .map_err(|e| self.error(e, what))?
            .to_vec();
        if status.is_success() {
            Ok(body)
        } else {
            Err(OllamaError::Api {
                status: status.as_u16(),
                message: api::error_message(&body),
            })
        }
    }

    fn error(&self, e: reqwest::Error, what: &'static str) -> OllamaError {
        // Un délai de connexion dépassé est aussi une erreur de connexion : Ollama ne tourne pas.
        if e.is_connect() {
            OllamaError::Unreachable(self.base.clone())
        } else if e.is_timeout() {
            OllamaError::Timeout(what)
        } else {
            OllamaError::Client(e.to_string())
        }
    }

    /// Erreur en lisant un flux déjà commencé : la connexion s'est coupée (Ollama arrêté).
    fn stream_error(&self, e: reqwest::Error, what: &'static str) -> OllamaError {
        if e.is_body() || e.is_decode() {
            OllamaError::Disconnected(what)
        } else {
            self.error(e, what)
        }
    }
}

fn with_json(
    rb: reqwest::RequestBuilder,
    body: &impl Serialize,
) -> Result<reqwest::RequestBuilder> {
    let json = serde_json::to_vec(body).map_err(|e| OllamaError::Client(e.to_string()))?;
    Ok(rb
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(json))
}

/// Lecteur d'un flux NDJSON : traite chaque ligne et fixe le délai d'attente du morceau
/// suivant.
trait LineReader {
    fn line(&mut self, line: &[u8]) -> Result<()>;
    /// Délai maximal avant le prochain morceau ; `base` : celui du client.
    fn idle_timeout(&self, base: Duration) -> Duration;
}

/// Suivi d'un flux de téléchargement : délai de la vérification, et fin.
#[derive(Debug, Default)]
struct PullState {
    /// Taille de chaque couche annoncée.
    layers: Vec<(String, u64)>,
    verifying: bool,
    success: bool,
}

impl PullState {
    fn event(&mut self, event: &PullEvent) {
        if let Some((digest, total, _)) = event.layer() {
            match self.layers.iter_mut().find(|(d, _)| d == digest) {
                Some(layer) => layer.1 = total,
                None => self.layers.push((digest.to_string(), total)),
            }
        }
        self.verifying |= event.is_verifying();
        self.success |= event.is_success();
    }

    /// Délai supplémentaire pendant la vérification, selon la taille des couches.
    fn verify_time(&self) -> Duration {
        if self.verifying {
            let total: u64 = self.layers.iter().map(|(_, t)| t).sum();
            Duration::from_secs(total / VERIFY_BYTES_PER_S)
        } else {
            Duration::ZERO
        }
    }
}

struct PullReader<F> {
    state: PullState,
    on_event: F,
}

impl<F: FnMut(&PullEvent)> LineReader for PullReader<F> {
    fn line(&mut self, line: &[u8]) -> Result<()> {
        let event = api::parse_pull_line(line)?;
        self.state.event(&event);
        (self.on_event)(&event);
        Ok(())
    }

    fn idle_timeout(&self, base: Duration) -> Duration {
        base + self.state.verify_time()
    }
}

/// Suivi d'un flux de discussion : bilan de la dernière ligne.
struct ChatReader<F> {
    on_chunk: F,
    lines: usize,
    done: Option<ChatDone>,
}

impl<F: FnMut(&ChatChunk)> LineReader for ChatReader<F> {
    fn line(&mut self, line: &[u8]) -> Result<()> {
        let chunk = api::parse_chat_line(line)?;
        self.lines += 1;
        (self.on_chunk)(&chunk);
        if let Some(done) = chunk.done {
            self.done = Some(done);
        }
        Ok(())
    }

    /// Avant la première ligne, le modèle lit toute la conversation : plusieurs minutes pour un
    /// gros modèle sur le processeur. Ensuite, un token arrive au moins toutes les secondes.
    fn idle_timeout(&self, base: Duration) -> Duration {
        if self.lines == 0 {
            GENERATE_TIMEOUT
        } else {
            base
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{
        CHAT_DEFAULT, CHAT_NO_THINK_CONTENT, FakeOllama, NOT_FOUND, PULL_ERROR_QUANT,
        PULL_INSTALLED, PULL_RESUME, Reply, VERSION, standard_response,
    };

    #[tokio::test]
    async fn listes_et_erreurs() {
        let fake = FakeOllama::standard();
        let client = OllamaClient::new(format!("{}/", fake.url())).unwrap();
        assert_eq!(client.base_url(), fake.url());
        assert_eq!(client.version().await.unwrap(), VERSION);
        assert_eq!(client.tags().await.unwrap().len(), 3);
        assert_eq!(client.ps().await.unwrap().len(), 1);

        let fake = FakeOllama::start(|_| (404, r#"{"error":"page introuvable"}"#.into()));
        let client = OllamaClient::new(fake.url()).unwrap();
        match client.tags().await {
            Err(OllamaError::Api { status, message }) => {
                assert_eq!((status, message.as_str()), (404, "page introuvable"));
            }
            other => panic!("{other:?}"),
        }
    }

    /// Lignes du flux reçues par `pull`, ou son erreur.
    async fn pull(client: &OllamaClient) -> Result<Vec<PullEvent>> {
        let mut events = Vec::new();
        client
            .pull("hf.co/a/b:Q8_0", |e| events.push(e.clone()))
            .await
            .map(|()| events)
    }

    fn fixture_events(text: &str) -> Vec<PullEvent> {
        text.lines()
            .map(|l| api::parse_pull_line(l.as_bytes()).unwrap())
            .collect()
    }

    #[tokio::test]
    async fn flux_en_morceaux_quelconques() {
        // Morceaux de 7 octets : lignes coupées n'importe où (les caractères coupés sont testés
        // dans `ndjson`).
        let fake = FakeOllama::streaming(|_| {
            let bytes = PULL_RESUME.as_bytes();
            bytes.chunks(7).fold(Reply::chunked(200), |reply, chunk| {
                reply.data(String::from_utf8(chunk.to_vec()).unwrap())
            })
        });
        let client = OllamaClient::new(fake.url()).unwrap();
        assert_eq!(pull(&client).await.unwrap(), fixture_events(PULL_RESUME));
        assert_eq!(fake.requests()[0].json()["stream"], true);

        // Réponse d'un bloc, sans fin de ligne finale.
        let fake = FakeOllama::start(|_| (200, PULL_INSTALLED.trim_end().into()));
        let client = OllamaClient::new(fake.url()).unwrap();
        assert_eq!(pull(&client).await.unwrap().len(), 9);
    }

    #[tokio::test]
    async fn flux_interrompu() {
        let client = |fake: &FakeOllama| OllamaClient::new(fake.url()).unwrap();
        // Erreur au milieu du flux, après le statut 200.
        let fake = FakeOllama::streaming(|_| Reply::ndjson(PULL_ERROR_QUANT));
        let err = pull(&client(&fake)).await.unwrap_err();
        assert!(
            matches!(&err, OllamaError::Interrupted(m) if m.starts_with("pull model manifest: 400"))
        );
        // Flux terminé sans « success ».
        let head: String = PULL_RESUME
            .lines()
            .take(5)
            .map(|l| format!("{l}\n"))
            .collect();
        let fake = FakeOllama::streaming(move |_| Reply::ndjson(&head));
        let err = pull(&client(&fake)).await.unwrap_err();
        assert!(matches!(err, OllamaError::Invalid(_)), "{err:?}");
        // Sans flux, Ollama répond une erreur HTTP 500.
        let fake = FakeOllama::start(|_| (500, r#"{"error":"pull model manifest: 400"}"#.into()));
        let err = pull(&client(&fake)).await.unwrap_err();
        assert!(
            matches!(&err, OllamaError::Api { status: 500, message } if message == "pull model manifest: 400"),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn delai_entre_deux_morceaux() {
        let head: String = PULL_RESUME
            .lines()
            .take(3)
            .map(|l| format!("{l}\n"))
            .collect();
        let fake = FakeOllama::streaming(move |_| {
            Reply::ndjson(&head)
                .pause(Duration::from_secs(5))
                .lines(PULL_RESUME)
        });
        let client = OllamaClient::new(fake.url())
            .unwrap()
            .with_idle_timeout(Duration::from_millis(300));
        let start = std::time::Instant::now();
        let err = pull(&client).await.unwrap_err();
        assert!(matches!(err, OllamaError::Timeout(_)), "{err:?}");
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "{:?}",
            start.elapsed()
        );
        assert_eq!(
            err.to_string(),
            "Ollama n'a pas répondu à temps (téléchargement)"
        );
    }

    #[tokio::test]
    async fn verification_sans_nouvelles() {
        // Ollama se tait pendant la vérification : 280 Mo annoncés allongent le délai de 5 s.
        let (download, end) =
            PULL_RESUME.split_at(PULL_RESUME.find("{\"status\":\"verifying").unwrap());
        let (verifying, end) = end.split_at(end.find('\n').unwrap() + 1);
        let (download, verifying, end) =
            (download.to_string(), verifying.to_string(), end.to_string());
        let fake = FakeOllama::streaming(move |_| {
            Reply::ndjson(&download)
                .lines(&verifying)
                .pause(Duration::from_millis(800))
                .lines(&end)
        });
        let client = OllamaClient::new(fake.url())
            .unwrap()
            .with_idle_timeout(Duration::from_millis(300));
        let events = pull(&client).await.unwrap();
        assert!(events.last().unwrap().is_success());
    }

    #[tokio::test]
    async fn creation_suppression_et_capacites() {
        let fake = FakeOllama::start(|r| match (r.path.as_str(), r.json()["model"].as_str()) {
            ("/api/delete", Some("absent")) => (404, NOT_FOUND.into()),
            ("/api/delete", Some("verrouille")) => {
                (500, r#"{"error":"fichier verrouillé"}"#.into())
            }
            _ => standard_response(r),
        });
        let client = OllamaClient::new(fake.url()).unwrap();
        client
            .create(
                "gram/qwen3-1.7b:q4_k_m",
                "hf.co/unsloth/Qwen3-1.7B-GGUF:Q4_K_M",
                16384,
            )
            .await
            .unwrap();
        let show = client.show("gram/qwen3-1.7b:q4_k_m").await.unwrap();
        assert_eq!(show.num_ctx, Some(16384));
        assert!(client.delete("gram/qwen3-1.7b:q4_k_m").await.unwrap());
        assert!(!client.delete("absent").await.unwrap(), "déjà absent");
        assert!(matches!(
            client.delete("verrouille").await,
            Err(OllamaError::Api { status: 500, .. })
        ));

        let reqs = fake.requests();
        let calls: Vec<(&str, &str)> = reqs
            .iter()
            .map(|r| (r.method.as_str(), r.path.as_str()))
            .collect();
        assert_eq!(
            calls[..3],
            [
                ("POST", "/api/create"),
                ("POST", "/api/show"),
                ("DELETE", "/api/delete")
            ]
        );
        assert_eq!(
            reqs[0].json()["from"],
            "hf.co/unsloth/Qwen3-1.7B-GGUF:Q4_K_M"
        );
        assert_eq!(
            reqs[2].json(),
            serde_json::json!({ "model": "gram/qwen3-1.7b:q4_k_m" })
        );
    }

    fn chat_request(messages: &[api::ChatMessage]) -> ChatRequest<'_> {
        ChatRequest {
            model: "gram/qwen3-1.7b:q4_k_m",
            messages,
            stream: true,
            think: None,
            keep_alive: None,
            options: api::ChatOptions::default(),
        }
    }

    /// Morceaux reçus par `chat` (réponse, réflexion), puis son bilan ou son erreur.
    async fn chat(client: &OllamaClient) -> (String, String, Result<ChatDone>) {
        let messages = [api::ChatMessage::new(api::ChatRole::User, "Bonjour")];
        let (mut content, mut thinking) = (String::new(), String::new());
        let done = client
            .chat(&chat_request(&messages), |c| {
                content.push_str(&c.content);
                thinking.push_str(&c.thinking);
            })
            .await;
        (content, thinking, done)
    }

    #[tokio::test]
    async fn discussion_en_flux() {
        // Morceaux de 5 octets : lignes et caractères coupés n'importe où.
        let fake = FakeOllama::streaming(|_| {
            let bytes = CHAT_DEFAULT.as_bytes();
            bytes.chunks(5).fold(Reply::chunked(200), |reply, chunk| {
                reply.data(String::from_utf8_lossy(chunk).into_owned())
            })
        });
        let client = OllamaClient::new(fake.url()).unwrap();
        let (_, thinking, done) = chat(&client).await;
        let done = done.unwrap();
        assert!(thinking.starts_with("Okay, the user said"));
        assert_eq!(
            (done.eval_count, done.done_reason.as_deref()),
            (104, Some("stop"))
        );
        let req = &fake.requests()[0];
        assert_eq!(
            (req.method.as_str(), req.path.as_str()),
            ("POST", "/api/chat")
        );
        assert_eq!(
            req.json(),
            serde_json::json!({
                "model": "gram/qwen3-1.7b:q4_k_m",
                "messages": [{ "role": "user", "content": "Bonjour" }],
                "stream": true,
            })
        );

        // Réponse enregistrée, ligne par ligne.
        let fake = FakeOllama::standard();
        let client = OllamaClient::new(fake.url()).unwrap();
        let (content, thinking, done) = chat(&client).await;
        assert_eq!(
            (content.as_str(), thinking.as_str()),
            (CHAT_NO_THINK_CONTENT, "")
        );
        assert_eq!(done.unwrap().eval_count, 11);
    }

    #[tokio::test]
    async fn discussion_interrompue() {
        let head: String = CHAT_DEFAULT
            .lines()
            .take(3)
            .map(|l| format!("{l}\n"))
            .collect();
        let client = |fake: &FakeOllama| OllamaClient::new(fake.url()).unwrap();
        // Erreur au milieu du flux : les morceaux d'avant sont arrivés.
        let h = head.clone();
        let fake = FakeOllama::streaming(move |_| {
            Reply::ndjson(&h).lines(r#"{"error":"llama runner process has terminated"}"#)
        });
        let (_, thinking, done) = chat(&client(&fake)).await;
        assert_eq!(thinking, "Okay, the");
        assert!(
            matches!(&done, Err(OllamaError::Interrupted(m)) if m.contains("terminated")),
            "{done:?}"
        );
        // Ollama arrêté au milieu : connexion fermée sans fin de réponse.
        let h = head.clone();
        let fake = FakeOllama::streaming(move |_| Reply::ndjson(&h).abort());
        let (_, thinking, done) = chat(&client(&fake)).await;
        assert_eq!(thinking, "Okay, the");
        let err = done.unwrap_err();
        assert!(matches!(err, OllamaError::Disconnected(_)), "{err:?}");
        assert_eq!(
            err.to_string(),
            "connexion avec Ollama coupée (réponse) : le relancer, puis réessayer"
        );
        // Flux terminé sans bilan.
        let fake = FakeOllama::streaming(move |_| Reply::ndjson(&head));
        let (_, _, done) = chat(&client(&fake)).await;
        assert!(matches!(done, Err(OllamaError::Invalid(_))), "{done:?}");
        // Modèle absent : HTTP 404, avant tout flux.
        let fake = FakeOllama::start(|_| {
            (
                404,
                r#"{"error":"model 'gram/absent:q4_k_m' not found"}"#.into(),
            )
        });
        let (_, _, done) = chat(&client(&fake)).await;
        assert!(
            matches!(&done, Err(OllamaError::Api { status: 404, message }) if api::is_model_not_found(404, message)),
            "{done:?}"
        );
    }

    #[tokio::test]
    async fn discussion_delais_et_annulation() {
        let lines: Vec<String> = CHAT_DEFAULT.lines().map(|l| format!("{l}\n")).collect();
        let (first, rest) = (lines[..2].concat(), lines[2..].concat());
        // La première ligne peut tarder (chargement du modèle) : le délai entre deux morceaux
        // ne vaut qu'ensuite.
        let (f, r) = (first.clone(), rest.clone());
        let fake = FakeOllama::streaming(move |_| {
            Reply::chunked(200)
                .pause(Duration::from_millis(600))
                .lines(&f)
                .lines(&r)
        });
        let client = OllamaClient::new(fake.url())
            .unwrap()
            .with_idle_timeout(Duration::from_millis(300));
        assert!(chat(&client).await.2.is_ok());
        // Silence au milieu de la réponse.
        let (f, r) = (first.clone(), rest.clone());
        let fake = FakeOllama::streaming(move |_| {
            Reply::ndjson(&f).pause(Duration::from_secs(5)).lines(&r)
        });
        let client = OllamaClient::new(fake.url())
            .unwrap()
            .with_idle_timeout(Duration::from_millis(300));
        let err = chat(&client).await.2.unwrap_err();
        assert!(matches!(err, OllamaError::Timeout("réponse")), "{err:?}");

        // Annuler = abandonner le futur : il rend la main tout de suite, avec les morceaux reçus.
        let fake = FakeOllama::streaming(move |_| {
            Reply::ndjson(&first)
                .pause(Duration::from_secs(5))
                .lines(&rest)
        });
        let client = OllamaClient::new(fake.url()).unwrap();
        let messages = [api::ChatMessage::new(api::ChatRole::User, "Bonjour")];
        let request = chat_request(&messages);
        let mut thinking = String::new();
        let start = std::time::Instant::now();
        let chat = client.chat(&request, |c| thinking.push_str(&c.thinking));
        let cancelled = tokio::time::timeout(Duration::from_millis(400), chat).await;
        assert!(cancelled.is_err(), "annulé");
        assert!(start.elapsed() < Duration::from_secs(2));
        assert_eq!(thinking, "Okay,");
    }

    #[tokio::test]
    async fn ollama_absent() {
        let addr = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let client = OllamaClient::for_addr(addr).unwrap();
        let err = client.tags().await.unwrap_err();
        assert!(matches!(err, OllamaError::Unreachable(_)), "{err:?}");
        assert_eq!(
            err.to_string(),
            format!("Ollama ne répond pas sur http://{addr} : le lancer, puis réessayer")
        );
    }
}
