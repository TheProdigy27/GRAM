//! Synchronisation de bout en bout sur des réponses Hugging Face réelles (aucun appel réseau).

use super::*;
use crate::api::HF_BASE;
use crate::http::Response;
use crate::test_support::{FakeTransport, TempDir, fixture};
use gram_core::{ArchSource, UseCase};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use url::Url;

/// Sous-ensemble d'une liste enregistrée, dans l'ordre donné.
fn subset(file: &str, ids: &[&str]) -> Vec<u8> {
    let all: Vec<serde_json::Value> =
        serde_json::from_slice(&fixture(&format!("list/{file}.json"))).unwrap();
    let picked: Vec<&serde_json::Value> = ids
        .iter()
        .map(|id| {
            all.iter()
                .find(|m| m["id"] == *id)
                .unwrap_or_else(|| panic!("{id} absent de {file}"))
        })
        .collect();
    serde_json::to_vec(&picked).unwrap()
}

fn fixture_path(rel: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/hf")
        .join(rel)
}

/// Routes : listes de la tranche 5–10B, détails et en-têtes enregistrés, sinon 404.
fn scenario() -> FakeTransport {
    let text_downloads = subset(
        "text-generation_5-10B_downloads",
        &[
            "ornith-ai/Ornith-1.0-9B-GGUF",
            "Qwen/Qwen3-8B-GGUF",
            "Qwen/Qwen2.5-7B-Instruct-GGUF",
            "bartowski/Qwen2.5-7B-Instruct-GGUF",
            "unsloth/Qwen3-8B-GGUF",
            "bartowski/Qwen_Qwen3-8B-GGUF",
        ],
    );
    // Dépôt peu téléchargé glissé dans une liste « téléchargements » : il doit être écarté.
    let mut text_downloads: Vec<serde_json::Value> =
        serde_json::from_slice(&text_downloads).unwrap();
    text_downloads.push(
        serde_json::from_slice::<Vec<serde_json::Value>>(&subset(
            "text-generation_5-10B_trendingScore",
            &["manuelgscom/askmdb-9b-GGUF"],
        ))
        .unwrap()
        .remove(0),
    );
    let text_downloads = serde_json::to_vec(&text_downloads).unwrap();
    let vision_downloads = subset(
        "image-text-to-text_5-10B_downloads",
        &["unsloth/Qwen3.5-9B-GGUF", "bartowski/Qwen_Qwen3.5-9B-GGUF"],
    );
    let text_trending = subset(
        "text-generation_5-10B_trendingScore",
        &[
            "Igor282/gemma-4-e4b-pruned-ru-en-Q4_K_M",
            "Qwen/Qwen3-8B-GGUF",
            "manuelgscom/dale-9b-GGUF",
        ],
    );

    FakeTransport::with_route(move |req| {
        let url = Url::parse(&req.url).unwrap();
        let segs: Vec<String> = url.path_segments().unwrap().map(percent_decode).collect();
        if url.path() == "/api/models" {
            let q: HashMap<String, String> = url.query_pairs().into_owned().collect();
            let key = (
                q["pipeline_tag"].as_str(),
                q["num_parameters"].as_str(),
                q["sort"].as_str(),
            );
            let body = match key {
                ("text-generation", "min:5B,max:10B", "downloads") => text_downloads.clone(),
                ("image-text-to-text", "min:5B,max:10B", "downloads") => vision_downloads.clone(),
                ("text-generation", "min:5B,max:10B", "trendingScore") => text_trending.clone(),
                _ => b"[]".to_vec(),
            };
            return Some(Response::ok(body));
        }
        if segs.len() == 4 && segs[0] == "api" && segs[1] == "models" {
            let rel = format!("details/{}--{}.json", segs[2], segs[3]);
            return fixture_path(&rel)
                .exists()
                .then(|| Response::ok(fixture(&rel)));
        }
        if segs.len() >= 5 && segs[2] == "resolve" {
            let file = segs[4..].join("/");
            if file == "config.json" {
                let rel = format!("config/{}--{}.json", segs[0], segs[1]);
                return fixture_path(&rel)
                    .exists()
                    .then(|| Response::ok(fixture(&rel)));
            }
            let stem = file.trim_end_matches(".gguf");
            let rel = format!("gguf/{}--{}--{stem}.bin", segs[0], segs[1]);
            if !fixture_path(&rel).exists() {
                return None;
            }
            let data = fixture(&rel);
            let end = req
                .range
                .as_ref()
                .map_or(data.len(), |r| (r.end as usize).min(data.len()));
            return Some(Response {
                status: 206,
                ..Response::ok(data[..end].to_vec())
            });
        }
        None
    })
}

fn percent_decode(s: &str) -> String {
    url::form_urlencoded::parse(format!("x={s}").as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default()
}

fn options() -> SyncOptions {
    SyncOptions {
        max_params: Some(10_000_000_000),
        detail_groups: 5,
        ..SyncOptions::default()
    }
}

fn model<'a>(c: &'a Catalog, repo: &str) -> &'a ModelEntry {
    c.models
        .iter()
        .find(|m| m.repo_id == repo)
        .unwrap_or_else(|| panic!("{repo} absent du catalogue"))
}

#[tokio::test(start_paused = true)]
async fn synchronisation_complete_puis_cache_puis_hors_ligne() {
    let dir = TempDir::new();
    let cache = Cache::new(dir.path());
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let hub = Hub::new(scenario()).on_event(move |e| sink.lock().unwrap().push(e));
    assert_eq!(hub.base(), HF_BASE);

    // 1. En ligne.
    let out = sync_with_seed(&hub, &cache, &options(), None)
        .await
        .unwrap();
    let s = &out.summary;
    let c = &out.catalog;
    assert_eq!(s.source.kind, DataSourceKind::Online);
    // 3 tranches (0–2B, 2–5B, 5–10B) × 2 pipelines × 2 tris, 5 dépôts canoniques, puis
    // 2 alternatives pour Qwen3-8B (dépôt officiel incomplet) : unsloth (absent, 404) et
    // bartowski.
    assert_eq!(s.api_calls, 12 + 5 + 2);
    // 3 en-têtes GGUF, 1 en-tête absent (404) puis son config.json.
    assert_eq!(s.resolver_calls, 5);
    assert_eq!((s.models, s.detailed, s.with_arch), (6, 4, 4));
    assert_eq!(s.repos, 10);
    assert_eq!(c.max_params, Some(10_000_000_000));
    assert_eq!(s.warnings.len(), 1, "{:?}", s.warnings);
    assert!(s.warnings[0].contains("Igor282"), "{:?}", s.warnings);

    // Pré-classement : deux par téléchargements cumulés, un par tendance. Qwen3.5-9B, premier
    // en tendance, est déjà pris : on passe au suivant, Qwen3-8B.
    let order: Vec<&str> = c.models.iter().map(|m| m.display_name.as_str()).collect();
    assert_eq!(
        order,
        vec![
            "Ornith-1.0-9B",
            "Qwen3.5-9B",
            "Qwen3-8B",
            "Qwen2.5-7B-Instruct",
            "gemma-4-e4b-pruned-ru-en",
            "dale-9b"
        ]
    );
    // Dépôt peu téléchargé et absent des tendances : écarté.
    assert!(!c.models.iter().any(|m| m.repo_id.contains("askmdb")));

    let qwen3 = model(c, "Qwen/Qwen3-8B-GGUF");
    let alternatives: Vec<&str> = qwen3
        .alternatives
        .iter()
        .map(|a| a.repo_id.as_str())
        .collect();
    assert_eq!(
        alternatives,
        vec!["unsloth/Qwen3-8B-GGUF", "bartowski/Qwen_Qwen3-8B-GGUF"]
    );
    assert_eq!(
        qwen3.alternatives[1].sha,
        "0b69f75b7472688e6808490aa2b85efdb81b5ce7"
    );
    assert_eq!(qwen3.base_model.as_deref(), Some("Qwen/Qwen3-8B"));
    // 5 quantifications officielles + 20 libellés que seul bartowski propose.
    assert_eq!(qwen3.variants.len(), 25);
    let smallest = &qwen3.variants[0];
    assert_eq!(
        (smallest.label.as_str(), smallest.repo_id.as_str()),
        ("IQ2_M", "bartowski/Qwen_Qwen3-8B-GGUF")
    );
    let q4 = qwen3.variants.iter().find(|v| v.label == "Q4_K_M").unwrap();
    assert_eq!(q4.repo_id, "Qwen/Qwen3-8B-GGUF");
    let arch = qwen3.arch.as_ref().unwrap();
    assert_eq!(arch.source, ArchSource::GgufHeader);
    assert_eq!(
        (arch.n_layers, arch.n_kv_heads, arch.head_dim),
        (36, 8, 128)
    );
    assert_eq!(qwen3.ctx_train, 40960);
    assert_eq!(qwen3.pipeline, Pipeline::TextGeneration);
    assert!(qwen3.downloads > 1_000_000);

    let q35 = model(c, "unsloth/Qwen3.5-9B-GGUF");
    assert_eq!(q35.pipeline, Pipeline::ImageTextToText);
    assert!(q35.use_cases.contains(&UseCase::Vision));
    assert_eq!(q35.mmproj_bytes, Some(918_166_080));
    assert_eq!(q35.arch.as_ref().unwrap().kv_layers, Some(8));

    // En-tête GGUF absent : repli sur le config.json du modèle de base.
    let q25 = model(c, "Qwen/Qwen2.5-7B-Instruct-GGUF");
    let arch = q25.arch.as_ref().unwrap();
    assert_eq!(arch.source, ArchSource::ConfigJson);
    assert_eq!(arch.n_layers, 28);

    let ornith = model(c, "ornith-ai/Ornith-1.0-9B-GGUF");
    assert_eq!(ornith.arch.as_ref().unwrap().architecture, "qwen35");
    assert_eq!(ornith.variants.len(), 5);

    // Dépôts en tendance non détaillés : dans le catalogue, sans tailles réelles.
    let dale = model(c, "manuelgscom/dale-9b-GGUF");
    assert!(dale.variants.is_empty() && dale.arch.is_none());

    // Cache écrit.
    assert!(dir.path().join("catalog.json").exists());
    assert_eq!(
        std::fs::read_dir(dir.path().join("details"))
            .unwrap()
            .count(),
        5
    );
    assert_eq!(
        std::fs::read_dir(dir.path().join("arch")).unwrap().count(),
        4
    );

    // Progression : 12 listes, 5 détails, deux passes d'alternatives, 4 architectures.
    let totals: Vec<(SyncPhase, u32)> = events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            HubEvent::Progress(p) if p.done == p.total => Some((p.phase, p.total)),
            _ => None,
        })
        .collect();
    assert_eq!(
        totals,
        vec![
            (SyncPhase::Listing, 12),
            (SyncPhase::Details, 5),
            (SyncPhase::Details, 1),
            (SyncPhase::Details, 1),
            (SyncPhase::Architecture, 4)
        ]
    );

    // 2. Catalogue récent en cache : aucun appel.
    let again = sync_with_seed(&hub, &cache, &options(), None)
        .await
        .unwrap();
    assert_eq!(again.summary.source.kind, DataSourceKind::Cache);
    assert_eq!(
        (again.summary.api_calls, again.summary.resolver_calls),
        (0, 0)
    );
    assert_eq!(again.catalog, out.catalog);

    // Le cache ne couvre pas une machine plus grosse : nouvelle synchronisation.
    let bigger = SyncOptions {
        max_params: Some(20_000_000_000),
        ..options()
    };
    hub.transport().offline.store(true, Ordering::Relaxed);
    // 3. Réseau coupé : repli sur le cache, avec un avertissement.
    let offline = sync_with_seed(&hub, &cache, &bigger, None).await.unwrap();
    assert_eq!(offline.summary.source.kind, DataSourceKind::Cache);
    assert_eq!(offline.catalog.models.len(), 6);
    assert!(offline.summary.warnings[0].contains("injoignable"));
    // Une seule requête tentée (plus une nouvelle tentative) : pas d'attente inutile.
    assert_eq!(offline.summary.api_calls, 2);

    // 4. Resynchronisation forcée : détails et architectures viennent du cache.
    hub.transport().offline.store(false, Ordering::Relaxed);
    let forced = SyncOptions {
        force: true,
        ..options()
    };
    let resync = sync_with_seed(&hub, &cache, &forced, None).await.unwrap();
    assert_eq!(resync.summary.source.kind, DataSourceKind::Online);
    // 12 listes + le détail toujours absent d'Igor282. Qwen3-8B est complet grâce au cache
    // de bartowski : unsloth n'est pas redemandé.
    assert_eq!(resync.summary.api_calls, 13);
    assert_eq!(resync.summary.resolver_calls, 0);
    assert_eq!(resync.summary.with_arch, 4);

    // 5. Mode hors ligne explicite.
    let off = SyncOptions {
        offline: true,
        ..options()
    };
    let o = sync_with_seed(&hub, &cache, &off, None).await.unwrap();
    assert_eq!(o.summary.source.kind, DataSourceKind::Cache);
    assert_eq!(o.summary.api_calls + o.summary.resolver_calls, 0);
}

#[tokio::test(start_paused = true)]
async fn hors_ligne_sans_cache() {
    let dir = TempDir::new();
    let cache = Cache::new(dir.path().join("vide"));
    let hub = Hub::new(FakeTransport::new());
    hub.transport().offline.store(true, Ordering::Relaxed);
    let opts = SyncOptions {
        force: true,
        ..options()
    };
    // Ni cache, ni catalogue embarqué.
    assert!(matches!(
        sync_with_seed(&hub, &cache, &opts, None).await,
        Err(HubError::NoCatalog)
    ));
    // Catalogue embarqué.
    let seed = Catalog {
        format: CATALOG_FORMAT,
        fetched_at: Utc::now(),
        max_params: None,
        models: vec![],
    };
    let out = sync_with_seed(&hub, &cache, &opts, Some(&seed))
        .await
        .unwrap();
    assert_eq!(out.summary.source.kind, DataSourceKind::Snapshot);
}

#[tokio::test(start_paused = true)]
async fn enrichissement_a_la_demande() {
    let dir = TempDir::new();
    let cache = Cache::new(dir.path());
    let hub = Hub::new(scenario());
    let out = sync_with_seed(&hub, &cache, &options(), None)
        .await
        .unwrap();
    // dale-9b n'a pas été détaillé ; son dépôt n'a pas de fixture : erreur 404 propagée.
    let dale = model(&out.catalog, "manuelgscom/dale-9b-GGUF").clone();
    assert!(matches!(
        enrich(&hub, &cache, &dale).await,
        Err(HubError::Http { status: 404, .. })
    ));
    // Un modèle déjà détaillé : tout vient du cache, sauf l'alternative unsloth, jamais
    // obtenue (404), que la fiche détail redemande.
    let before = hub.api_calls() + hub.resolver_calls();
    let qwen3 = model(&out.catalog, "Qwen/Qwen3-8B-GGUF");
    let stripped = ModelEntry {
        variants: vec![],
        arch: None,
        ..qwen3.clone()
    };
    let e = enrich(&hub, &cache, &stripped).await.unwrap();
    assert_eq!(&e, qwen3);
    assert_eq!(hub.api_calls() + hub.resolver_calls(), before + 1);
}

#[test]
fn borne_haute() {
    use gram_core::{CpuArch, CpuInfo, GpuInfo, Os, Overrides, RamInfo, RamKind, Source, Vendor};
    let gpu = |vram_gib: u64, integrated| GpuInfo {
        index: 0,
        name: "GPU".into(),
        vendor: Vendor::Nvidia,
        vram_total: vram_gib << 30,
        vram_free: None,
        shared_mem: 0,
        integrated,
        unified_memory: false,
        bandwidth_gbs: None,
        bandwidth_source: Source::Default,
        cuda_cc: None,
    };
    let hw = |gpu: Option<GpuInfo>, ram_gib: u64| HardwareProfile {
        os: Os::Windows,
        cpu: CpuInfo {
            name: "CPU".into(),
            arch: CpuArch::X86,
            cores_physical: 8,
            cores_logical: 16,
            max_clock_mhz: None,
            avx2: true,
            avx512: false,
            fma: true,
            f16c: true,
        },
        ram: RamInfo {
            total: ram_gib << 30,
            available: ram_gib << 29,
            installed: None,
            kind: RamKind::Ddr4,
            speed_mts: None,
            modules: 2,
            slots: None,
            channels: 2,
            channels_source: Source::Estimated,
            bandwidth_gbs: None,
        },
        gpu,
        other_gpus: vec![],
        backends: Default::default(),
        disks: vec![],
        overrides: Overrides::default(),
        warnings: vec![],
    };
    // 16 Go de RAM, iGPU (non compté) : ~32 milliards → tranches jusqu'à 20–40B.
    let small = max_params_for(&hw(Some(gpu(1, true)), 16));
    assert!((30_000_000_000..36_000_000_000).contains(&small), "{small}");
    assert_eq!(SizeBucket::up_to(Some(small)).len(), 5);
    // RTX 3060 12 Gio + 32 Gio : (10,8 + 24) Gio / 0,37 ≈ 101 milliards → toutes les tranches.
    let p2 = max_params_for(&hw(Some(gpu(12, false)), 32));
    assert!((95_000_000_000..105_000_000_000).contains(&p2), "{p2}");
    assert_eq!(SizeBucket::up_to(Some(p2)).len(), 7);
    // Mac à mémoire unifiée : la mémoire du GPU est prise sur la RAM, elle ne s'y ajoute pas.
    let mut apple = gpu(12, true);
    apple.unified_memory = true;
    assert_eq!(max_params_for(&hw(Some(apple), 16)), small);
}

#[test]
fn filtrage() {
    let repo = |json: &str, trending| ListedRepo {
        model: serde_json::from_str(json).unwrap(),
        trending,
        vision: false,
    };
    let opts = options();
    let ok = r#"{"id":"a/b","sha":"0123abcd","downloads":900,"gguf":{"total":8000000000}}"#;
    assert!(keep(&repo(ok, false), &opts));
    // Peu téléchargé : seulement s'il est en tendance.
    let rare = r#"{"id":"a/b","sha":"0123abcd","downloads":12,"gguf":{"total":8000000000}}"#;
    assert!(!keep(&repo(rare, false), &opts));
    assert!(keep(&repo(rare, true), &opts));
    // Au-dessus de la borne, sans paramètres, sans sha, identifiant invalide, brouillon.
    for bad in [
        r#"{"id":"a/b","sha":"0123abcd","downloads":900,"gguf":{"total":12000000000}}"#,
        r#"{"id":"a/b","sha":"0123abcd","downloads":900,"gguf":{"total":0}}"#,
        r#"{"id":"a/b","downloads":900,"gguf":{"total":8000000000}}"#,
        r#"{"id":"../b","sha":"0123abcd","downloads":900,"gguf":{"total":8000000000}}"#,
        r#"{"id":"a/b","sha":"0123abcd","downloads":900,"gguf":{"total":1500000000,"architecture":"dflash"}}"#,
    ] {
        assert!(!keep(&repo(bad, true), &opts), "{bad}");
    }
}

#[test]
fn requetes_de_liste() {
    let q = list_queries(&SyncOptions::default());
    // 7 tranches × 2 pipelines × 2 tris.
    assert_eq!(q.len(), 28);
    let q = list_queries(&options());
    assert_eq!(q.len(), 12);
    assert!(q.iter().all(|q| q.bucket.min_b < 10));
}
