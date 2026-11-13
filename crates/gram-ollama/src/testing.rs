//! Faux serveur Ollama pour les tests (aucun Ollama réel) : il répond sur `127.0.0.1` avec les
//! réponses de `tests/fixtures/ollama/` et garde la trace des requêtes reçues. Une réponse peut
//! arriver en plusieurs morceaux séparés par des pauses, comme les flux d'Ollama ([`Reply`]).
//!
//! Disponible pour les autres crates avec la fonctionnalité `test-support`.

use crate::measure::MEASURE_TOKENS;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const TAGS: &str = include_str!("../../../tests/fixtures/ollama/tags.json");
pub const PS: &str = include_str!("../../../tests/fixtures/ollama/ps.json");
pub const GENERATE: &str = include_str!("../../../tests/fixtures/ollama/generate.json");
pub const GENERATE_WARMUP: &str =
    include_str!("../../../tests/fixtures/ollama/generate-warmup.json");
/// Version d'Ollama qui a donné les réponses enregistrées.
pub const VERSION: &str = "0.40.16";
pub const NOT_FOUND: &str = include_str!("../../../tests/fixtures/ollama/error-not-found.json");
/// `POST /api/pull` d'un modèle déjà installé (Qwen3 1.7B) : 9 lignes.
pub const PULL_INSTALLED: &str =
    include_str!("../../../tests/fixtures/ollama/pull-installed.ndjson");
/// `POST /api/pull` repris après annulation (SmolVLM 256M : modèle, projecteur, paramètres).
pub const PULL_RESUME: &str = include_str!("../../../tests/fixtures/ollama/pull-resume.ndjson");
/// Somme des `total` des couches de [`PULL_RESUME`].
pub const PULL_RESUME_TOTAL: u64 = 175_054_528 + 103_769_856 + 475;
/// `POST /api/pull` d'une quantification qui n'existe pas : erreur au milieu du flux.
pub const PULL_ERROR_QUANT: &str =
    include_str!("../../../tests/fixtures/ollama/pull-error-quant.ndjson");
/// `POST /api/pull` d'un dépôt protégé depuis `hf.co/` : erreur au milieu du flux.
pub const PULL_ERROR_GATED: &str =
    include_str!("../../../tests/fixtures/ollama/pull-error-gated.ndjson");
pub const CREATE: &str = include_str!("../../../tests/fixtures/ollama/create.json");
/// `POST /api/show` du modèle dérivé de Qwen3 1.7B, réglé à 16 384 tokens.
pub const SHOW_ALIAS: &str = include_str!("../../../tests/fixtures/ollama/show-alias.json");
/// `POST /api/chat` de Qwen3 1.7B sans `think` : réflexion (à part) puis réponse, 100 lignes.
pub const CHAT_DEFAULT: &str = include_str!("../../../tests/fixtures/ollama/chat-default.ndjson");
/// `POST /api/chat` de Qwen3 1.7B avec `think: false` : « 12 × 3 = 36. », 11 lignes.
pub const CHAT_NO_THINK: &str = include_str!("../../../tests/fixtures/ollama/chat-no-think.ndjson");
/// Réponse de [`CHAT_NO_THINK`].
pub const CHAT_NO_THINK_CONTENT: &str = "12 × 3 = 36.";

/// Réponse de `/api/tags` qui liste ces modèles.
pub fn tags_json(names: &[&str]) -> String {
    let models: Vec<serde_json::Value> = names
        .iter()
        .map(|n| serde_json::json!({ "name": n, "model": n, "size": 1, "details": {} }))
        .collect();
    serde_json::json!({ "models": models }).to_string()
}

/// Réponse du faux serveur : un statut, puis le corps en un ou plusieurs morceaux, avec des
/// pauses entre eux.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    status: u16,
    parts: Vec<Part>,
    chunked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Data(String),
    Pause(Duration),
    /// Connexion fermée sans terminer la réponse (Ollama arrêté au milieu d'un flux).
    Abort,
}

impl Reply {
    /// Corps entier, annoncé par `Content-Length`.
    pub fn new(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            parts: vec![Part::Data(body.into())],
            chunked: false,
        }
    }

    /// Corps envoyé par morceaux (`Transfer-Encoding: chunked`), comme les flux d'Ollama.
    pub fn chunked(status: u16) -> Self {
        Self {
            status,
            parts: Vec::new(),
            chunked: true,
        }
    }

    /// Statut 200, un morceau par ligne.
    pub fn ndjson(text: &str) -> Self {
        Self::chunked(200).lines(text)
    }

    /// Ajoute un morceau par ligne de `text`.
    pub fn lines(mut self, text: &str) -> Self {
        for line in text.lines().filter(|l| !l.is_empty()) {
            self.parts.push(Part::Data(format!("{line}\n")));
        }
        self
    }

    pub fn data(mut self, data: impl Into<String>) -> Self {
        self.parts.push(Part::Data(data.into()));
        self
    }

    pub fn pause(mut self, pause: Duration) -> Self {
        self.parts.push(Part::Pause(pause));
        self
    }

    /// Ferme la connexion ici, sans terminer la réponse.
    pub fn abort(mut self) -> Self {
        self.parts.push(Part::Abort);
        self
    }
}

impl From<(u16, String)> for Reply {
    fn from((status, body): (u16, String)) -> Self {
        Self::new(status, body)
    }
}

/// Une requête reçue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub body: String,
}

impl Recorded {
    /// Corps JSON de la requête (`Null` s'il est vide ou invalide).
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or_default()
    }
}

/// Réponses enregistrées : version, modèles installés, modèles chargés, génération (celle de
/// l'échauffement, qui charge le modèle, pour une requête de moins de [`MEASURE_TOKENS`]),
/// téléchargement d'un modèle déjà là, création, `show` d'un modèle dérivé, suppression et
/// discussion (sans réflexion).
pub fn standard_response(r: &Recorded) -> (u16, String) {
    match (r.method.as_str(), r.path.as_str()) {
        ("GET", "/api/version") => (200, format!(r#"{{"version":"{VERSION}"}}"#)),
        ("GET", "/api/tags") => (200, TAGS.into()),
        ("GET", "/api/ps") => (200, PS.into()),
        ("POST", "/api/pull") => (200, PULL_INSTALLED.into()),
        ("POST", "/api/create") => (200, CREATE.into()),
        ("POST", "/api/show") => (200, SHOW_ALIAS.into()),
        ("DELETE", "/api/delete") => (200, String::new()),
        ("POST", "/api/chat") => (200, CHAT_NO_THINK.into()),
        ("POST", "/api/generate") => {
            let tokens = r.json()["options"]["num_predict"].as_u64();
            if tokens.is_some_and(|n| n < u64::from(MEASURE_TOKENS)) {
                (200, GENERATE_WARMUP.into())
            } else {
                (200, GENERATE.into())
            }
        }
        _ => (404, r#"{"error":"404 page not found"}"#.into()),
    }
}

/// Serveur HTTP minimal, une connexion à la fois (une réponse en pause bloque les suivantes) ;
/// arrêté à la destruction.
pub struct FakeOllama {
    addr: SocketAddr,
    url: String,
    requests: Arc<Mutex<Vec<Recorded>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FakeOllama {
    /// `respond` renvoie le statut HTTP et le corps JSON de chaque requête.
    pub fn start(respond: impl Fn(&Recorded) -> (u16, String) + Send + Sync + 'static) -> Self {
        Self::streaming(move |r| Reply::from(respond(r)))
    }

    /// `respond` renvoie la réponse de chaque requête, éventuellement en morceaux.
    pub fn streaming(respond: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("port local");
        let addr = listener.local_addr().expect("adresse locale");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (requests, stop) = (Arc::clone(&requests), Arc::clone(&stop));
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    let Some(req) = read_request(&stream) else {
                        continue;
                    };
                    let reply = respond(&req);
                    requests.lock().expect("verrou").push(req);
                    write_reply(stream, &reply, &stop);
                }
            })
        };
        Self {
            addr,
            url: format!("http://{addr}"),
            requests,
            stop,
            thread: Some(thread),
        }
    }

    /// Avec les réponses enregistrées ([`standard_response`]).
    pub fn standard() -> Self {
        Self::start(standard_response)
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().expect("verrou").clone()
    }
}

impl Drop for FakeOllama {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Débloque `accept`.
        let _ = TcpStream::connect(self.addr);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Lit toute la requête (en-têtes puis corps selon `Content-Length`) avant de répondre : sous
/// Windows, fermer un socket qui a des données non lues provoque un RST qui efface la réponse.
fn read_request(stream: &TcpStream) -> Option<Recorded> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let mut length = 0usize;
    loop {
        line.clear();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().ok()?;
        }
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).ok()?;
    Some(Recorded {
        method,
        path,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

/// Écrit la réponse ; s'arrête si le client a fermé la connexion ou si le serveur s'arrête.
fn write_reply(mut stream: TcpStream, reply: &Reply, stop: &AtomicBool) {
    let status = reply.status;
    let reason = if status < 400 { "OK" } else { "Error" };
    let length = if reply.chunked {
        "Transfer-Encoding: chunked".to_string()
    } else {
        let len: usize = reply
            .parts
            .iter()
            .map(|p| match p {
                Part::Data(d) => d.len(),
                Part::Pause(_) | Part::Abort => 0,
            })
            .sum();
        format!("Content-Length: {len}")
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\n\
         {length}\r\nConnection: close\r\n\r\n"
    );
    let mut send = |bytes: &[u8]| {
        stream
            .write_all(bytes)
            .and_then(|()| stream.flush())
            .is_ok()
    };
    if !send(head.as_bytes()) {
        return;
    }
    for part in &reply.parts {
        let sent = match part {
            Part::Data(d) if reply.chunked => {
                d.is_empty() || send(format!("{:x}\r\n{d}\r\n", d.len()).as_bytes())
            }
            Part::Data(d) => send(d.as_bytes()),
            Part::Pause(pause) => sleep_unless(*pause, stop),
            Part::Abort => false,
        };
        if !sent {
            return;
        }
    }
    if reply.chunked && !send(b"0\r\n\r\n") {
        return;
    }
    let _ = stream.shutdown(Shutdown::Write);
}

/// Attend `pause`, sauf si le serveur s'arrête entre-temps (`false`).
fn sleep_unless(pause: Duration, stop: &AtomicBool) -> bool {
    let end = Instant::now() + pause;
    while Instant::now() < end {
        if stop.load(Ordering::SeqCst) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10).min(end - Instant::now()));
    }
    true
}
