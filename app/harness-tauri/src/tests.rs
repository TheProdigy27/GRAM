//! Commandes IPC de bout en bout, avec le runtime simulé de Tauri : noms des commandes,
//! sérialisation des réponses et des erreurs. Sans Ollama réel ni détection réelle.

use crate::chat::ChatSend;
use crate::conversations::tests::TempDir;
use crate::state::{Config, HarnessState};
use gram_link::{MANIFEST_FILE, SharedManifest, SharedModel};
use gram_ollama::OllamaClient;
use gram_ollama::testing::{CHAT_NO_THINK_CONTENT, FakeOllama, tags_json};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::ipc::{CallbackFn, Channel, InvokeBody, InvokeResponseBody};
use tauri::test::{INVOKE_KEY, MockRuntime, mock_builder, mock_context, noop_assets};
use tauri::webview::InvokeRequest;
use tauri::{App, Manager, WebviewWindow, WebviewWindowBuilder};

/// Application simulée.
struct Harness {
    _app: App<MockRuntime>,
    webview: WebviewWindow<MockRuntime>,
}

impl Harness {
    fn new(ollama: Option<OllamaClient>, shared_dir: Option<PathBuf>, installed: bool) -> Self {
        Self::with_config(
            Config {
                ollama,
                shared_dir,
                conversations_dir: None,
            },
            installed,
        )
    }

    fn with_config(config: Config, installed: bool) -> Self {
        let state = HarnessState::new(config, Arc::new(move || installed));
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

/// Réponse transmise par le canal propre à la requête (`Channel`), au format que lit
/// l'interface ; puis les commandes des conversations.
#[test]
fn discussion_et_conversations() {
    let dir = TempDir::new("ipc-discussion");
    let fake = FakeOllama::standard();
    let h = Harness::with_config(
        Config {
            ollama: Some(OllamaClient::new(fake.url()).unwrap()),
            shared_dir: None,
            conversations_dir: Some(dir.0.join("conversations")),
        },
        true,
    );
    let events = Arc::new(Mutex::new(Vec::<Value>::new()));
    let sink = Arc::clone(&events);
    let channel = Channel::new(move |body| {
        let InvokeResponseBody::Json(json) = body else {
            panic!("événement binaire");
        };
        sink.lock()
            .unwrap()
            .push(serde_json::from_str(&json).unwrap());
        Ok(())
    });
    let request = ChatSend {
        conversation_id: None,
        model: "gram/qwen3-1.7b:q4_k_m".into(),
        content: Some("Combien font 12 × 3 ?".into()),
    };
    tauri::async_runtime::block_on(crate::commands::chat_send(
        h.webview.state(),
        request,
        channel,
    ))
    .unwrap();

    let events = events.lock().unwrap().clone();
    let kinds: Vec<&str> = events
        .iter()
        .map(|e| e["event"].as_str().unwrap())
        .collect();
    assert_eq!(kinds.first(), Some(&"started"));
    assert_eq!(kinds.last(), Some(&"done"));
    assert!(kinds[1..kinds.len() - 1].iter().all(|k| *k == "delta"));
    let conversation = &events[0]["data"]["conversation"];
    assert_eq!(conversation["title"], "Combien font 12 × 3 ?");
    assert_eq!(conversation["model"], "gram/qwen3-1.7b:q4_k_m");
    let id = conversation["id"].as_str().unwrap().to_string();
    assert_eq!(events[1]["data"], json!({ "content": "1", "thinking": "" }));
    let message = &events.last().unwrap()["data"]["message"];
    assert_eq!(message["role"], "assistant");
    assert_eq!(message["content"], CHAT_NO_THINK_CONTENT);
    assert_eq!(message["thinking"], Value::Null);
    assert_eq!(message["stats"]["evalCount"], 11);
    assert_eq!(message["stats"]["doneReason"], "stop");
    assert_eq!(message["interrupted"], false);
    assert_eq!(message["error"], Value::Null);

    // Conversations.
    let list = h.invoke("list_conversations", json!({})).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], id.as_str());
    let full = h.invoke("get_conversation", json!({ "id": id })).unwrap();
    assert_eq!(full["messages"].as_array().unwrap().len(), 2);
    assert_eq!(full["messages"][0]["content"], "Combien font 12 × 3 ?");
    assert_eq!(full["instructions"], Value::Null);
    let renamed = h
        .invoke(
            "rename_conversation",
            json!({ "id": id, "title": "  Calcul   mental " }),
        )
        .unwrap();
    assert_eq!(renamed["title"], "Calcul mental");
    assert_eq!(
        renamed["updatedAt"], full["updatedAt"],
        "titre sans effet sur la date"
    );
    let err = h
        .invoke("rename_conversation", json!({ "id": id, "title": " " }))
        .unwrap_err();
    assert!(err.as_str().unwrap().starts_with("Titre vide"), "{err}");
    assert_eq!(
        h.invoke("chat_stop", json!({ "conversationId": id }))
            .unwrap(),
        false
    );
    assert_eq!(
        h.invoke("delete_conversation", json!({ "id": id }))
            .unwrap(),
        true
    );
    let err = h
        .invoke("get_conversation", json!({ "id": id }))
        .unwrap_err();
    assert!(
        err.as_str()
            .unwrap()
            .starts_with("Conversation introuvable"),
        "{err}"
    );
    assert_eq!(
        h.invoke("list_conversations", json!({})).unwrap(),
        json!([])
    );
}

#[test]
fn conversations_sans_dossier() {
    let h = Harness::new(None, None, true);
    let err = h.invoke("list_conversations", json!({})).unwrap_err();
    assert!(
        err.as_str()
            .unwrap()
            .starts_with("Dossier de données introuvable"),
        "{err}"
    );
}

/// Identifiant (dossier de données propre au harness), fenêtre, port de l'interface et CSP,
/// reprise telle quelle de GRAM.
#[test]
fn configuration() {
    let conf: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    assert_eq!(conf["identifier"], crate::state::APP_DIR_NAME);
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
