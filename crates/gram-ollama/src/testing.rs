//! Faux serveur Ollama pour les tests (aucun Ollama réel) : il répond sur `127.0.0.1` avec les
//! réponses de `tests/fixtures/ollama/` et garde la trace des requêtes reçues.
//!
//! Disponible pour les autres crates avec la fonctionnalité `test-support`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub const TAGS: &str = include_str!("../../../tests/fixtures/ollama/tags.json");
pub const PS: &str = include_str!("../../../tests/fixtures/ollama/ps.json");
pub const GENERATE: &str = include_str!("../../../tests/fixtures/ollama/generate.json");
pub const NOT_FOUND: &str = include_str!("../../../tests/fixtures/ollama/error-not-found.json");

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

/// Réponses enregistrées : version, modèles installés, modèles chargés, génération.
pub fn standard_response(r: &Recorded) -> (u16, String) {
    match (r.method.as_str(), r.path.as_str()) {
        ("GET", "/api/version") => (200, r#"{"version":"0.13.1"}"#.into()),
        ("GET", "/api/tags") => (200, TAGS.into()),
        ("GET", "/api/ps") => (200, PS.into()),
        ("POST", "/api/generate") => (200, GENERATE.into()),
        _ => (404, r#"{"error":"404 page not found"}"#.into()),
    }
}

/// Serveur HTTP minimal, une connexion à la fois ; arrêté à la destruction.
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
                    let (status, body) = respond(&req);
                    requests.lock().expect("verrou").push(req);
                    write_response(stream, status, &body);
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

fn write_response(mut stream: TcpStream, status: u16, body: &str) {
    let reason = if status < 400 { "OK" } else { "Error" };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Write);
}
