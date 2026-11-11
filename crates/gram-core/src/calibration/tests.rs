use super::*;
use crate::estimate::{Workload, estimate};
use crate::model::{ArchInfo, ArchSource, Pipeline};

const GIB: u64 = 1 << 30;

/// RTX 5080 16 Go + DDR5-6000 en double canal (machine de développement).
fn budget() -> MemoryBudget {
    MemoryBudget {
        vram: 14 * GIB + 300 * (1 << 20),
        ram: 47 * GIB,
        gpu_bandwidth_gbs: Some(960.0),
        ram_bandwidth_gbs: 96.0,
        unified: false,
    }
}

/// Qwen3-8B (36 couches, 8 têtes KV × 128) ; `active` pour un MoE.
fn model(params_total: u64, active: Option<u64>) -> ModelEntry {
    ModelEntry {
        repo_id: "Qwen/Qwen3-8B-GGUF".into(),
        sha: String::new(),
        base_model: None,
        display_name: "Qwen3-8B".into(),
        params_total,
        params_active: active,
        arch: Some(ArchInfo {
            architecture: "qwen3".into(),
            n_layers: 36,
            n_heads: 32,
            n_kv_heads: 8,
            head_dim: 128,
            kv_layers: None,
            embedding_length: None,
            expert_count: None,
            expert_used_count: None,
            sliding_window: None,
            source: ArchSource::GgufHeader,
        }),
        ctx_train: 40_960,
        pipeline: Pipeline::TextGeneration,
        use_cases: vec![UseCase::General],
        downloads: 0,
        likes: 0,
        created_at: None,
        last_modified: None,
        gated: false,
        variants: vec![],
        mmproj_bytes: None,
        alternatives: vec![],
    }
}

fn variant(size: u64) -> QuantVariant {
    QuantVariant {
        label: "Q4_K_M".into(),
        repo_id: "Qwen/Qwen3-8B-GGUF".into(),
        files: vec!["Qwen3-8B-Q4_K_M.gguf".into()],
        size_bytes: size,
        bpw: 4.9,
        exact: true,
    }
}

fn at() -> DateTime<Utc> {
    "2026-11-10T20:00:00Z".parse().unwrap()
}

/// 128 tokens à `tok_s`, modèle chargé tel quel.
fn run(tok_s: f64, loaded: Option<LoadedModel>) -> OllamaRun {
    OllamaRun {
        eval_count: 128,
        eval_duration_ns: (128.0 / tok_s * 1e9) as u64,
        prompt_eval_count: 43,
        prompt_eval_cached_count: 3,
        prompt_eval_duration_ns: 20_000_000,
        load_duration_ns: 2_500_000_000,
        loaded,
        others_loaded: 0,
    }
}

fn setup() -> (ModelEntry, QuantVariant, Estimate) {
    let m = model(8_190_000_000, None);
    let v = variant(5_027_783_488);
    let work = Workload {
        ctx: 8192,
        vision: false,
    };
    let e = estimate(&m, &v, &budget(), work, &EstimateParams::default());
    assert_eq!(e.mode, RunMode::Gpu);
    (m, v, e)
}

#[test]
fn mesure_face_a_l_estimation() {
    let (m, v, e) = setup();
    let total = e.breakdown.total();
    let loaded = LoadedModel {
        size: total + total / 10,
        size_vram: total + total / 10,
        context_length: Some(8192),
    };
    let r = measurement(
        &m,
        &v,
        &e,
        "hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M",
        &run(80.0, Some(loaded)),
        at(),
    );
    assert_eq!(r.ctx, 8192);
    assert!((r.tok_s - 80.0).abs() < 0.01, "{}", r.tok_s);
    assert_eq!(r.prompt_tok_s, Some(2000.0));
    assert_eq!(r.load_ms, 2500);
    assert_eq!(
        (r.mem_total, r.mem_vram),
        (Some(loaded.size), Some(loaded.size))
    );
    assert_eq!(r.ctx_loaded, Some(8192));
    let err = r.tok_s_error.unwrap();
    assert!((err - (e.tok_s - 80.0) / 80.0).abs() < 1e-4, "{err}");
    let mem_err = r.mem_error.unwrap();
    assert!((mem_err + 0.0909).abs() < 1e-3, "{mem_err}");
    assert_eq!(r.estimated_mode, RunMode::Gpu);
    assert_eq!(r.estimated_mem_total, total);
    assert_eq!(r.active_bytes, v.size_bytes);
    assert_eq!(r.gpu_share, 1.0);
    assert!(r.reliable);
    assert!(r.notes.is_empty(), "{:?}", r.notes);
    assert_eq!(r.measured_at, at());
}

#[test]
fn prompt_repris_du_cache() {
    let (m, v, e) = setup();
    // Mesure refaite pendant que le modèle est chargé : 44 tokens du prompt sur 45 viennent du
    // cache d'Ollama (réponse réelle d'Ollama 0.40.16).
    let mut r = run(140.0, None);
    (r.prompt_eval_count, r.prompt_eval_cached_count) = (45, 44);
    r.prompt_eval_duration_ns = 7_762_000;
    let out = measurement(&m, &v, &e, "x", &r, at());
    assert_eq!(out.prompt_tok_s, None);
    assert!(out.reliable, "la vitesse de génération reste valable");

    // Ancienne version d'Ollama, sans `prompt_eval_cached_count`.
    (r.prompt_eval_count, r.prompt_eval_cached_count) = (45, 0);
    r.prompt_eval_duration_ns = 18_000_000;
    let out = measurement(&m, &v, &e, "x", &r, at());
    assert_eq!(out.prompt_tok_s, Some(2500.0));
}

#[test]
fn remarques_sur_la_mesure() {
    let (m, v, e) = setup();
    let total = e.breakdown.total();
    // Une partie en RAM, contexte différent, mémoire bien plus haute, autre modèle chargé.
    let mut r = run(
        30.0,
        Some(LoadedModel {
            size: total * 2,
            size_vram: total,
            context_length: Some(32_768),
        }),
    );
    r.others_loaded = 2;
    let out = measurement(&m, &v, &e, "x", &r, at());
    assert_eq!(out.gpu_share, 0.5);
    let notes = out.notes.join("\n");
    assert!(
        notes.contains("en RAM alors que l'estimation tenait tout en VRAM"),
        "{notes}"
    );
    assert!(
        notes.contains("contexte de 32 768 tokens (demandé : 8 192)"),
        "{notes}"
    );
    assert!(notes.contains("OLLAMA_NUM_PARALLEL"), "{notes}");
    assert!(notes.contains("2 autres modèles chargés"), "{notes}");

    // Trop peu de tokens, mémoire non lue : la part GPU vient de l'estimation.
    let mut r = run(80.0, None);
    r.eval_count = 5;
    let out = measurement(&m, &v, &e, "x", &r, at());
    assert!(!out.reliable);
    assert_eq!((out.mem_total, out.mem_error), (None, None));
    assert_eq!(out.gpu_share, 1.0);
    assert!(out.notes[0].starts_with("Seulement 5 tokens"));
    assert!(out.notes[1].starts_with("Mémoire non lue"));

    // Rien généré : pas d'écart calculé.
    let mut r = run(80.0, None);
    r.eval_count = 0;
    let out = measurement(&m, &v, &e, "x", &r, at());
    assert_eq!((out.tok_s, out.tok_s_error), (0.0, None));
}

#[test]
fn moe_ne_lit_que_ses_parametres_actifs() {
    let m = model(30_500_000_000, Some(3_300_000_000));
    let v = variant(18 * GIB);
    let e = estimate(
        &m,
        &v,
        &budget(),
        Workload {
            ctx: 4096,
            vision: false,
        },
        &EstimateParams::default(),
    );
    let out = measurement(&m, &v, &e, "x", &run(100.0, None), at());
    let cost = crate::estimate::token_cost(&m, v.size_bytes);
    assert_eq!(
        (out.active_bytes, out.model_overhead_ms),
        (cost.bytes, cost.extra_ms)
    );
    assert!(out.active_bytes < v.size_bytes / 4);
    assert!(out.model_overhead_ms > 0.0);
    // Modèle dense : tous les poids, sans temps fixe propre.
    let (m, v, e) = setup();
    let out = measurement(&m, &v, &e, "x", &run(100.0, None), at());
    assert_eq!(
        (out.active_bytes, out.model_overhead_ms),
        (v.size_bytes, 0.0)
    );
}

#[test]
fn temps_fixe_propre_au_modele_retire() {
    // Un MoE mesuré sur une machine où l'efficacité GPU vaut 0,7 : son temps fixe propre est
    // retiré du temps mesuré, il ne fausse pas l'efficacité.
    let mut moe = synthetic(1_500_000_000, 1.0, 0.7, 0.6, 1.5 + 1.3);
    moe.model_overhead_ms = 1.3;
    let c = calibrate(vec![moe.clone()], &budget(), &before());
    assert_eq!(c.gpu_efficiency, Some(0.7));
    // Plus rapide que ne le permet son temps fixe propre : ignoré.
    moe.model_overhead_ms = 1000.0;
    assert_eq!(calibrate(vec![moe], &budget(), &before()).used, 0);
    // Mesure enregistrée avant ce champ : modèle dense.
    let mut json = serde_json::to_value(on_gpu(GIB, 0.7, 1.5)).unwrap();
    json.as_object_mut().unwrap().remove("modelOverheadMs");
    let old: Measurement = serde_json::from_value(json).unwrap();
    assert_eq!(old.model_overhead_ms, 0.0);
}

/// Mesure synthétique : vitesse donnée par la formule avec les efficacités et le temps fixe.
fn synthetic(active: u64, share: f32, gpu_eff: f64, ram_eff: f64, fixed_ms: f64) -> Measurement {
    let (m, v, e) = setup();
    let x = transfer_seconds(active, f64::from(share), &budget());
    let tok_s = 1.0 / (x.gpu / gpu_eff + x.ram / ram_eff + fixed_ms / 1000.0);
    let mut out = measurement(&m, &v, &e, &format!("m{active}"), &run(tok_s, None), at());
    out.active_bytes = active;
    out.gpu_share = share;
    out
}

/// Modèle entièrement en VRAM.
fn on_gpu(active: u64, eff: f64, fixed_ms: f64) -> Measurement {
    synthetic(active, 1.0, eff, 0.6, fixed_ms)
}

/// Réglages de départ des tests : les anciennes valeurs par défaut (0,55 partout et 1,5 ms).
fn before() -> EstimateParams {
    EstimateParams {
        gpu_efficiency: 0.55,
        ram_efficiency: 0.55,
        token_overhead_ms: 1.5,
        ..EstimateParams::default()
    }
}

#[test]
fn calibration_sur_une_mesure() {
    let p = before();
    let c = calibrate(vec![on_gpu(5 * GIB, 0.70, 1.5)], &budget(), &p);
    assert_eq!(c.used, 1);
    assert_eq!((c.gpu_efficiency, c.ram_efficiency), (Some(0.70), None));
    assert_eq!(c.token_overhead_ms, None);
    assert!(c.changed);
    assert_eq!(
        (
            c.current_gpu_efficiency,
            c.current_ram_efficiency,
            c.current_token_overhead_ms
        ),
        (0.55, 0.55, 1.5)
    );
    assert!(
        c.summary
            .starts_with("D'après 1 mesure : efficacité GPU 0,70 (actuelle 0,55)."),
        "{}",
        c.summary
    );
    assert!(
        c.summary
            .contains("Efficacité RAM inchangée : aucune mesure d'un modèle en partie en RAM."),
        "{}",
        c.summary
    );
    assert!(
        c.summary.contains("taille très différente"),
        "{}",
        c.summary
    );

    // Seule l'efficacité GPU change.
    let mut applied = p.clone();
    assert!(c.apply_to(&mut applied));
    assert_eq!(
        (
            applied.gpu_efficiency,
            applied.ram_efficiency,
            applied.token_overhead_ms
        ),
        (0.70, 0.55, 1.5)
    );
}

#[test]
fn calibration_mediane_de_plusieurs_mesures_proches() {
    let ms = vec![
        on_gpu(5 * GIB, 0.60, 1.5),
        on_gpu(6 * GIB, 0.70, 1.5),
        on_gpu(7 * GIB, 0.65, 1.5),
    ];
    let c = calibrate(ms, &budget(), &before());
    assert_eq!(
        (c.used, c.gpu_efficiency, c.token_overhead_ms),
        (3, Some(0.65), None)
    );
}

#[test]
fn calibration_des_efficacites_et_du_temps_fixe() {
    // Un modèle dense de 8 Go, un MoE qui lit 1,8 Go par token, un modèle hybride (40 % en
    // RAM) : tous mesurés sur une machine où l'efficacité vaut 0,8 sur GPU et 0,6 sur CPU, et
    // le temps fixe 1 ms.
    let ms = vec![
        synthetic(8 * GIB, 1.0, 0.8, 0.6, 1.0),
        synthetic(1_800_000_000, 1.0, 0.8, 0.6, 1.0),
        synthetic(20 * GIB, 0.6, 0.8, 0.6, 1.0),
    ];
    let p = before();
    let c = calibrate(ms, &budget(), &p);
    assert_eq!(
        (c.gpu_efficiency, c.ram_efficiency, c.token_overhead_ms),
        (Some(0.8), Some(0.6), Some(1.0))
    );
    assert!(c.changed);
    assert_eq!(
        c.summary,
        "D'après 3 mesures : efficacité GPU 0,80 (actuelle 0,55), efficacité RAM 0,60 \
         (actuelle 0,55), temps fixe par token 1,0 ms (actuel 1,5 ms)."
    );
    let mut applied = p.clone();
    c.apply_to(&mut applied);
    assert_eq!(
        (
            applied.gpu_efficiency,
            applied.ram_efficiency,
            applied.token_overhead_ms
        ),
        (0.8, 0.6, 1.0)
    );
}

#[test]
fn valeurs_par_defaut_tirees_des_mesures_reelles() {
    // Machine de développement, Ollama 0.40.16, contexte 8K : Qwen3-1.7B et Qwen3-8B en Q4_K_M
    // sur la RTX 5080, puis sur CPU seul (`num_gpu: 0`). Médianes de 3 à 5 essais.
    let real = |active: u64, share: f32, tok_s: f32| {
        let mut m = synthetic(active, share, 1.0, 1.0, 0.0);
        m.tok_s = tok_s;
        m
    };
    let (small, large) = (1_107_409_472, 5_027_783_488);
    let ms = vec![
        real(small, 1.0, 449.8),
        real(large, 1.0, 136.8),
        real(small, 0.0, 50.0),
        real(large, 0.0, 12.0),
    ];
    let c = calibrate(ms, &budget(), &before());
    let d = EstimateParams::default();
    assert_eq!(
        (c.gpu_efficiency, c.ram_efficiency, c.token_overhead_ms),
        (
            Some(d.gpu_efficiency),
            Some(d.ram_efficiency),
            Some(d.token_overhead_ms)
        )
    );
    // Avec ces valeurs, l'estimation de chaque mesure est à moins de 4 % de la réalité.
    let error = |m: &Measurement| {
        let x = transfer_seconds(m.active_bytes, f64::from(m.gpu_share), &budget());
        let estimated = f64::from(crate::estimate::tok_s_for(x, m.model_overhead_ms, &d));
        (estimated - f64::from(m.tok_s)) / f64::from(m.tok_s)
    };
    for m in &c.measurements {
        assert!(error(m).abs() < 0.04, "{} : {}", m.tok_s, error(m));
    }
    assert!(!calibrate(c.measurements.clone(), &budget(), &d).changed);

    // Ornith-1.5-35B-A3B en Q2_K (MoE, 3,5B actifs sur 35,5B) : 208 tok/s sur GPU, 26,7 sur
    // CPU seul. Avec ses termes propres, il s'accorde avec les modèles denses : à 2 % près, et
    // la calibration des six mesures garde les mêmes réglages.
    let mut ornith = model(35_505_251_456, Some(3_519_489_152));
    ornith.display_name = "Ornith-1.5-35B-A3B".into();
    let q2 = variant(13_085_779_616);
    let e = estimate(
        &ornith,
        &q2,
        &budget(),
        Workload {
            ctx: 8192,
            vision: false,
        },
        &d,
    );
    let moe = |share: f32, tok_s: f64| {
        let mut m = measurement(
            &ornith,
            &q2,
            &e,
            &format!("o{share}"),
            &run(tok_s, None),
            at(),
        );
        m.gpu_share = share;
        m
    };
    let (gpu, cpu) = (moe(1.0, 208.0), moe(0.0, 26.7));
    for m in [&gpu, &cpu] {
        assert!(error(m).abs() < 0.02, "{} : {}", m.tok_s, error(m));
    }
    let mut all = c.measurements;
    all.extend([gpu, cpu]);
    let c = calibrate(all, &budget(), &d);
    assert_eq!(c.used, 6);
    assert!(!c.changed, "{}", c.summary);
}

#[test]
fn mesure_en_ram_seulement() {
    // Un modèle hybride (70 % en RAM) : seule l'efficacité RAM est ajustée.
    let c = calibrate(
        vec![synthetic(20 * GIB, 0.3, 0.55, 0.6, 1.5)],
        &budget(),
        &before(),
    );
    assert_eq!((c.gpu_efficiency, c.ram_efficiency), (None, Some(0.6)));
    assert!(
        c.summary
            .contains("Efficacité GPU inchangée : aucune mesure d'un modèle tenant en VRAM."),
        "{}",
        c.summary
    );
    // La part en VRAM est lue à l'efficacité GPU actuelle (0,55 au lieu de 0,8) : l'erreur
    // retombe sur l'efficacité RAM, 0,63 au lieu de 0,6.
    let c = calibrate(
        vec![synthetic(20 * GIB, 0.6, 0.8, 0.6, 1.5)],
        &budget(),
        &before(),
    );
    assert_eq!(c.ram_efficiency, Some(0.63));
}

#[test]
fn temps_fixe_aberrant_ignore() {
    // Mesures incohérentes (le petit modèle plus lent que le gros) : pas d'ajustement du temps
    // fixe, l'efficacité seule.
    let mut slow_small = on_gpu(1_000_000_000, 0.7, 1.5);
    slow_small.tok_s = 20.0;
    let c = calibrate(
        vec![on_gpu(8 * GIB, 0.7, 1.5), slow_small],
        &budget(),
        &before(),
    );
    assert_eq!(c.token_overhead_ms, None);
    assert!(c.gpu_efficiency.is_some());
}

#[test]
fn deja_calibre() {
    let c = calibrate(vec![on_gpu(5 * GIB, 0.55, 1.5)], &budget(), &before());
    assert_eq!(c.gpu_efficiency, Some(0.55));
    assert!(!c.changed);
    assert!(
        c.summary
            .ends_with("Les réglages actuels correspondent déjà aux mesures."),
        "{}",
        c.summary
    );
}

#[test]
fn rien_a_calibrer() {
    let p = before();
    let c = calibrate(vec![], &budget(), &p);
    assert_eq!((c.used, c.gpu_efficiency, c.changed), (0, None, false));
    assert!(c.summary.starts_with("Aucune mesure exploitable"));
    assert!(!c.apply_to(&mut p.clone()));

    // Mesure peu fiable : ignorée.
    let mut m = on_gpu(5 * GIB, 0.7, 1.5);
    m.reliable = false;
    assert_eq!(calibrate(vec![m], &budget(), &p).used, 0);

    // Plus rapide que le plafond du temps fixe (1 000 / 1,5 ≈ 667 tok/s).
    let mut fast = on_gpu(100_000_000, 0.7, 1.5);
    fast.tok_s = 900.0;
    let c = calibrate(vec![fast.clone()], &budget(), &p);
    assert_eq!((c.gpu_efficiency, c.ram_efficiency), (None, None));
    assert!(
        c.summary.starts_with(
            "Les mesures sont plus rapides que ne le permet le temps fixe par token (1,5 ms)"
        ),
        "{}",
        c.summary
    );

    // Avec une mesure exploitable en RAM : celle-ci est calibrée, l'autre signalée.
    let hybrid = synthetic(20 * GIB, 0.3, 0.55, 0.6, 1.5);
    let c = calibrate(vec![fast, hybrid], &budget(), &p);
    assert_eq!((c.gpu_efficiency, c.ram_efficiency), (None, Some(0.6)));
    assert!(
        c.summary.contains("Certaines mesures sont plus rapides"),
        "{}",
        c.summary
    );
}

#[test]
fn efficacite_bornee() {
    let c = calibrate(vec![on_gpu(5 * GIB, 5.0, 1.5)], &budget(), &before());
    assert_eq!(c.gpu_efficiency, Some(2.0));
}

#[test]
fn sans_gpu_discret_tout_passe_par_la_ram() {
    let cpu = MemoryBudget {
        vram: 0,
        ram: 12 * GIB,
        gpu_bandwidth_gbs: None,
        ram_bandwidth_gbs: 51.2,
        unified: false,
    };
    // Une part « VRAM » mesurée (iGPU via Vulkan) ne change rien : même bande passante.
    let x = transfer_seconds(5 * GIB, 0.8, &cpu);
    assert_eq!(x, transfer_seconds(5 * GIB, 0.0, &cpu));
    assert_eq!(x.gpu, 0.0);

    // Seule l'efficacité RAM compte, et le résumé ne parle pas du GPU.
    let mut m = synthetic(5 * GIB, 0.0, 0.8, 0.6, 1.5);
    m.tok_s = (1.0 / (x.ram / 0.6 + 0.0015)) as f32;
    let c = calibrate(vec![m], &cpu, &before());
    assert_eq!((c.gpu_efficiency, c.ram_efficiency), (None, Some(0.6)));
    assert!(!c.summary.contains("GPU"), "{}", c.summary);
}

#[test]
fn historique_des_mesures() {
    let mut list = Vec::new();
    let mut a = on_gpu(GIB, 0.7, 1.5);
    a.ollama_model = "hf.co/a/b:Q4_K_M".into();
    record(&mut list, a.clone());
    let mut b = a.clone();
    b.ctx = 16_384;
    record(&mut list, b);
    assert_eq!(list.len(), 2);
    // Même modèle (casse différente) et même contexte : remplacé, et remonté en tête.
    let mut again = a.clone();
    again.ollama_model = "HF.CO/A/B:q4_k_m".into();
    again.tok_s = 99.0;
    record(&mut list, again);
    assert_eq!(list.len(), 2);
    assert_eq!((list[0].tok_s, list[0].ctx), (99.0, 8192));
    assert_eq!(list[1].ctx, 16_384);

    for i in 0..60 {
        let mut m = a.clone();
        m.ctx = 1000 + i;
        record(&mut list, m);
    }
    assert_eq!(list.len(), MAX_MEASUREMENTS);
    assert_eq!(list[0].ctx, 1059);
}

#[test]
fn format_json() {
    let (m, v, e) = setup();
    let out = measurement(&m, &v, &e, "x", &run(80.0, None), at());
    let json = serde_json::to_value(&out).unwrap();
    for key in [
        "ollamaModel",
        "tokS",
        "estimatedTokS",
        "tokSError",
        "memError",
        "gpuShare",
    ] {
        assert!(json.get(key).is_some(), "{key}");
    }
    assert_eq!(json["estimatedMode"]["kind"], "gpu");
    let back: Measurement = serde_json::from_value(json).unwrap();
    assert_eq!(back, out);
}
