//! Outils de test : fixtures Hugging Face enregistrées, faux transport, dossier temporaire.

use crate::http::{Request, Response, Transport, TransportError};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Contenu de `tests/fixtures/hf/{rel}`.
pub fn fixture(rel: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/hf")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("fixture {} : {e}", path.display()))
}

type Route = Box<dyn Fn(&Request) -> Option<Response> + Send + Sync>;

/// Transport sans réseau : réponses mises en file par URL, puis une fonction de routage,
/// sinon 404. Garde la trace des requêtes.
#[derive(Default)]
pub struct FakeTransport {
    queued: Mutex<HashMap<String, VecDeque<Response>>>,
    route: Option<Route>,
    pub requests: Mutex<Vec<Request>>,
    /// Simule une coupure réseau.
    pub offline: AtomicBool,
}

impl FakeTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_route(
        route: impl Fn(&Request) -> Option<Response> + Send + Sync + 'static,
    ) -> Self {
        Self {
            route: Some(Box::new(route)),
            ..Self::default()
        }
    }

    pub fn push(&self, url: &str, resp: Response) {
        self.queued
            .lock()
            .unwrap()
            .entry(url.to_owned())
            .or_default()
            .push_back(resp);
    }
}

impl Transport for FakeTransport {
    async fn get(&self, req: &Request) -> Result<Response, TransportError> {
        self.requests.lock().unwrap().push(req.clone());
        if self.offline.load(Ordering::Relaxed) {
            return Err(TransportError("réseau coupé (test)".into()));
        }
        if let Some(r) = self
            .queued
            .lock()
            .unwrap()
            .get_mut(&req.url)
            .and_then(VecDeque::pop_front)
        {
            return Ok(r);
        }
        Ok(self
            .route
            .as_ref()
            .and_then(|f| f(req))
            .unwrap_or_else(|| Response::status(404)))
    }
}

/// Dossier temporaire supprimé à la fin du test.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "gram-hub-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
