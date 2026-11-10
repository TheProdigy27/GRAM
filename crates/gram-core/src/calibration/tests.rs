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
        prompt_eval_count: 40,
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
    let expected = (18 * GIB) as f64 * 3.3 / 30.5;
    assert!((out.active_bytes as f64 - expected).abs() < 2.0);
}

/// Mesure synthétique : vitesse donnée par la formule avec `eff` et `fixed_ms`.
fn synthetic(active: u64, share: f32, eff: f64, fixed_ms: f64) -> Measurement {
    let (m, v, e) = setup();
    let x = transfer_seconds(active, f64::from(share), &budget());
    let tok_s = 1.0 / (x / eff + fixed_ms / 1000.0);
    let mut out = measurement(&m, &v, &e, &format!("m{active}"), &run(tok_s, None), at());
    out.active_bytes = active;
    out.gpu_share = share;
    out
}

#[test]
fn calibration_sur_une_mesure() {
    let p = EstimateParams::default(); // 0,55 et 1,5 ms
    let c = calibrate(vec![synthetic(5 * GIB, 1.0, 0.70, 1.5)], &budget(), &p);
    assert_eq!(c.used, 1);
    assert_eq!(c.efficiency, Some(0.70));
    assert_eq!(c.token_overhead_ms, None);
    assert!(c.changed);
    assert_eq!(
        (c.current_efficiency, c.current_token_overhead_ms),
        (0.55, 1.5)
    );
    assert!(
        c.summary
            .starts_with("D'après 1 mesure : efficacité 0,70 (actuelle 0,55).")
    );
    assert!(
        c.summary.contains("taille très différente"),
        "{}",
        c.summary
    );

    let mut applied = p.clone();
    assert!(c.apply_to(&mut applied));
    assert_eq!((applied.efficiency, applied.token_overhead_ms), (0.70, 1.5));
}

#[test]
fn calibration_mediane_de_plusieurs_mesures_proches() {
    let p = EstimateParams::default();
    let ms = vec![
        synthetic(5 * GIB, 1.0, 0.60, 1.5),
        synthetic(6 * GIB, 1.0, 0.70, 1.5),
        synthetic(7 * GIB, 1.0, 0.65, 1.5),
    ];
    let c = calibrate(ms, &budget(), &p);
    assert_eq!(
        (c.used, c.efficiency, c.token_overhead_ms),
        (3, Some(0.65), None)
    );
}

#[test]
fn calibration_de_l_efficacite_et_du_temps_fixe() {
    // Un modèle dense de 8 Go, un MoE qui lit 1,8 Go par token, un modèle hybride : tous
    // mesurés sur une machine où l'efficacité vaut 0,7 et le temps fixe 2,2 ms.
    let p = EstimateParams::default();
    let ms = vec![
        synthetic(8 * GIB, 1.0, 0.7, 2.2),
        synthetic(1_800_000_000, 1.0, 0.7, 2.2),
        synthetic(20 * GIB, 0.6, 0.7, 2.2),
    ];
    let c = calibrate(ms, &budget(), &p);
    assert_eq!(c.efficiency, Some(0.7));
    assert_eq!(c.token_overhead_ms, Some(2.2));
    assert!(c.changed);
    assert!(
        c.summary.contains(
            "efficacité 0,70 (actuelle 0,55), temps fixe par token 2,2 ms (actuel 1,5 ms)"
        ),
        "{}",
        c.summary
    );
    let mut applied = p.clone();
    c.apply_to(&mut applied);
    assert_eq!((applied.efficiency, applied.token_overhead_ms), (0.7, 2.2));
}

#[test]
fn temps_fixe_aberrant_ignore() {
    // Mesures incohérentes (le petit modèle plus lent que le gros) : pas d'ajustement du temps
    // fixe, l'efficacité seule.
    let p = EstimateParams::default();
    let mut slow_small = synthetic(1_000_000_000, 1.0, 0.7, 1.5);
    slow_small.tok_s = 20.0;
    let c = calibrate(
        vec![synthetic(8 * GIB, 1.0, 0.7, 1.5), slow_small],
        &budget(),
        &p,
    );
    assert_eq!(c.token_overhead_ms, None);
    assert!(c.efficiency.is_some());
}

#[test]
fn deja_calibre() {
    let p = EstimateParams::default();
    let c = calibrate(vec![synthetic(5 * GIB, 1.0, 0.55, 1.5)], &budget(), &p);
    assert_eq!(c.efficiency, Some(0.55));
    assert!(!c.changed);
    assert!(
        c.summary
            .ends_with("Les réglages actuels correspondent déjà aux mesures.")
    );
}

#[test]
fn rien_a_calibrer() {
    let p = EstimateParams::default();
    let c = calibrate(vec![], &budget(), &p);
    assert_eq!((c.used, c.efficiency, c.changed), (0, None, false));
    assert!(c.summary.starts_with("Aucune mesure exploitable"));
    assert!(!c.apply_to(&mut p.clone()));

    // Mesure peu fiable : ignorée.
    let mut m = synthetic(5 * GIB, 1.0, 0.7, 1.5);
    m.reliable = false;
    assert_eq!(calibrate(vec![m], &budget(), &p).used, 0);

    // Plus rapide que le plafond du temps fixe (1 000 / 1,5 ≈ 667 tok/s).
    let mut fast = synthetic(100_000_000, 1.0, 0.7, 1.5);
    fast.tok_s = 900.0;
    let c = calibrate(vec![fast], &budget(), &p);
    assert_eq!(c.efficiency, None);
    assert!(
        c.summary.contains("temps fixe par token (1,5 ms)"),
        "{}",
        c.summary
    );
}

#[test]
fn efficacite_bornee() {
    let p = EstimateParams::default();
    let c = calibrate(vec![synthetic(5 * GIB, 1.0, 5.0, 1.5)], &budget(), &p);
    assert_eq!(c.efficiency, Some(2.0));
}

#[test]
fn sans_gpu_discret_tout_passe_par_la_ram() {
    let cpu = MemoryBudget {
        vram: 0,
        ram: 12 * GIB,
        gpu_bandwidth_gbs: None,
        ram_bandwidth_gbs: 51.2,
    };
    // Une part « VRAM » mesurée (iGPU via Vulkan) ne change rien : même bande passante.
    assert_eq!(
        transfer_seconds(5 * GIB, 0.8, &cpu),
        transfer_seconds(5 * GIB, 0.0, &cpu)
    );
}

#[test]
fn historique_des_mesures() {
    let mut list = Vec::new();
    let mut a = synthetic(GIB, 1.0, 0.7, 1.5);
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
