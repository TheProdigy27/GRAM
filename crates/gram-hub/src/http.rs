//! Accès HTTP à Hugging Face : transport, limites de débit (§7.2), 429 et pagination.
//!
//! [`Transport`] isole l'envoi des requêtes : [`ReqwestTransport`] en production, des fixtures
//! dans les tests (aucun appel réseau). [`Hub`] ajoute par-dessus le limiteur, les nouvelles
//! tentatives, les compteurs d'appels et le suivi du quota.

use crate::error::{HubError, Result};
use gram_core::{RateLimitInfo, SyncProgress};
use std::fmt;
use std::future::Future;
use std::ops::Range;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{Semaphore, SemaphorePermit};
use tokio::time::Instant;

/// Token Hugging Face (optionnel : il augmente les quotas). Jamais affiché ni sérialisé :
/// `Debug` le masque et il n'implémente ni `Display` ni `Serialize`.
#[derive(Clone)]
pub struct HfToken(String);

impl HfToken {
    pub fn new(token: impl Into<String>) -> Option<Self> {
        let t = token.into().trim().to_owned();
        (!t.is_empty()).then_some(Self(t))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for HfToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HfToken(***)")
    }
}

/// Catégorie de quota (§7.2) : l'API Hub est bien plus limitée que les resolvers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint {
    Api,
    Resolver,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub url: String,
    pub endpoint: Endpoint,
    /// Octets demandés (`Range`), fin exclue. Le corps lu ne dépasse jamais cette taille, même
    /// si le serveur ignore l'en-tête et renvoie le fichier entier.
    pub range: Option<Range<u64>>,
}

impl Request {
    pub fn api(url: String) -> Self {
        Self {
            url,
            endpoint: Endpoint::Api,
            range: None,
        }
    }

    pub fn resolver(url: String, range: Option<Range<u64>>) -> Self {
        Self {
            url,
            endpoint: Endpoint::Resolver,
            range,
        }
    }
}

/// En-tête `RateLimit: "api";r=489;t=189`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimit {
    pub category: String,
    pub remaining: u32,
    pub reset_s: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
    /// URL de la page suivante (en-tête `Link: <…>; rel="next"`).
    pub link_next: Option<String>,
    pub rate_limit: Option<RateLimit>,
    /// En-tête `Retry-After`, en secondes.
    pub retry_after_s: Option<u64>,
}

impl Response {
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            body: body.into(),
            link_next: None,
            rate_limit: None,
            retry_after_s: None,
        }
    }

    pub fn status(status: u16) -> Self {
        Self {
            status,
            ..Self::ok(Vec::new())
        }
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Échec avant toute réponse HTTP (connexion, DNS, délai dépassé…).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError(pub String);

pub trait Transport: Send + Sync {
    fn get(
        &self,
        req: &Request,
    ) -> impl Future<Output = std::result::Result<Response, TransportError>> + Send;
}

/// Corps maximal accepté pour une réponse de l'API (une page de 100 modèles pèse ~1 Mo).
const MAX_API_BODY: usize = 64 << 20;

/// Transport réel : reqwest + rustls, réponses compressées en gzip.
pub struct ReqwestTransport {
    client: reqwest::Client,
    token: Option<HfToken>,
}

impl ReqwestTransport {
    pub fn new(token: Option<HfToken>) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("GRAM/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| HubError::Network(e.to_string()))?;
        Ok(Self { client, token })
    }
}

impl Transport for ReqwestTransport {
    async fn get(&self, req: &Request) -> std::result::Result<Response, TransportError> {
        let mut rb = self.client.get(&req.url);
        // reqwest retire `Authorization` lors d'une redirection vers un autre hôte (CDN) :
        // le token n'est envoyé qu'à huggingface.co. `Range`, lui, est conservé.
        if let Some(token) = &self.token {
            rb = rb.bearer_auth(token.expose());
        }
        if let Some(r) = &req.range {
            rb = rb.header(
                reqwest::header::RANGE,
                format!("bytes={}-{}", r.start, r.end.saturating_sub(1)),
            );
        }
        let mut resp = rb.send().await.map_err(transport_error)?;
        let headers = resp.headers();
        let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
        let link_next = header("link").and_then(parse_link_next);
        let rate_limit = header("ratelimit").and_then(parse_rate_limit);
        let retry_after_s = header("retry-after").and_then(|v| v.trim().parse().ok());
        let status = resp.status().as_u16();

        let limit = match &req.range {
            Some(r) => usize::try_from(r.end - r.start).unwrap_or(usize::MAX),
            None => MAX_API_BODY,
        };
        let mut body = Vec::new();
        while body.len() < limit {
            match resp.chunk().await.map_err(transport_error)? {
                Some(chunk) => body.extend_from_slice(&chunk),
                None => break,
            }
        }
        body.truncate(limit);
        Ok(Response {
            status,
            body,
            link_next,
            rate_limit,
            retry_after_s,
        })
    }
}

fn transport_error(e: reqwest::Error) -> TransportError {
    // `without_url` : message court ; l'URL est déjà dans les logs de `Hub`.
    TransportError(e.without_url().to_string())
}

/// `RateLimit: "api";r=489;t=189` → catégorie, requêtes restantes, secondes avant remise à zéro.
pub fn parse_rate_limit(value: &str) -> Option<RateLimit> {
    let mut parts = value.split(';').map(str::trim);
    let category = parts.next()?.trim_matches('"').to_owned();
    let (mut remaining, mut reset_s) = (None, None);
    for p in parts {
        if let Some(v) = p.strip_prefix("r=") {
            remaining = v.parse().ok();
        } else if let Some(v) = p.strip_prefix("t=") {
            reset_s = v.parse().ok();
        }
    }
    Some(RateLimit {
        category,
        remaining: remaining?,
        reset_s: reset_s?,
    })
}

/// `Link: <https://…&cursor=…>; rel="next"` → URL de la page suivante.
pub fn parse_link_next(value: &str) -> Option<String> {
    value.split(',').find_map(|part| {
        let (url, params) = part.split_once(';')?;
        params
            .split(';')
            .any(|p| p.trim().replace(' ', "") == "rel=\"next\"")
            .then(|| {
                url.trim()
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .to_owned()
            })
    })
}

/// Réglages des nouvelles tentatives et du débit.
#[derive(Debug, Clone)]
pub struct Policy {
    /// Requêtes simultanées (§12 : 4).
    pub concurrency: usize,
    /// Écart minimal entre deux départs de requête (§12 : ~4 par seconde).
    pub spacing: Duration,
    /// Nouvelles tentatives après un 429 ou une erreur 5xx.
    pub max_retries: u32,
    /// Nouvelles tentatives après une erreur réseau (peu : une coupure se voit vite).
    pub max_network_retries: u32,
    /// Attente maximale acceptée après un 429 (une fenêtre de quota dure 5 minutes).
    pub max_wait: Duration,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            concurrency: 4,
            spacing: Duration::from_millis(250),
            max_retries: 3,
            max_network_retries: 1,
            max_wait: Duration::from_secs(310),
        }
    }
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
enum Decision {
    Done,
    /// Réessayer après ce délai ; `true` : bloquer toutes les requêtes (quota épuisé).
    Retry(Duration, bool),
    Fail(HubError),
}

#[cfg(test)]
impl PartialEq for HubError {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

#[cfg(test)]
impl Eq for HubError {}

/// Que faire de ce résultat, à la tentative `attempt` (0 pour la première) ? Pur, testé.
fn decide(
    result: &std::result::Result<Response, TransportError>,
    url: &str,
    attempt: u32,
    policy: &Policy,
) -> Decision {
    let backoff = Duration::from_secs(1 << attempt.min(5));
    match result {
        Err(TransportError(msg)) if attempt < policy.max_network_retries => {
            tracing::debug!(url, "erreur réseau ({msg}), nouvelle tentative");
            Decision::Retry(backoff, false)
        }
        Err(TransportError(msg)) => Decision::Fail(HubError::Network(msg.clone())),
        Ok(r) if r.is_success() => Decision::Done,
        Ok(r) if r.status == 429 => {
            let reset_s = r
                .rate_limit
                .as_ref()
                .map(|rl| u64::from(rl.reset_s))
                .or(r.retry_after_s)
                .unwrap_or(60);
            // +1 s : la fenêtre se termine parfois une fraction de seconde après `t`.
            let wait = Duration::from_secs(reset_s + 1);
            if attempt < policy.max_retries && wait <= policy.max_wait {
                Decision::Retry(wait, true)
            } else {
                Decision::Fail(HubError::RateLimited { reset_s })
            }
        }
        Ok(r) if (r.status >= 500 || r.status == 408) && attempt < policy.max_retries => {
            Decision::Retry(backoff, false)
        }
        Ok(r) => Decision::Fail(HubError::Http {
            status: r.status,
            url: url.to_owned(),
        }),
    }
}

/// Limite le nombre de requêtes simultanées et leur cadence, et bloque tout le monde après
/// un 429 jusqu'à la remise à zéro du quota.
struct Limiter {
    sem: Semaphore,
    next_slot: Mutex<Instant>,
    spacing: Duration,
}

impl Limiter {
    fn new(policy: &Policy) -> Self {
        Self {
            sem: Semaphore::new(policy.concurrency.max(1)),
            next_slot: Mutex::new(Instant::now()),
            spacing: policy.spacing,
        }
    }

    async fn acquire(&self) -> SemaphorePermit<'_> {
        let permit = self.sem.acquire().await.expect("sémaphore jamais fermé");
        let start = {
            let mut next = self.next_slot.lock().expect("verrou du limiteur");
            let start = (*next).max(Instant::now());
            *next = start + self.spacing;
            start
        };
        tokio::time::sleep_until(start).await;
        permit
    }

    fn pause_all(&self, wait: Duration) {
        let mut next = self.next_slot.lock().expect("verrou du limiteur");
        *next = (*next).max(Instant::now() + wait);
    }
}

/// Événements émis pendant une synchronisation (relayés au frontend par Tauri).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubEvent {
    Progress(SyncProgress),
    RateLimit(RateLimitInfo),
}

type Observer = Arc<dyn Fn(HubEvent) + Send + Sync>;

/// Client Hugging Face : un transport, plus le limiteur, les nouvelles tentatives et les
/// compteurs d'appels.
pub struct Hub<T> {
    transport: T,
    base: String,
    policy: Policy,
    limiter: Limiter,
    api_calls: AtomicU32,
    resolver_calls: AtomicU32,
    last_rate_limit: Mutex<Option<RateLimitInfo>>,
    observer: Option<Observer>,
}

impl<T: Transport> Hub<T> {
    pub fn new(transport: T) -> Self {
        Self::with_policy(transport, Policy::default())
    }

    pub fn with_policy(transport: T, policy: Policy) -> Self {
        Self {
            transport,
            base: crate::api::HF_BASE.to_owned(),
            limiter: Limiter::new(&policy),
            policy,
            api_calls: AtomicU32::new(0),
            resolver_calls: AtomicU32::new(0),
            last_rate_limit: Mutex::new(None),
            observer: None,
        }
    }

    /// Reçoit la progression et le quota restant.
    pub fn on_event(mut self, f: impl Fn(HubEvent) + Send + Sync + 'static) -> Self {
        self.observer = Some(Arc::new(f));
        self
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn api_calls(&self) -> u32 {
        self.api_calls.load(Ordering::Relaxed)
    }

    pub fn resolver_calls(&self) -> u32 {
        self.resolver_calls.load(Ordering::Relaxed)
    }

    pub fn rate_limit(&self) -> Option<RateLimitInfo> {
        *self.last_rate_limit.lock().expect("verrou du quota")
    }

    pub(crate) fn emit(&self, event: HubEvent) {
        if let Some(f) = &self.observer {
            f(event);
        }
    }

    /// Envoie la requête (avec nouvelles tentatives) ; renvoie une réponse 2xx ou une erreur.
    pub async fn get(&self, req: &Request) -> Result<Response> {
        let mut attempt = 0;
        loop {
            let result = {
                let _permit = self.limiter.acquire().await;
                match req.endpoint {
                    Endpoint::Api => self.api_calls.fetch_add(1, Ordering::Relaxed),
                    Endpoint::Resolver => self.resolver_calls.fetch_add(1, Ordering::Relaxed),
                };
                self.transport.get(req).await
            };
            if let Ok(r) = &result {
                self.track_rate_limit(r);
            }
            match decide(&result, &req.url, attempt, &self.policy) {
                Decision::Done => return result.map_err(|e| HubError::Network(e.0)),
                Decision::Retry(wait, global) => {
                    if global {
                        tracing::warn!(
                            "quota Hugging Face atteint : pause de {} s",
                            wait.as_secs()
                        );
                        self.limiter.pause_all(wait);
                    } else {
                        tokio::time::sleep(wait).await;
                    }
                    attempt += 1;
                }
                Decision::Fail(e) => return Err(e),
            }
        }
    }

    /// Requête API dont la réponse JSON est désérialisée.
    pub async fn get_json<R: serde::de::DeserializeOwned>(&self, url: String) -> Result<R> {
        let resp = self.get(&Request::api(url.clone())).await?;
        serde_json::from_slice(&resp.body).map_err(|e| HubError::parse(url, e))
    }

    /// Liste paginée : suit `Link: rel="next"` jusqu'à `max_items` éléments.
    pub async fn get_paginated<R: serde::de::DeserializeOwned>(
        &self,
        url: String,
        max_items: usize,
    ) -> Result<Vec<R>> {
        let mut items = Vec::new();
        let mut next = Some(url);
        while let Some(url) = next.take() {
            let resp = self.get(&Request::api(url.clone())).await?;
            let mut page: Vec<R> =
                serde_json::from_slice(&resp.body).map_err(|e| HubError::parse(&url, e))?;
            let empty = page.is_empty();
            items.append(&mut page);
            if items.len() < max_items && !empty {
                next = resp.link_next;
            }
        }
        items.truncate(max_items);
        Ok(items)
    }

    fn track_rate_limit(&self, r: &Response) {
        let Some(rl) = r.rate_limit.as_ref().filter(|rl| rl.category == "api") else {
            return;
        };
        let info = RateLimitInfo {
            remaining: rl.remaining,
            reset_s: rl.reset_s,
        };
        *self.last_rate_limit.lock().expect("verrou du quota") = Some(info);
        self.emit(HubEvent::RateLimit(info));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeTransport;

    #[test]
    fn token_masque() {
        let t = HfToken::new("  hf_secret  ").unwrap();
        assert_eq!(t.expose(), "hf_secret");
        assert_eq!(format!("{t:?}"), "HfToken(***)");
        assert!(HfToken::new("   ").is_none());
    }

    #[test]
    fn rate_limit() {
        assert_eq!(
            parse_rate_limit("\"api\";r=489;t=189"),
            Some(RateLimit {
                category: "api".into(),
                remaining: 489,
                reset_s: 189
            })
        );
        assert_eq!(
            parse_rate_limit("\"resolvers\"; r=0; t=7")
                .unwrap()
                .remaining,
            0
        );
        assert_eq!(parse_rate_limit("\"api\";r=12"), None);
        assert_eq!(parse_rate_limit(""), None);
    }

    #[test]
    fn link_next() {
        // En-tête réel (tronqué au curseur).
        let real = r#"<https://huggingface.co/api/models?filter=gguf&limit=3&expand=gguf&cursor=eyIkb3IiOlt7ImRvd25sb2Fk%3D%3D>; rel="next""#;
        assert_eq!(
            parse_link_next(real).as_deref(),
            Some(
                "https://huggingface.co/api/models?filter=gguf&limit=3&expand=gguf&cursor=eyIkb3IiOlt7ImRvd25sb2Fk%3D%3D"
            )
        );
        assert_eq!(
            parse_link_next(r#"<https://a/prev>; rel="prev", <https://a/next>; rel="next""#)
                .as_deref(),
            Some("https://a/next")
        );
        assert_eq!(parse_link_next(r#"<https://a/prev>; rel="prev""#), None);
    }

    fn resp(status: u16, rl: Option<(u32, u32)>, retry_after: Option<u64>) -> Response {
        Response {
            rate_limit: rl.map(|(remaining, reset_s)| RateLimit {
                category: "api".into(),
                remaining,
                reset_s,
            }),
            retry_after_s: retry_after,
            ..Response::status(status)
        }
    }

    #[test]
    fn decisions() {
        let p = Policy::default();
        let d = |r: std::result::Result<Response, TransportError>, attempt| {
            decide(&r, "https://huggingface.co/x", attempt, &p)
        };
        assert_eq!(d(Ok(resp(200, None, None)), 0), Decision::Done);
        assert_eq!(d(Ok(resp(206, None, None)), 0), Decision::Done);
        // 429 : on attend la fin de la fenêtre (t), pour tout le monde.
        assert_eq!(
            d(Ok(resp(429, Some((0, 42)), None)), 0),
            Decision::Retry(Duration::from_secs(43), true)
        );
        assert_eq!(
            d(Ok(resp(429, None, Some(5))), 0),
            Decision::Retry(Duration::from_secs(6), true)
        );
        assert_eq!(
            d(Ok(resp(429, Some((0, 42)), None)), 3),
            Decision::Fail(HubError::RateLimited { reset_s: 42 })
        );
        assert_eq!(
            d(Ok(resp(429, Some((0, 3600)), None)), 0),
            Decision::Fail(HubError::RateLimited { reset_s: 3600 })
        );
        // 5xx : backoff exponentiel.
        assert_eq!(
            d(Ok(resp(503, None, None)), 1),
            Decision::Retry(Duration::from_secs(2), false)
        );
        assert!(matches!(
            d(Ok(resp(503, None, None)), 3),
            Decision::Fail(HubError::Http { status: 503, .. })
        ));
        // 401 / 404 : inutile de réessayer.
        assert!(matches!(
            d(Ok(resp(401, None, None)), 0),
            Decision::Fail(HubError::Http { status: 401, .. })
        ));
        // Réseau : une seule nouvelle tentative.
        let net = || Err(TransportError("connexion refusée".into()));
        assert_eq!(d(net(), 0), Decision::Retry(Duration::from_secs(1), false));
        assert_eq!(
            d(net(), 1),
            Decision::Fail(HubError::Network("connexion refusée".into()))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn reessaie_apres_429_et_compte_les_appels() {
        let fake = FakeTransport::new();
        fake.push(
            "https://huggingface.co/api/x",
            resp(429, Some((0, 30)), None),
        );
        fake.push(
            "https://huggingface.co/api/x",
            Response {
                rate_limit: Some(RateLimit {
                    category: "api".into(),
                    remaining: 499,
                    reset_s: 300,
                }),
                ..Response::ok(b"[1,2]".to_vec())
            },
        );
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let hub = Hub::new(fake).on_event(move |e| sink.lock().unwrap().push(e));
        let start = Instant::now();
        let v: Vec<u32> = hub
            .get_json("https://huggingface.co/api/x".into())
            .await
            .unwrap();
        assert_eq!(v, vec![1, 2]);
        assert!(start.elapsed() >= Duration::from_secs(31));
        assert_eq!(hub.api_calls(), 2);
        assert_eq!(hub.resolver_calls(), 0);
        assert_eq!(
            hub.rate_limit(),
            Some(RateLimitInfo {
                remaining: 499,
                reset_s: 300
            })
        );
        assert_eq!(events.lock().unwrap().len(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn pagination() {
        let fake = FakeTransport::new();
        fake.push(
            "https://huggingface.co/p1",
            Response {
                link_next: Some("https://huggingface.co/p2".into()),
                ..Response::ok(b"[1,2]".to_vec())
            },
        );
        fake.push(
            "https://huggingface.co/p2",
            Response {
                link_next: Some("https://huggingface.co/p3".into()),
                ..Response::ok(b"[3,4]".to_vec())
            },
        );
        let hub = Hub::new(fake);
        let v: Vec<u32> = hub
            .get_paginated("https://huggingface.co/p1".into(), 3)
            .await
            .unwrap();
        // La page 3 n'est pas demandée : on a déjà 3 éléments.
        assert_eq!(v, vec![1, 2, 3]);
        assert_eq!(hub.api_calls(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn cadence_limitee() {
        let fake = FakeTransport::new();
        for _ in 0..8 {
            fake.push("https://huggingface.co/x", Response::ok(b"0".to_vec()));
        }
        let hub = Hub::new(fake);
        let start = Instant::now();
        let futs = (0..8).map(|_| hub.get_json::<u32>("https://huggingface.co/x".into()));
        let results = futures_util::future::join_all(futs).await;
        assert!(results.iter().all(Result::is_ok));
        // 8 départs espacés de 250 ms : au moins 1,75 s.
        assert!(start.elapsed() >= Duration::from_millis(1750));
    }
}
