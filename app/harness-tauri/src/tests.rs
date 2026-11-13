//! Commandes IPC de bout en bout, avec le runtime simulé de Tauri : noms des commandes,
//! sérialisation des réponses et des erreurs. Sans Ollama réel ni détection réelle.

use crate::state::{Config, HarnessState};
use gram_link::{MANIFEST_FILE, SharedManifest, SharedModel};
use gram_ollama::OllamaClient;
use gram_ollama::testing::{FakeOllama, tags_json};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{INVOKE_KEY, MockRuntime, mock_builder, mock_context, noop_assets};
use tauri::webview::InvokeRequest;
use tauri::{App, WebviewWindow, WebviewWindowBuilder};

/// Dossier temporaire supprimé à la fin du test.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("gram-harness-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Application simulée.
struct Harness {
    _app: App<MockRuntime>,
    webview: WebviewWindow<MockRuntime>,
}

impl Harness {
    fn new(ollama: Option<OllamaClient>, shared_dir: Option<PathBuf>, installed: bool) -> Self {
        let state = HarnessState::new(Config { ollama, shared_dir }, Arc::new(move || installed));
        let app = mock_builder()
            .manage(state)
            .invoke_handler(crate::commands::handler())
            .build(mock_context(noop_assets()))
            .unwrap();
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        Self { _app: app, webview }
    }

    /// `invoke(cmd, args)` tel que l'appelle l'interface.
    fn invoke(&self, cmd: &str, args: Value) -> Result<Value, Value> {
        let url = if cfg!(windows) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        };
        tauri::test::get_ipc_response(
            &self.webview,
            InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: url.parse().unwrap(),
                body: InvokeBody::Json(args),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .map(|body| body.deserialize::<Value>().unwrap())
    }
}

/// Adresse locale où rien n'écoute.
fn closed_port() -> OllamaClient {
    let addr = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    OllamaClient::for_addr(addr).unwrap()
}

#[test]
fn liste_avec_le_manifeste() {
    let dir = TempDir::new("liste");
    let entry: SharedModel = serde_json::from_value(json!({
        "ollamaName": "gram/qwen3-1.7b:q4_k_m",
        "baseName": "hf.co/unsloth/Qwen3-1.7B-GGUF:Q4_K_M",
        "baseInstalledByGram": false,
        "displayName": "Qwen3 1.7B",
        "repoId": "Qwen/Qwen3-1.7B-GGUF",
        "quant": "Q4_K_M",
        "size": 1_107_409_472_u64,
        "ctx": 8192,
        "kvCache": "f16",
        "useCase": "general",
        "vision": false,
        "estimatedTokS": 450.0,
        "measuredTokS": null,
        "addedAt": "2026-11-13T10:00:00Z",
    }))
    .unwrap();
    gram_link::json::save(
        &dir.0.join(MANIFEST_FILE),
        &SharedManifest {
            models: vec![entry],
            ..SharedManifest::default()
        },
    )
    .unwrap();
    // Réponses enregistrées : `/api/show` du modèle dérivé pour tous (16 384 tokens).
    let fake = FakeOllama::start(|r| match r.path.as_str() {
        "/api/tags" => (
            200,
            tags_json(&[
                "hf.co/unsloth/Qwen3-1.7B-GGUF:Q4_K_M",
                "gram/qwen3-1.7b:q4_k_m",
                "qwen3:8b",
            ]),
        ),
        _ => gram_ollama::testing::standard_response(r),
    });
    let h = Harness::new(
        Some(OllamaClient::new(fake.url()).unwrap()),
        Some(dir.0.clone()),
        true,
    );
    let r = h.invoke("list_models", json!({})).unwrap();
    assert_eq!(r["ollama"], "running");
    let models = r["models"].as_array().unwrap();
    assert_eq!(models.len(), 2, "la base est masquée : {r}");
    assert_eq!(models[0]["name"], "gram/qwen3-1.7b:q4_k_m");
    assert_eq!(models[0]["displayName"], "Qwen3 1.7B");
    assert_eq!(models[0]["fromGram"], true);
    assert_eq!(models[0]["ctx"], 16384);
    assert_eq!(models[0]["estimatedTokS"], 450.0);
    assert_eq!(models[0]["measuredTokS"], Value::Null);
    assert_eq!(
        (models[0]["thinking"].clone(), models[0]["tools"].clone()),
        (json!(true), json!(true))
    );
    assert_eq!(models[1]["name"], "qwen3:8b");
    assert_eq!(models[1]["fromGram"], false);
}

#[test]
fn ollama_arrete_ou_absent() {
    let stopped = Harness::new(Some(closed_port()), None, true);
    assert_eq!(
        stopped.invoke("list_models", json!({})).unwrap(),
        json!({ "ollama": "stopped", "models": [] })
    );
    let absent = Harness::new(Some(closed_port()), None, false);
    assert_eq!(
        absent.invoke("list_models", json!({})).unwrap()["ollama"],
        "absent"
    );
}

#[test]
fn erreurs_en_francais() {
    // OLLAMA_HOST inutilisable.
    let h = Harness::new(None, None, true);
    let err = h.invoke("list_models", json!({})).unwrap_err();
    assert!(
        err.as_str()
            .unwrap()
            .starts_with("API d'Ollama inaccessible"),
        "{err}"
    );
    // Réponse illisible d'Ollama.
    let fake = FakeOllama::start(|_| (200, "<html>".into()));
    let h = Harness::new(Some(OllamaClient::new(fake.url()).unwrap()), None, true);
    let err = h.invoke("list_models", json!({})).unwrap_err();
    assert!(err.as_str().unwrap().contains("/api/tags"), "{err}");
}

/// Identifiant (dossier de données propre au harness), fenêtre, port de l'interface et CSP,
/// reprise telle quelle de GRAM.
#[test]
fn configuration() {
    let conf: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    let gram: Value =
        serde_json::from_str(include_str!("../../src-tauri/tauri.conf.json")).unwrap();
    assert_eq!(conf["identifier"], "fr.gram.harness");
    assert_eq!(conf["productName"], "GRAM Harness");
    assert_eq!(conf["app"]["windows"][0]["label"], "main");
    assert_eq!(conf["app"]["security"], gram["app"]["security"]);

    let angular: Value = serde_json::from_str(include_str!("../../ui/angular.json")).unwrap();
    let harness = &angular["projects"]["harness"]["architect"];
    let port = harness["serve"]["options"]["port"].as_u64().unwrap();
    assert_eq!(conf["build"]["devUrl"], format!("http://localhost:{port}"));
    let output = harness["build"]["options"]["outputPath"].as_str().unwrap();
    assert_eq!(
        conf["build"]["frontendDist"],
        format!("../ui/{output}/browser")
    );
}
