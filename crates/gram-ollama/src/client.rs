//! Requêtes HTTP vers l'API locale d'Ollama.

use crate::addr::ollama_addr_from_env;
use crate::api::{self, GenerateRequest, GenerateResponse, PsEntry};
use crate::error::{OllamaError, Result};
use gram_core::OllamaModel;
use std::net::SocketAddr;
use std::time::Duration;

/// L'API est locale : elle accepte la connexion tout de suite si elle tourne. Sous Windows, une
/// connexion refusée met environ 2 s à échouer (le SYN est retenté) : inutile d'attendre plus.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
/// Listes (`/api/tags`, `/api/ps`, `/api/version`).
const LIST_TIMEOUT: Duration = Duration::from_secs(10);
/// Génération : chargement d'un gros modèle depuis un disque lent, puis génération.
const GENERATE_TIMEOUT: Duration = Duration::from_secs(600);

/// Client de l'API d'Ollama sur cette machine.
#[derive(Debug, Clone)]
pub struct OllamaClient {
    base: String,
    http: reqwest::Client,
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
        })
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

    /// Modèles chargés en mémoire (`GET /api/ps`).
    pub async fn ps(&self) -> Result<Vec<PsEntry>> {
        let body = self.get("/api/ps").await?;
        api::parse_ps(&body)
    }

    /// Génération sans flux (`POST /api/generate`) : charge le modèle si besoin.
    pub async fn generate(&self, req: &GenerateRequest<'_>) -> Result<GenerateResponse> {
        let json = serde_json::to_vec(req).map_err(|e| OllamaError::Client(e.to_string()))?;
        let rb = self
            .http
            .post(self.url("/api/generate"))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(json)
            .timeout(GENERATE_TIMEOUT);
        let body = self.send(rb, "génération").await?;
        api::parse_generate(&body)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeOllama;

    #[tokio::test]
    async fn listes_et_erreurs() {
        let fake = FakeOllama::standard();
        let client = OllamaClient::new(format!("{}/", fake.url())).unwrap();
        assert_eq!(client.base_url(), fake.url());
        assert_eq!(client.version().await.unwrap(), "0.13.1");
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
