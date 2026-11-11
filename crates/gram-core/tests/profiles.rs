//! Profils matériels types (§15 du plan) face à des modèles réels du catalogue embarqué :
//! ordres de grandeur attendus au contexte 8k, avec les paramètres par défaut (§12).
//!
//! | Profil                                        | ~8B en Q4_K_M (~5 Go)     | ~70B en Q4_K_M (~42 Go)       |
//! |-----------------------------------------------|---------------------------|-------------------------------|
//! | P1 : portable iGPU, 16 Go DDR4-3200 ×2        | CPU, ~4–7 tok/s           | Trop juste                    |
//! | P2 : RTX 3060 12 Go + 32 Go DDR4-3600         | GPU complet, ~45–60 tok/s | Trop juste ou Limite          |
//! | P3 : RTX 4090 24 Go + 64 Go DDR5-6000         | GPU complet, rapide       | Hybride, ~2 tok/s             |
//! | P4 : RX 7800 XT 16 Go (sans NVML) + 32 Go DDR5 | GPU complet, ~70–110 tok/s | Trop juste ou Limite         |
//! | P5 : MacBook Pro M1 Pro, 16 Go unifiés        | GPU (Metal), ~25–40 tok/s | Trop juste                    |
//!
//! Vitesses revues après les mesures réelles (efficacité 0,81 sur GPU, 0,62 sur CPU, termes
//! propres aux MoE) et recoupées avec des vitesses publiées : ~50–57 tok/s pour un 8B en Q4_K_M
//! sur RTX 3060, ~118 tok/s pour Llama 2 7B en Q4_0 (3,8 Go) sur RX 7800 XT, soit ~95 pour 5 Go,
//! ~190 tok/s pour Qwen3-30B-A3B en Q4_K_M sur RTX 4090, ~36 tok/s pour Llama 2 7B en Q4_0 sur
//! M1 Pro (GPU 16 cœurs, llama.cpp), soit ~28 pour 5 Go.

use gram_core::estimate::{QuantTable, estimate};
use gram_core::{
    Estimate, EstimateParams, FitLevel, HardwareProfile, MemoryBudget, ModelEntry, RunMode,
    Workload, estimate_variants,
};

macro_rules! fixture {
    ($path:literal) => {
        serde_json::from_str(include_str!(concat!("../../../tests/fixtures/", $path))).expect($path)
    };
}

fn p1() -> HardwareProfile {
    fixture!("profiles/p1-portable-igpu-16go.json")
}
fn p2() -> HardwareProfile {
    fixture!("profiles/p2-rtx3060-12go-32go-ddr4.json")
}
fn p3() -> HardwareProfile {
    fixture!("profiles/p3-rtx4090-24go-64go-ddr5.json")
}
fn p4() -> HardwareProfile {
    fixture!("profiles/p4-rx7800xt-16go-32go-ddr5.json")
}
/// MacBook Pro M1 Pro 16 Go : limite de Metal de 11,84 Gio, 204,8 Go/s.
fn p5() -> HardwareProfile {
    fixture!("profiles/p5-m1pro-16go-macos.json")
}

/// Qwen3-8B (dense, 36 couches, 8 têtes KV × 128) : Q4_K_M = 4,68 Gio.
fn qwen3_8b() -> ModelEntry {
    fixture!("models/Qwen--Qwen3-8B-GGUF.json")
}
/// Llama-3.3-70B (dense, 80 couches, 8 têtes KV × 128) : Q4_K_M = 39,6 Gio.
fn llama_70b() -> ModelEntry {
    fixture!("models/unsloth--Llama-3.3-70B-Instruct-GGUF.json")
}
/// Ornith-1.5-35B-A3B (MoE 3,5B actifs, 10 couches d'attention sur 41, vision) : Q4_K_M = 20,2 Gio.
fn ornith_35b_a3b() -> ModelEntry {
    fixture!("models/ornith-ai--Ornith-1.5-35B-A3B-GGUF.json")
}
/// Qwen3-30B-A3B, pas encore détaillé : ni tailles réelles, ni architecture.
fn qwen3_30b_a3b() -> ModelEntry {
    fixture!("models/Qwen--Qwen3-30B-A3B-GGUF.json")
}

const CTX: Workload = Workload {
    ctx: 8192,
    vision: false,
};

fn run(model: &ModelEntry, label: &str, hw: &HardwareProfile, work: Workload) -> Estimate {
    let variant = model
        .variants
        .iter()
        .find(|v| v.label == label)
        .unwrap_or_else(|| panic!("{label} absent de {}", model.repo_id));
    let p = EstimateParams::default();
    estimate(
        model,
        variant,
        &MemoryBudget::from_profile(hw, &p),
        work,
        &p,
    )
}

fn q4(model: &ModelEntry, hw: &HardwareProfile) -> Estimate {
    run(model, "Q4_K_M", hw, CTX)
}

/// « Trop juste ou Limite » : ne tient pas, ou seulement en hybride / CPU.
fn too_tight_or_marginal(e: &Estimate) -> bool {
    matches!(e.fit, FitLevel::TooTight | FitLevel::Marginal) && e.mode != RunMode::Gpu
}

#[test]
fn p1_portable_igpu() {
    let hw = p1();
    let small = q4(&qwen3_8b(), &hw);
    assert_eq!(small.mode, RunMode::Cpu, "un iGPU compte comme du CPU");
    assert!((4.0..=7.0).contains(&small.tok_s), "{}", small.tok_s);
    assert_eq!(small.ctx_used, 8192);
    assert!(!small.approximate);

    let big = q4(&llama_70b(), &hw);
    assert_eq!(big.mode, RunMode::TooBig);
    assert_eq!(big.fit, FitLevel::TooTight);
}

#[test]
fn p2_rtx3060() {
    let hw = p2();
    let small = q4(&qwen3_8b(), &hw);
    assert_eq!(small.mode, RunMode::Gpu);
    assert!((45.0..=60.0).contains(&small.tok_s), "{}", small.tok_s);
    assert_eq!(small.fit, FitLevel::Perfect);
    // Contexte 8k : 1,125 Gio de cache KV (144 Kio par token).
    assert_eq!(small.breakdown.kv, (1 << 30) + (1 << 27));
    // (10,8 − 4,68 − 0,5) Gio / 144 Kio : 32k possibles en restant sur le GPU.
    assert!(
        (32_768..40_960).contains(&small.ctx_max),
        "{}",
        small.ctx_max
    );

    let big = q4(&llama_70b(), &hw);
    assert!(too_tight_or_marginal(&big), "{big:?}");
}

#[test]
fn p3_rtx4090() {
    let hw = p3();
    let small = q4(&qwen3_8b(), &hw);
    assert_eq!(small.mode, RunMode::Gpu);
    assert!(small.tok_s > 80.0, "{}", small.tok_s);
    assert_eq!(small.fit, FitLevel::Perfect);

    let big = q4(&llama_70b(), &hw);
    let RunMode::Hybrid { gpu_layers, layers } = big.mode else {
        panic!("hybride attendu : {big:?}");
    };
    assert_eq!(layers, 80);
    assert!((30..=45).contains(&gpu_layers), "{gpu_layers} couches GPU");
    assert!((1.5..=3.0).contains(&big.tok_s), "{}", big.tok_s);
    assert_eq!(big.fit, FitLevel::Marginal);
    assert_eq!(big.ctx_used, 8192);
}

#[test]
fn p4_rx7800xt_sans_nvml() {
    let hw = p4();
    assert!(!hw.backends.cuda);
    let small = q4(&qwen3_8b(), &hw);
    assert_eq!(small.mode, RunMode::Gpu);
    assert!((70.0..=110.0).contains(&small.tok_s), "{}", small.tok_s);
    assert_eq!(small.fit, FitLevel::Perfect);

    let big = q4(&llama_70b(), &hw);
    assert!(too_tight_or_marginal(&big), "{big:?}");
}

#[test]
fn llama_70b_en_quantification_agressive() {
    // UD-IQ2_M (2,75 bits par poids, 22,6 Gio) passe en hybride sur la RTX 3060.
    let e = run(&llama_70b(), "UD-IQ2_M", &p2(), CTX);
    assert!(matches!(e.mode, RunMode::Hybrid { .. }), "{e:?}");
    assert!(e.tok_s < 5.0, "section « lent » : {}", e.tok_s);
    // UD-IQ2_XXS (2,2 bits par poids) est sous le minimum : pas proposé.
    let p = EstimateParams::default();
    let all = estimate_variants(
        &llama_70b(),
        &MemoryBudget::from_profile(&p2(), &p),
        CTX,
        &p,
        QuantTable::embedded(),
    );
    assert!(all.iter().all(|v| v.variant.bpw >= p.min_bpw));
    assert!(all.iter().any(|v| v.variant.label == "UD-IQ2_M"));
    assert!(!all.iter().any(|v| v.variant.label == "UD-IQ2_XXS"));
}

#[test]
fn moe_hybride_reste_rapide() {
    let model = ornith_35b_a3b();
    let e = q4(&model, &p2());
    let RunMode::Hybrid { gpu_layers, layers } = e.mode else {
        panic!("hybride attendu : {e:?}");
    };
    assert_eq!(layers, 41);
    assert!(gpu_layers >= 15, "{gpu_layers}");
    // 10 couches d'attention sur 41, 2 têtes KV × 256 : 20 Kio par token.
    assert_eq!(e.kv_per_token, 20 << 10);
    // Seuls 3,5 milliards de paramètres sont lus par token : bien plus rapide qu'un modèle dense
    // de même taille en hybride (~21 tok/s).
    assert!((15.0..=40.0).contains(&e.tok_s), "{}", e.tok_s);

    // Usage vision : le projecteur (861 Mio) s'ajoute.
    let vision = run(
        &model,
        "Q4_K_M",
        &p2(),
        Workload {
            ctx: 8192,
            vision: true,
        },
    );
    assert_eq!(vision.breakdown.vision, model.mmproj_bytes.unwrap());
    assert_eq!(
        vision.breakdown.total(),
        e.breakdown.total() + model.mmproj_bytes.unwrap()
    );
}

#[test]
fn modele_non_detaille() {
    let model = qwen3_30b_a3b();
    assert!(model.variants.is_empty() && model.arch.is_none());
    let p = EstimateParams::default();
    let all = estimate_variants(
        &model,
        &MemoryBudget::from_profile(&p3(), &p),
        CTX,
        &p,
        QuantTable::embedded(),
    );
    assert_eq!(all.len(), 6, "échelle de repli");
    assert!(
        all.iter()
            .all(|v| v.estimate.approximate && !v.variant.exact)
    );
    let q4 = all.iter().find(|v| v.variant.label == "Q4_K_M").unwrap();
    // ~17,4 Gio estimés : tient sur la RTX 4090.
    assert_eq!(q4.estimate.mode, RunMode::Gpu);
    // Vitesses publiées pour ce modèle sur RTX 4090 : de l'ordre de 190 tok/s. Les termes propres
    // aux MoE, ajustés sur un autre MoE (Ornith-1.5-35B-A3B mesuré sur RTX 5080), donnent ~195 ;
    // sans eux, ~330.
    assert!(
        (150.0..=250.0).contains(&q4.estimate.tok_s),
        "MoE : {}",
        q4.estimate.tok_s
    );
    // Repli prudent : plus que le KV réel de Qwen3-30B-A3B (48 couches, 4 têtes KV × 128 = 96 Kio).
    assert!(q4.estimate.kv_per_token > 96 << 10);
}

#[test]
fn le_contexte_plus_long_coute_plus() {
    let hw = p2();
    let model = qwen3_8b();
    let short = run(&model, "Q4_K_M", &hw, CTX);
    let long = run(
        &model,
        "Q4_K_M",
        &hw,
        Workload {
            ctx: 32_768,
            vision: false,
        },
    );
    assert_eq!(long.breakdown.kv, short.breakdown.kv * 4);
    assert!(long.mem_usage > short.mem_usage);
    assert_eq!(long.ctx_max, short.ctx_max);
}

#[test]
fn p5_mac_apple_silicon() {
    let hw = p5();
    let b = MemoryBudget::from_profile(&hw, &EstimateParams::default());
    assert!(b.unified && b.gpu_offload());

    // Le GPU à mémoire unifiée reçoit le modèle (Metal), à la bande passante de la mémoire.
    let small = q4(&qwen3_8b(), &hw);
    assert_eq!(small.mode, RunMode::Gpu, "{small:?}");
    assert!((25.0..=40.0).contains(&small.tok_s), "{}", small.tok_s);
    assert_eq!(small.fit, FitLevel::Perfect);
    // Contexte maximal limité par la mémoire du GPU (10,7 Gio), pas par VRAM + RAM.
    assert!(
        (32_768..=40_960).contains(&small.ctx_max),
        "{}",
        small.ctx_max
    );

    // 16 Go en tout : un 70B ne tient nulle part, même en comptant le GPU.
    let big = q4(&llama_70b(), &hw);
    assert_eq!(big.mode, RunMode::TooBig);
    assert!(big.mem_usage > 3.0, "{}", big.mem_usage);
}
