//! Commandes IPC de bout en bout, avec le runtime simulé de Tauri : noms des commandes,
//! arguments en camelCase (comme les envoie le WebView), sérialisation des réponses et des
//! erreurs, événements. Sans réseau, sans détection réelle et sans toucher au trousseau.

use crate::state::{AppState, Config};
use crate::token::MemoryStore;
use gram_core::{RuntimeStatus, RuntimesStatus};
use gram_ollama::OllamaClient;
use gram_ollama::testing::FakeOllama;
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

fn state_in(dir: &Path, ollama: Option<OllamaClient>) -> AppState {
    AppState::new(
        // Cache vide : le catalogue vient du catalogue embarqué.
        Config {
            dir: dir.to_path_buf(),
            network: false,
            ollama,
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
        Self::with_ollama(dir, None)
    }

    /// Avec l'API d'Ollama (un faux serveur, `gram_ollama::testing`).
    fn with_ollama(dir: &TempDir, ollama: Option<OllamaClient>) -> Self {
        let app = mock_builder()
            .manage(state_in(&dir.0, ollama))
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

/// Le dossier de données (`%LOCALAPPDATA%\fr.gram.desktop`) est celui de WebView2, et celui que
/// la désinstallation propose de supprimer : il suit l'identifiant de l'application.
#[test]
fn dossier_de_donnees_suit_l_identifiant() {
    let conf: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    assert_eq!(conf["identifier"], gram_hub::cache::APP_ID);
}

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
fn corrections_enregistrees_simulation_non() {
    let dir = TempDir::new("corrections");
    {
        let h = Harness::new(&dir);
        let hw = h.ok(
            "set_hardware_overrides",
            json!({ "overrides": {
                "ramChannels": 4,
                "gpuBandwidthGbs": 900.0,
                "simulatedRamTotal": 128u64 << 30
            } }),
        );
        assert_eq!(hw["ram"]["channels"], 4);
        assert_eq!(hw["gpu"]["bandwidthSource"], "manual");
        assert_eq!(hw["ram"]["installed"], 128u64 << 30);

        // Le dialogue reçoit le matériel tel que détecté, et les GPU simulables.
        let options = h.ok("hardware_options", json!({}));
        let detected = &options["detected"];
        assert_eq!(detected["ram"]["channels"], 2);
        assert_eq!(detected["ram"]["channelsSource"], "estimated");
        assert_eq!(detected["gpu"]["bandwidthSource"], "table");
        assert_eq!(
            detected["overrides"],
            json!(gram_core::Overrides::default())
        );
        let gpus = options["gpus"].as_array().unwrap();
        assert!(
            gpus.iter()
                .any(|g| g["label"] == "NVIDIA GeForce RTX 4090 24 Go"
                    && g["vram"] == 24u64 << 30
                    && g["bandwidthGbs"] == 1008.0)
        );
    }
    let saved: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.0.join("corrections.json")).unwrap())
            .unwrap();
    assert_eq!(saved["ramChannels"], 4);
    assert_eq!(saved["simulatedRamTotal"], Value::Null);

    // Prochain lancement : corrections reprises, sans la simulation.
    let h = Harness::new(&dir);
    let hw = h.ok("get_hardware", json!({}));
    assert_eq!(hw["ram"]["channels"], 4);
    assert_eq!(hw["ram"]["channelsSource"], "manual");
    assert_eq!(hw["gpu"]["bandwidthGbs"], 900.0);
    assert_eq!(hw["overrides"]["simulatedRamTotal"], Value::Null);
    assert_eq!(hw["ram"]["installed"], 64u64 << 30);

    // Retour aux valeurs détectées : enregistré aussi.
    h.ok("set_hardware_overrides", json!({ "overrides": {} }));
    drop(h);
    let h = Harness::new(&dir);
    assert_eq!(
        h.ok("get_hardware", json!({}))["ram"]["channelsSource"],
        "estimated"
    );
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

    // « Réinitialiser » : valeurs du §12, sans toucher au fichier.
    let defaults = h.ok("get_default_settings", json!({}));
    assert_eq!(
        serde_json::from_value::<gram_core::Settings>(defaults.clone()).unwrap(),
        gram_core::Settings::default()
    );
    assert_eq!(defaults["runtime"], "ollama");
    assert_eq!(defaults["catalogTtlHours"], 24);
    assert_eq!(defaults["detailGroups"], 150);
    assert_eq!(h.ok("get_settings", json!({}))["runtime"], "lmStudio");
}

#[test]
fn cache_taille_et_vidage() {
    let dir = TempDir::new("cache");
    let h = Harness::new(&dir);
    let empty = h.ok("get_cache_info", json!({}));
    assert_eq!(
        (empty["files"].as_u64(), empty["bytes"].as_u64()),
        (Some(0), Some(0))
    );
    assert!(empty["path"].as_str().unwrap().ends_with("cache"));

    let details = dir.0.join("cache").join("details");
    std::fs::create_dir_all(&details).unwrap();
    std::fs::write(details.join("a--b@0123456789abcdef.json"), [b'x'; 1000]).unwrap();
    std::fs::write(dir.0.join("cache").join("catalog.json"), b"{}").unwrap();
    let info = h.ok("get_cache_info", json!({}));
    assert_eq!(
        (info["files"].as_u64(), info["bytes"].as_u64()),
        (Some(2), Some(1002))
    );

    let cleared = h.ok("clear_cache", json!({}));
    assert_eq!(cleared["files"], 0);
    assert!(!dir.0.join("cache").exists());
    // Les paramètres ne font pas partie du cache.
    h.ok(
        "save_settings",
        json!({ "settings": gram_core::Settings::default() }),
    );
    h.ok("clear_cache", json!({}));
    assert!(dir.0.join("settings.json").exists());
    // Le catalogue en mémoire reste utilisable.
    assert!(
        !h.ok("rank", json!({ "request": { "limit": 1 } }))["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
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
    let written = h.ok(
        "export_ranking",
        json!({ "format": "md", "path": md.to_string_lossy() }),
    );
    assert_eq!(written, json!(md.to_string_lossy()));
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

// ── Ollama : modèles installés, mesures et calibration (phase 7) ───────────────────────────

const QWEN: &str = "Qwen/Qwen3-8B-GGUF";

fn client(fake: &FakeOllama) -> Option<OllamaClient> {
    Some(OllamaClient::new(fake.url()).unwrap())
}

fn measure_args(label: &str) -> Value {
    json!({ "request": { "repoId": QWEN, "label": label, "useCase": "general" } })
}

#[test]
fn modeles_installes_dans_ollama() {
    let fake = FakeOllama::standard();
    let dir = TempDir::new("ollama-installes");
    let h = Harness::with_ollama(&dir, client(&fake));

    // Le modèle distant (`remote_host`) de la fixture est écarté.
    let rt = h.ok("detect_runtimes", json!({}));
    let models = rt["ollamaModels"].as_array().unwrap();
    assert_eq!(models.len(), 3);
    assert_eq!(models[0]["name"], "hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M");
    assert_eq!(models[0]["repoId"], QWEN);
    assert_eq!(models[2]["repoId"], Value::Null);

    // Classement : quantifications installées depuis un des dépôts du modèle.
    let r = h.ok("rank", json!({ "request": { "showTooBig": true } }));
    let rows: Vec<&Value> = ["items", "slow", "tooBig"]
        .iter()
        .flat_map(|k| r[k].as_array().unwrap())
        .collect();
    let find = |repo: &str| rows.iter().find(|i| i["repoId"] == repo).unwrap();
    assert_eq!(find(QWEN)["installed"], json!(["Q4_K_M"]));
    // MoE installé depuis le dépôt d'unsloth, alternative du dépôt canonique.
    assert_eq!(
        find("Qwen/Qwen3-30B-A3B-GGUF")["installed"],
        json!(["UD-Q4_K_XL"])
    );
    let installed = rows
        .iter()
        .filter(|i| !i["installed"].as_array().unwrap().is_empty())
        .count();
    assert_eq!(installed, 2);

    // Fiche : la quantification installée porte son nom dans Ollama.
    let d = h.ok("get_model_detail", json!({ "request": { "repoId": QWEN } }));
    let names: Vec<(&str, &str)> = d["variants"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| Some((v["variant"]["label"].as_str()?, v["ollamaModel"].as_str()?)))
        .collect();
    assert_eq!(names, [("Q4_K_M", "hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M")]);
    let q8 = d["variants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["variant"]["label"] == "Q8_0")
        .unwrap();
    assert_eq!(
        q8["ollamaPull"],
        "ollama pull hf.co/Qwen/Qwen3-8B-GGUF:Q8_0"
    );
}

#[test]
fn mesure_puis_calibration() {
    let fake = FakeOllama::standard();
    let dir = TempDir::new("ollama-mesure");
    {
        let h = Harness::with_ollama(&dir, client(&fake));
        let empty = h.ok("get_calibration", json!({}));
        assert_eq!(
            (empty["used"].as_u64(), empty["efficiency"].as_f64()),
            (Some(0), None)
        );

        let m = h.ok("ollama_measure", measure_args("Q4_K_M"));
        assert_eq!(m["ollamaModel"], "hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M");
        assert_eq!(m["ctx"], 8192);
        // 128 tokens en 1,542 s.
        let tok_s = m["tokS"].as_f64().unwrap();
        assert!((tok_s - 83.0).abs() < 0.1, "{tok_s}");
        assert_eq!(m["memTotal"], 6_925_000_000u64);
        assert_eq!(m["ctxLoaded"], 8192);
        assert_eq!(m["reliable"], true);
        assert_eq!(m["estimatedMode"]["kind"], "gpu");
        // RTX 5080 (960 Go/s), efficacité 0,55 : ~91 tok/s estimés, soit ~9 % de trop.
        let error = m["tokSError"].as_f64().unwrap();
        assert!((0.05..0.15).contains(&error), "{error}");
        assert!(m["notes"].as_array().unwrap().is_empty(), "{}", m["notes"]);
        // Ollama a reçu le contexte de l'estimation, au chargement comme à la mesure.
        let generate: Vec<_> = fake
            .requests()
            .into_iter()
            .filter(|r| r.path == "/api/generate")
            .collect();
        assert_eq!(generate.len(), 2);
        assert!(
            generate
                .iter()
                .all(|r| r.json()["options"]["num_ctx"] == 8192)
        );

        // 5,24 ms de lecture à pleine bande passante pour 12,05 ms mesurées, dont 1,5 ms fixes.
        let cal = h.ok("get_calibration", json!({}));
        assert_eq!(cal["used"], 1);
        assert_eq!(cal["measurements"][0]["label"], "Q4_K_M");
        assert_eq!(cal["efficiency"], json!(0.5));
        assert_eq!(cal["tokenOverheadMs"], Value::Null);
        assert_eq!(cal["changed"], true);

        let settings = h.ok("apply_calibration", json!({}));
        assert_eq!(settings["estimate"]["efficiency"], json!(0.5));
        assert_eq!(settings["estimate"]["tokenOverheadMs"], json!(1.5));
        assert_eq!(h.ok("get_settings", json!({})), settings);
        assert_eq!(h.ok("get_calibration", json!({}))["changed"], false);
        // L'estimation suit : la nouvelle valeur est proche de la mesure.
        let d = h.ok("get_model_detail", json!({ "request": { "repoId": QWEN } }));
        let q4 = d["variants"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["variant"]["label"] == "Q4_K_M")
            .unwrap();
        let estimated = q4["estimate"]["tokS"].as_f64().unwrap();
        assert!((estimated - tok_s).abs() / tok_s < 0.02, "{estimated}");
    }
    assert!(dir.0.join("measurements.json").exists());

    // Relues au prochain lancement ; effacer les mesures ne touche pas aux paramètres.
    let h = Harness::with_ollama(&dir, client(&fake));
    assert_eq!(h.ok("get_calibration", json!({}))["used"], 1);
    let cleared = h.ok("clear_measurements", json!({}));
    assert_eq!(cleared["measurements"], json!([]));
    assert_eq!(
        h.ok("get_settings", json!({}))["estimate"]["efficiency"],
        json!(0.5)
    );
}

#[test]
fn mesures_refusees() {
    let fake = FakeOllama::start(|r| {
        match r.path.as_str() {
        "/api/generate" => (
            500,
            r#"{"error":"model requires more system memory (12.1 GiB) than is available (9.8 GiB)"}"#
                .into(),
        ),
        _ => gram_ollama::testing::standard_response(r),
    }
    });
    let dir = TempDir::new("ollama-refus");
    let h = Harness::with_ollama(&dir, client(&fake));
    let err = |label: &str| h.invoke("ollama_measure", measure_args(label)).unwrap_err();

    assert_eq!(
        err("Q8_0"),
        "Q8_0 n'est pas installé dans Ollama. L'installer avec : \
         ollama pull hf.co/Qwen/Qwen3-8B-GGUF:Q8_0"
    );
    assert_eq!(err("Q9_X"), "Quantification Q9_X inconnue pour Qwen3-8B.");
    assert_eq!(
        h.invoke(
            "ollama_measure",
            json!({ "request": { "repoId": "x/absent", "label": "Q4_K_M" } })
        )
        .unwrap_err(),
        "Modèle « x/absent » absent du catalogue."
    );
    assert_eq!(
        err("Q4_K_M"),
        "Ollama a refusé la requête (HTTP 500) : model requires more system memory (12.1 GiB) \
         than is available (9.8 GiB)"
    );
    assert_eq!(
        h.invoke("apply_calibration", json!({})).unwrap_err(),
        "Aucune mesure exploitable : rien à appliquer."
    );
    assert!(!dir.0.join("measurements.json").exists());

    // Ollama arrêté.
    let addr = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let stopped = TempDir::new("ollama-arrete");
    let h = Harness::with_ollama(&stopped, Some(OllamaClient::for_addr(addr).unwrap()));
    assert_eq!(
        h.invoke("ollama_measure", measure_args("Q4_K_M"))
            .unwrap_err(),
        format!("Ollama ne répond pas sur http://{addr} : le lancer, puis réessayer")
    );

    // Adresse inutilisable (OLLAMA_HOST en HTTPS).
    let none = TempDir::new("ollama-aucun");
    let h = Harness::new(&none);
    let e = h
        .invoke("ollama_measure", measure_args("Q4_K_M"))
        .unwrap_err();
    assert!(
        e.as_str().unwrap().starts_with("API d'Ollama inaccessible"),
        "{e}"
    );
}
