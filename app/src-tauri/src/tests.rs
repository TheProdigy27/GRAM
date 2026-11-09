//! Commandes IPC de bout en bout, avec le runtime simulé de Tauri : noms des commandes,
//! arguments en camelCase (comme les envoie le WebView), sérialisation des réponses et des
//! erreurs, événements. Sans réseau, sans détection réelle et sans toucher au trousseau.

use crate::state::{AppState, Config};
use crate::token::MemoryStore;
use gram_core::{RuntimeStatus, RuntimesStatus};
use gram_hub::Cache;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{INVOKE_KEY, MockRuntime, mock_builder, mock_context, noop_assets};
use tauri::webview::InvokeRequest;
use tauri::{App, Listener, Manager, WebviewWindow, WebviewWindowBuilder};

/// Dossier temporaire supprimé à la fin du test.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("gram-app-{name}-{}", std::process::id()));
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

/// Machine de développement enregistrée : Ryzen 7 9800X3D, RTX 5080 16 Go, 64 Go de DDR5.
fn raw_hardware() -> gram_hw::RawHardware {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/hardware/raw-9800x3d-rtx5080-parsec.json"
    ))
    .unwrap()
}

fn runtimes() -> RuntimesStatus {
    RuntimesStatus {
        ollama: RuntimeStatus {
            installed: true,
            running: true,
            path: None,
            version: Some("0.13.1".into()),
        },
        ..RuntimesStatus::default()
    }
}

fn state_in(dir: &Path) -> AppState {
    AppState::new(
        Config {
            // Cache vide : le catalogue vient du catalogue embarqué.
            cache: Cache::new(dir.join("cache")),
            settings_path: dir.join("settings.json"),
            network: false,
        },
        Arc::new(raw_hardware),
        Arc::new(runtimes),
        Box::new(MemoryStore::default()),
    )
}

/// Application simulée ; ses fichiers (cache, `settings.json`) sont dans `dir`.
struct Harness {
    app: App<MockRuntime>,
    webview: WebviewWindow<MockRuntime>,
    dir: PathBuf,
}

impl Harness {
    fn new(dir: &TempDir) -> Self {
        let app = mock_builder()
            .manage(state_in(&dir.0))
            .invoke_handler(crate::commands::handler())
            .build(mock_context(noop_assets()))
            .unwrap();
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        Self {
            app,
            webview,
            dir: dir.0.clone(),
        }
    }

    /// `invoke(cmd, args)` tel que l'appelle le frontend.
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

    fn ok(&self, cmd: &str, args: Value) -> Value {
        self.invoke(cmd, args)
            .unwrap_or_else(|e| panic!("{cmd} a échoué : {e}"))
    }

    /// Charges utiles reçues pour un événement.
    fn record(&self, event: &str) -> Arc<Mutex<Vec<Value>>> {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        self.app.listen(event, move |e| {
            sink.lock()
                .unwrap()
                .push(serde_json::from_str(e.payload()).unwrap());
        });
        seen
    }
}

const GIB: f64 = (1u64 << 30) as f64;

#[test]
fn ping() {
    let dir = TempDir::new("ping");
    let h = Harness::new(&dir);
    let r = h.ok("ping", json!({}));
    assert_eq!(r["message"], "pong");
    assert_eq!(r["coreVersion"], gram_core::VERSION);
}

#[test]
fn materiel_corrections_et_simulation() {
    let dir = TempDir::new("materiel");
    let h = Harness::new(&dir);
    let ready = h.record(crate::events::HARDWARE_READY);

    let hw = h.ok("get_hardware", json!({}));
    assert_eq!(hw["gpu"]["name"], "NVIDIA GeForce RTX 5080");
    assert_eq!(hw["ram"]["speedMts"], 6000);
    // Gardé en mémoire : `hardware-ready` n'est émis qu'à la détection.
    assert_eq!(h.ok("get_hardware", json!({})), hw);
    assert_eq!(ready.lock().unwrap().len(), 1);
    assert_eq!(
        ready.lock().unwrap()[0]["gpu"]["name"],
        "NVIDIA GeForce RTX 5080"
    );

    // Simulation : RTX 4090 et 128 Go de RAM.
    let sim = h.ok(
        "set_hardware_overrides",
        json!({ "overrides": {
            "simulatedGpu": "NVIDIA GeForce RTX 4090 24 Go",
            "simulatedRamTotal": 128u64 << 30
        } }),
    );
    assert_eq!(sim["gpu"]["name"], "NVIDIA GeForce RTX 4090");
    assert_eq!(
        sim["overrides"]["simulatedGpu"],
        "NVIDIA GeForce RTX 4090 24 Go"
    );
    let r = h.ok("rank", json!({ "request": { "limit": 1 } }));
    let vram = r["budget"]["vram"].as_f64().unwrap() / GIB;
    assert!(
        (21.0..22.0).contains(&vram),
        "VRAM simulée utilisable : {vram}"
    );

    // Retour aux valeurs détectées, puis nouvelle détection (corrections gardées).
    let back = h.ok("set_hardware_overrides", json!({ "overrides": {} }));
    assert_eq!(back["gpu"]["name"], "NVIDIA GeForce RTX 5080");
    h.ok(
        "set_hardware_overrides",
        json!({ "overrides": { "ramChannels": 1 } }),
    );
    let rescanned = h.ok("rescan_hardware", json!({}));
    assert_eq!(rescanned["ram"]["channels"], 1);
    assert_eq!(rescanned["ram"]["channelsSource"], "manual");
    assert_eq!(ready.lock().unwrap().len(), 2);

    let rt = h.ok("detect_runtimes", json!({}));
    assert_eq!(rt["ollama"]["installed"], true);
    assert_eq!(rt["lmStudio"]["installed"], false);
}

#[test]
fn classement_et_fiche_detail() {
    let dir = TempDir::new("classement");
    let h = Harness::new(&dir);
    let r = h.ok(
        "rank",
        json!({ "request": { "useCase": "code", "ctx": 16384, "limit": 5, "showTooBig": true } }),
    );
    assert_eq!(r["useCase"], "code");
    assert_eq!(r["ctx"], 16384);
    assert_eq!(r["runtime"], "ollama");
    assert_eq!(r["dataSource"]["kind"], "snapshot");
    let items = r["items"].as_array().unwrap();
    assert_eq!(items.len(), 5);
    assert!(r["totalItems"].as_u64().unwrap() > 100);
    let first = &items[0];
    for key in [
        "repoId",
        "displayName",
        "best",
        "score",
        "explanation",
        "variantCount",
    ] {
        assert!(first.get(key).is_some(), "champ {key} absent : {first}");
    }
    assert!(first["best"]["estimate"]["tokS"].as_f64().unwrap() >= 5.0);
    assert!(first["score"]["total"].as_f64().unwrap() <= 100.0);
    assert!(!r["tooBig"].as_array().unwrap().is_empty());

    // Valeurs par défaut : polyvalent, contexte 8k.
    let d = h.ok("rank", json!({ "request": {} }));
    assert_eq!(
        (d["useCase"].as_str(), d["ctx"].as_u64()),
        (Some("general"), Some(8192))
    );

    // Fiche détail du premier modèle (hors ligne : pas d'enrichissement).
    let repo = first["repoId"].as_str().unwrap();
    let detail = h.ok(
        "get_model_detail",
        json!({ "request": { "repoId": repo, "useCase": "code", "ctx": 16384, "runtime": "llamaCpp" } }),
    );
    assert_eq!(detail["model"]["repoId"], repo);
    assert_eq!(detail["runtime"], "llamaCpp");
    assert_eq!(detail["hfUrl"], format!("https://huggingface.co/{repo}"));
    let best = &detail["variants"][detail["best"].as_u64().unwrap() as usize];
    assert_eq!(best["variant"], first["best"]["variant"]);
    let cmd = best["launch"]["commands"][0]["command"].as_str().unwrap();
    assert!(cmd.starts_with("llama-server -hf "), "{cmd}");

    let err = h
        .invoke(
            "get_model_detail",
            json!({ "request": { "repoId": "x/absent" } }),
        )
        .unwrap_err();
    assert_eq!(err, "Modèle « x/absent » absent du catalogue.");
}

#[test]
fn parametres_persistants() {
    let dir = TempDir::new("parametres");
    let path = dir.0.join("settings.json");
    {
        let h = Harness::new(&dir);
        let mut s = h.ok("get_settings", json!({}));
        assert_eq!(s["runtime"], "ollama");
        assert_eq!(s["useCases"]["code"]["ctx"], 16384);
        s["runtime"] = json!("lmStudio");
        s["estimate"]["vramMargin"] = json!(5.0);
        s["useCases"]["general"]["ctx"] = json!(32768);
        let saved = h.ok("save_settings", json!({ "settings": s }));
        assert_eq!(saved["runtime"], "lmStudio");
        assert!(
            (saved["estimate"]["vramMargin"].as_f64().unwrap() - 0.9).abs() < 1e-6,
            "borné"
        );
        assert_eq!(h.ok("get_settings", json!({})), saved);
        // Le classement suit les paramètres.
        let r = h.ok("rank", json!({ "request": { "limit": 1 } }));
        assert_eq!(
            (r["runtime"].as_str(), r["ctx"].as_u64()),
            (Some("lmStudio"), Some(32768))
        );
        assert!(path.exists());
    }
    // Relu au prochain lancement.
    let h = Harness::new(&dir);
    assert_eq!(h.ok("get_settings", json!({}))["runtime"], "lmStudio");
}

#[test]
fn token_jamais_renvoye() {
    let dir = TempDir::new("token");
    let h = Harness::new(&dir);
    assert_eq!(h.ok("has_hf_token", json!({})), false);
    assert_eq!(
        h.ok("set_hf_token", json!({ "token": "  hf_secret123  " })),
        Value::Null
    );
    assert_eq!(h.ok("has_hf_token", json!({})), true);
    let state = h.app.state::<AppState>();
    assert_eq!(
        state.tokens().get().unwrap().as_deref(),
        Some("hf_secret123")
    );
    // Le token n'apparaît dans aucune réponse.
    let settings = h.ok("get_settings", json!({})).to_string();
    assert!(!settings.contains("hf_secret"));
    assert_eq!(
        h.invoke("set_hf_token", json!({ "token": "   " }))
            .unwrap_err(),
        "Token Hugging Face vide."
    );
    h.ok("clear_hf_token", json!({}));
    assert_eq!(h.ok("has_hf_token", json!({})), false);
}

#[test]
fn synchronisation_hors_ligne_et_evenements() {
    let dir = TempDir::new("sync");
    let h = Harness::new(&dir);
    let done = h.record(crate::events::SYNC_DONE);
    let summary = h.ok("sync_catalog", json!({ "force": false }));
    assert_eq!(summary["source"]["kind"], "snapshot");
    assert!(summary["models"].as_u64().unwrap() > 300);
    assert_eq!(summary["apiCalls"], 0);
    let events = done.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], summary);
}

#[test]
fn export_markdown_et_csv() {
    let dir = TempDir::new("export");
    let h = Harness::new(&dir);
    h.ok(
        "rank",
        json!({ "request": { "useCase": "fastChat", "limit": 3 } }),
    );
    let md = h.dir.join("classement.md");
    let csv = h.dir.join("classement.csv");
    h.ok(
        "export_ranking",
        json!({ "format": "md", "path": md.to_string_lossy() }),
    );
    h.ok(
        "export_ranking",
        json!({ "format": "csv", "path": csv.to_string_lossy() }),
    );
    let text = std::fs::read_to_string(&md).unwrap();
    assert!(text.starts_with("# Classement GRAM"));
    assert!(text.contains("Cas d'usage : Chat rapide · contexte 4 096 tokens"));
    assert!(text.contains("GPU NVIDIA GeForce RTX 5080"));
    // Exporté en entier, sans la limite de 3 lignes du dernier classement.
    let rows = text
        .lines()
        .filter(|l| l.starts_with("| ") && !l.starts_with("| #"))
        .count();
    assert!(rows > 3, "{rows}");
    let table = std::fs::read_to_string(&csv).unwrap();
    assert!(table.starts_with("\u{feff}section;rang;"));

    let wrong = h.dir.join("classement.exe");
    let err = h
        .invoke(
            "export_ranking",
            json!({ "format": "csv", "path": wrong.to_string_lossy() }),
        )
        .unwrap_err();
    assert_eq!(err, "Le fichier d'export doit avoir l'extension .csv");
    assert!(!wrong.exists());
}
