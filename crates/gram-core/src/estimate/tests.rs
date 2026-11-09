use super::*;
use crate::hardware::{
    Backends, CpuInfo, GpuInfo, HardwareProfile, Overrides, RamInfo, RamKind, Source, Vendor,
};
use crate::model::{ArchInfo, ArchSource, Pipeline, UseCase};
use serde_json::json;

const KIB: u64 = 1 << 10;
const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;

fn arch(n_layers: u32, n_kv_heads: u32, head_dim: u32) -> ArchInfo {
    ArchInfo {
        architecture: "llama".into(),
        n_layers,
        n_heads: 32,
        n_kv_heads,
        head_dim,
        kv_layers: None,
        embedding_length: None,
        expert_count: None,
        expert_used_count: None,
        sliding_window: None,
        source: ArchSource::GgufHeader,
    }
}

/// Modèle dense de type Llama-3.1-8B : 32 couches, 8 têtes KV × 128 (128 Kio par token).
fn model(params_total: u64, arch: Option<ArchInfo>, ctx_train: u32) -> ModelEntry {
    ModelEntry {
        repo_id: "test/Model-GGUF".into(),
        sha: "abc".into(),
        base_model: None,
        display_name: "Model".into(),
        params_total,
        params_active: None,
        arch,
        ctx_train,
        pipeline: Pipeline::TextGeneration,
        use_cases: vec![UseCase::General],
        downloads: 0,
        likes: 0,
        created_at: None,
        last_modified: None,
        gated: false,
        variants: Vec::new(),
        mmproj_bytes: None,
        alternatives: Vec::new(),
    }
}

fn llama8b() -> ModelEntry {
    model(8_030_261_312, Some(arch(32, 8, 128)), 131_072)
}

fn variant(label: &str, size_bytes: u64, bpw: f32) -> QuantVariant {
    QuantVariant {
        label: label.into(),
        repo_id: "test/Model-GGUF".into(),
        files: vec![format!("model-{label}.gguf")],
        size_bytes,
        bpw,
        exact: true,
    }
}

fn budget(vram: u64, ram: u64) -> MemoryBudget {
    MemoryBudget {
        vram,
        ram,
        gpu_bandwidth_gbs: Some(360.0),
        ram_bandwidth_gbs: 51.2,
    }
}

fn cpu_only(ram: u64) -> MemoryBudget {
    MemoryBudget {
        vram: 0,
        ram,
        gpu_bandwidth_gbs: None,
        ram_bandwidth_gbs: 51.2,
    }
}

fn work(ctx: u32) -> Workload {
    Workload { ctx, vision: false }
}

/// 4 Gio de poids + 1 Gio de KV (8k) + 0,5 Gio de surcoût = 5,5 Gio.
fn run(vram_ram: MemoryBudget, ctx: u32) -> Estimate {
    estimate(
        &llama8b(),
        &variant("Q4_K_M", 4 * GIB, 4.3),
        &vram_ram,
        work(ctx),
        &EstimateParams::default(),
    )
}

// ── Cache KV (§8.1) ─────────────────────────────────────────────────────────────────────────

#[test]
fn kv_points_de_repere() {
    // Llama-3.1-8B : 128 Kio par token, 1 Gio pour 8 192 tokens.
    let llama8 = kv_bytes_per_token(&arch(32, 8, 128), KvCacheType::F16);
    assert_eq!(llama8, 128 * KIB);
    assert_eq!(llama8 * 8192, GIB);
    // Qwen3-8B : 144 Kio par token, 1,125 Gio pour 8 192 tokens.
    let qwen8 = kv_bytes_per_token(&arch(36, 8, 128), KvCacheType::F16);
    assert_eq!(qwen8, 144 * KIB);
    assert_eq!(qwen8 * 8192, GIB + GIB / 8);
    // Llama-3.1-70B : 320 Kio par token.
    assert_eq!(
        kv_bytes_per_token(&arch(80, 8, 128), KvCacheType::F16),
        320 * KIB
    );
}

#[test]
fn kv_seulement_les_couches_d_attention() {
    // Qwen3.5-4B : 8 couches d'attention sur 32 (les autres en attention linéaire).
    let mut a = arch(32, 4, 256);
    assert_eq!(kv_bytes_per_token(&a, KvCacheType::F16), 128 * KIB);
    a.kv_layers = Some(8);
    assert_eq!(kv_bytes_per_token(&a, KvCacheType::F16), 32 * KIB);
}

#[test]
fn kv_quantifie() {
    let a = arch(32, 8, 128); // 65 536 éléments par token
    assert_eq!(kv_bytes_per_token(&a, KvCacheType::Q8_0), 65_536 * 34 / 32);
    assert_eq!(kv_bytes_per_token(&a, KvCacheType::Q4_0), 65_536 * 18 / 32);
    assert!((KvCacheType::Q8_0.bytes_per_element() - 1.0625).abs() < 1e-9);
    assert!((KvCacheType::Q4_0.bytes_per_element() - 0.5625).abs() < 1e-9);
}

#[test]
fn kv_de_repli_prudent() {
    // (paramètres, KV réel f16) : Qwen3-0.6B, Llama-3.2-3B, Qwen3-4B, Llama-3.1-8B, Qwen3-8B,
    // Qwen3-14B, Qwen3-32B, Llama-3.3-70B.
    let real = [
        (0.6e9, 112),
        (3.2e9, 112),
        (4.0e9, 144),
        (8.03e9, 128),
        (8.19e9, 144),
        (14.8e9, 160),
        (32.8e9, 256),
        (70.6e9, 320),
    ];
    for (params, kib) in real {
        let fallback = fallback_kv_bytes_per_token(params, KvCacheType::F16);
        let real = kib * KIB;
        assert!(
            fallback >= real && fallback <= real * 3 / 2,
            "{params} : repli {fallback} pour {real} réels"
        );
    }
    let f16 = fallback_kv_bytes_per_token(8e9, KvCacheType::F16);
    let q8 = fallback_kv_bytes_per_token(8e9, KvCacheType::Q8_0);
    assert!(q8 < f16 && q8 > f16 / 2);
}

#[test]
fn couches_de_repli() {
    assert_eq!(fallback_layers(8_000_000_000), 34);
    assert!((75..=85).contains(&fallback_layers(70_600_000_000)));
    assert_eq!(fallback_layers(1_000_000_000), 16);
    assert_eq!(fallback_layers(0), 16);
}

// ── Mémoire utilisable (§6.5) ───────────────────────────────────────────────────────────────

fn gpu(vram_total: u64, vram_free: Option<u64>, integrated: bool) -> GpuInfo {
    GpuInfo {
        index: 0,
        name: "GPU de test".into(),
        vendor: Vendor::Nvidia,
        vram_total,
        vram_free,
        shared_mem: 16 * GIB,
        integrated,
        bandwidth_gbs: Some(360.0),
        bandwidth_source: Source::Table,
        cuda_cc: None,
    }
}

fn hw(gpu: Option<GpuInfo>, ram_total: u64, ram_available: u64) -> HardwareProfile {
    HardwareProfile {
        cpu: CpuInfo {
            name: "CPU de test".into(),
            cores_physical: 8,
            cores_logical: 16,
            max_clock_mhz: None,
            avx2: true,
            avx512: false,
            fma: true,
            f16c: true,
        },
        ram: RamInfo {
            total: ram_total,
            available: ram_available,
            installed: Some(ram_total),
            kind: RamKind::Ddr4,
            speed_mts: Some(3200),
            modules: 2,
            slots: Some(4),
            channels: 2,
            channels_source: Source::Estimated,
            bandwidth_gbs: Some(51.2),
        },
        gpu,
        other_gpus: vec![],
        backends: Backends::default(),
        disks: vec![],
        overrides: Overrides::default(),
        warnings: vec![],
    }
}

#[test]
fn vram_utilisable() {
    let p = EstimateParams::default();
    let cap = (12.0 * GIB as f64 * (1.0 - f64::from(p.vram_margin))) as u64;
    // VRAM presque libre : la marge de 10 % s'applique.
    let b = MemoryBudget::from_profile(&hw(Some(gpu(12 * GIB, Some(11 * GIB), false)), 0, 0), &p);
    assert_eq!(b.vram, cap);
    assert!(b.gpu_offload());
    // VRAM occupée par d'autres applications : VRAM libre + tolérance.
    let busy = hw(Some(gpu(12 * GIB, Some(9 * GIB), false)), 0, 0);
    assert_eq!(
        MemoryBudget::from_profile(&busy, &p).vram,
        9 * GIB + VRAM_FREE_TOLERANCE
    );
    // « Je ferme tout » : l'occupation actuelle est ignorée.
    let close = EstimateParams {
        close_apps: true,
        ..p.clone()
    };
    assert_eq!(MemoryBudget::from_profile(&busy, &close).vram, cap);
    // GPU simulé : VRAM libre inconnue.
    let sim = hw(Some(gpu(12 * GIB, None, false)), 0, 0);
    assert_eq!(MemoryBudget::from_profile(&sim, &p).vram, cap);
}

#[test]
fn gpu_integre_ou_absent() {
    let p = EstimateParams::default();
    for profile in [
        hw(Some(gpu(128 * MIB, None, true)), 16 * GIB, 8 * GIB),
        hw(None, 16 * GIB, 8 * GIB),
    ] {
        let b = MemoryBudget::from_profile(&profile, &p);
        assert_eq!(b.vram, 0);
        assert_eq!(b.gpu_bandwidth_gbs, None);
        assert!(!b.gpu_offload());
        assert_eq!(b.ram, 8 * GIB);
    }
}

#[test]
fn ram_utilisable() {
    let p = EstimateParams::default();
    let close = EstimateParams {
        close_apps: true,
        ..p.clone()
    };
    // min(RAM disponible, 75 % de la RAM totale).
    assert_eq!(
        MemoryBudget::from_profile(&hw(None, 32 * GIB, 20 * GIB), &p).ram,
        20 * GIB
    );
    assert_eq!(
        MemoryBudget::from_profile(&hw(None, 32 * GIB, 30 * GIB), &p).ram,
        24 * GIB
    );
    // « Je ferme tout » : RAM totale − 4 Go, sans jamais baisser.
    assert_eq!(
        MemoryBudget::from_profile(&hw(None, 32 * GIB, 20 * GIB), &close).ram,
        28 * GIB
    );
    assert_eq!(
        MemoryBudget::from_profile(&hw(None, 8 * GIB, 5 * GIB), &close).ram,
        5 * GIB
    );
}

#[test]
fn bandes_passantes_par_defaut() {
    let mut profile = hw(Some(gpu(8 * GIB, None, false)), 16 * GIB, 8 * GIB);
    profile.gpu.as_mut().unwrap().bandwidth_gbs = None;
    profile.ram.bandwidth_gbs = None;
    let b = MemoryBudget::from_profile(&profile, &EstimateParams::default());
    assert_eq!(b.gpu_bandwidth_gbs, Some(DEFAULT_GPU_BANDWIDTH_GBS));
    assert_eq!(b.ram_bandwidth_gbs, crate::DEFAULT_RAM_BANDWIDTH_GBS);
}

// ── Mode d'exécution (§8.2) ─────────────────────────────────────────────────────────────────

#[test]
fn exactement_la_vram_puis_un_octet_de_plus() {
    let need = 4 * GIB + GIB + 512 * MIB;
    let e = run(budget(need, 32 * GIB), 8192);
    assert_eq!(e.mode, RunMode::Gpu);
    assert_eq!((e.mem_gpu, e.mem_cpu), (need, 0));
    assert_eq!(e.mem_usage, 1.0);
    assert_eq!(e.fit, FitLevel::Marginal, "plus de 95 % de la VRAM");

    let e = run(budget(need - 1, 32 * GIB), 8192);
    assert_eq!(
        e.mode,
        RunMode::Hybrid {
            gpu_layers: 31,
            layers: 32
        }
    );
    // 31 couches sur 32 : 3,875 Gio de poids en VRAM, 128 Mio en RAM.
    assert_eq!(e.mem_gpu, GIB + 512 * MIB + 4 * GIB / 32 * 31);
    assert_eq!(e.mem_cpu, 4 * GIB / 32);
    assert_eq!(e.fit, FitLevel::Good);
}

#[test]
fn couches_gpu_en_hybride() {
    // VRAM 3,5 Gio : 1,5 Gio pour le KV et le surcoût, 2 Gio pour la moitié des poids.
    let e = run(budget(3 * GIB + 512 * MIB, 16 * GIB), 8192);
    assert_eq!(
        e.mode,
        RunMode::Hybrid {
            gpu_layers: 16,
            layers: 32
        }
    );
    assert_eq!(e.mem_gpu, 3 * GIB + 512 * MIB);
    assert_eq!(e.mem_cpu, 2 * GIB);
    assert_eq!(e.mem_gpu + e.mem_cpu, e.breakdown.total());
    assert_eq!(e.fit, FitLevel::Marginal, "moins de 75 % des couches");
    // Mémoire du mode : VRAM + RAM.
    let u = (5.5 / 19.5) as f32;
    assert!((e.mem_usage - u).abs() < 1e-6);
}

#[test]
fn hybride_sans_assez_de_ram() {
    // Il faudrait au moins 1,25 Gio de RAM, même au contexte minimal.
    let e = run(budget(3 * GIB + 512 * MIB, GIB), 8192);
    assert_eq!(e.mode, RunMode::TooBig);
    assert_eq!(e.fit, FitLevel::TooTight);
    assert_eq!((e.ctx_used, e.ctx_max), (2048, 0));
    assert_eq!((e.mem_gpu, e.mem_cpu, e.tok_s), (0, 0, 0.0));
    assert_eq!(e.breakdown.kv, GIB / 4);
    assert!(e.mem_usage > 1.0);
}

#[test]
fn cpu_sans_gpu() {
    let e = run(cpu_only(16 * GIB), 8192);
    assert_eq!(e.mode, RunMode::Cpu);
    assert_eq!((e.mem_gpu, e.mem_cpu), (0, 4 * GIB + GIB + 512 * MIB));
    assert_eq!(e.fit, FitLevel::Marginal);
    assert!(!e.approximate);
    assert!(e.warnings.is_empty(), "{:?}", e.warnings);
}

#[test]
fn gpu_trop_petit_pour_le_kv() {
    // 1 Gio de VRAM ne suffit pas au KV + surcoût : aucune couche sur le GPU, donc CPU.
    let e = run(budget(GIB, 16 * GIB), 8192);
    assert_eq!(e.mode, RunMode::Cpu);
}

// ── Contexte (§8.3) ─────────────────────────────────────────────────────────────────────────

#[test]
fn contexte_reduit_pour_tenir() {
    // 5 Gio de RAM : 8k ne tient pas (5,5 Gio), 4k tient tout juste (5 Gio).
    let e = run(cpu_only(5 * GIB), 8192);
    assert_eq!(e.mode, RunMode::Cpu);
    assert_eq!((e.ctx_used, e.ctx_max), (4096, 4096));
    assert_eq!(e.breakdown.kv, GIB / 2);
    assert_eq!(
        e.warnings,
        ["Contexte réduit à 4\u{a0}096 tokens pour tenir en mémoire."]
    );
}

#[test]
fn contexte_minimal_non_puissance_de_deux() {
    // 6 000 → 3 000 → 2 048 (contexte minimal), sans descendre en dessous.
    let ram = 4 * GIB + 512 * MIB + 2048 * 128 * KIB;
    let e = run(cpu_only(ram), 6000);
    assert_eq!(e.ctx_used, 2048);
    let e = run(cpu_only(ram - 1), 6000);
    assert_eq!((e.mode, e.ctx_used), (RunMode::TooBig, 2048));
}

#[test]
fn contexte_limite_par_le_modele() {
    let m = model(8_030_261_312, Some(arch(32, 8, 128)), 4096);
    let e = estimate(
        &m,
        &variant("Q4_K_M", 4 * GIB, 4.3),
        &budget(12 * GIB, 32 * GIB),
        work(16_384),
        &EstimateParams::default(),
    );
    assert_eq!((e.mode, e.ctx_used, e.ctx_max), (RunMode::Gpu, 4096, 4096));
    assert_eq!(
        e.warnings,
        ["Contexte limité à 4\u{a0}096 tokens par le modèle."]
    );
}

#[test]
fn contexte_maximal_par_mode() {
    // GPU : (12 − 4 − 0,5) Gio / 128 Kio.
    assert_eq!(run(budget(12 * GIB, 32 * GIB), 8192).ctx_max, 61_440);
    // Hybride : le KV doit laisser une couche (128 Mio) en VRAM : (3,5 − 0,5 − 0,125) Gio / 128 Kio.
    assert_eq!(
        run(budget(3 * GIB + 512 * MIB, 16 * GIB), 8192).ctx_max,
        23_552
    );
    // CPU : (16 − 4,5) Gio / 128 Kio.
    assert_eq!(run(cpu_only(16 * GIB), 8192).ctx_max, 94_208);
    // Plafonné au contexte d'entraînement.
    assert_eq!(run(budget(80 * GIB, 32 * GIB), 8192).ctx_max, 131_072);
}

#[test]
fn contexte_d_entrainement_inconnu() {
    let m = model(8_030_261_312, Some(arch(32, 8, 128)), 0);
    let e = estimate(
        &m,
        &variant("Q4_K_M", 4 * GIB, 4.3),
        &budget(80 * GIB, 32 * GIB),
        work(8192),
        &EstimateParams::default(),
    );
    assert_eq!(e.ctx_max, UNKNOWN_CTX_TRAIN_CAP);
    assert_eq!(e.warnings, ["Contexte d'entraînement inconnu."]);
}

// ── Vitesse (§8.4) ──────────────────────────────────────────────────────────────────────────

/// Paramètres par défaut, sans temps fixe par token : la formule de bande passante seule.
fn bandwidth_only() -> EstimateParams {
    EstimateParams {
        token_overhead_ms: 0.0,
        ..EstimateParams::default()
    }
}

#[test]
fn vitesse_limitee_par_la_bande_passante() {
    let b = budget(12 * GIB, 32 * GIB);
    let p = bandwidth_only();
    // 5 Go sur une RTX 3060 (360 Go/s) : ~40 tok/s ; sur DDR4-3200 double canal : ~5,6 tok/s.
    let gpu = tok_s(RunMode::Gpu, 5_000_000_000, 1.0, &b, &p);
    assert!((gpu - 39.6).abs() < 0.01, "{gpu}");
    let cpu = tok_s(RunMode::Cpu, 5_000_000_000, 1.0, &b, &p);
    assert!((cpu - 5.632).abs() < 0.001, "{cpu}");
    // Hybride, moitié des couches : 2,5 Go / 360 Go/s + 2,5 Go / 51,2 Go/s.
    let half = RunMode::Hybrid {
        gpu_layers: 16,
        layers: 32,
    };
    let hybrid = tok_s(half, 5_000_000_000, 1.0, &b, &p);
    let expected = 0.55 / (2.5 / 360.0 + 2.5 / 51.2);
    assert!((f64::from(hybrid) - expected).abs() < 0.01, "{hybrid}");
    // MoE : seuls les paramètres actifs (10 %) sont lus à chaque token.
    let moe = tok_s(RunMode::Gpu, 5_000_000_000, 0.1, &b, &p);
    assert!((moe - 396.0).abs() < 0.1, "{moe}");
    assert_eq!(tok_s(RunMode::TooBig, 5_000_000_000, 1.0, &b, &p), 0.0);
}

#[test]
fn temps_fixe_par_token() {
    let b = budget(12 * GIB, 32 * GIB);
    let p = EstimateParams::default();
    let at = |bytes: u64| f64::from(tok_s(RunMode::Gpu, bytes, 1.0, &b, &p));
    // 5 Go sur une RTX 3060 : 25,3 ms de lecture + 1,5 ms, soit ~37 tok/s au lieu de ~40.
    let expected = 1.0 / (5e9 / (360e9 * 0.55) + 0.0015);
    assert!((at(5_000_000_000) - expected).abs() < 0.01);
    assert!((36.0..38.0).contains(&expected));
    // Un token qui lit peu d'octets (MoE, petit modèle) bute sur le temps fixe :
    // 500 Mo → ~250 tok/s au lieu de 396, et jamais plus de 1000 / 1,5 ≈ 667 tok/s.
    let small = at(500_000_000);
    assert!((240.0..260.0).contains(&small), "{small}");
    assert!(at(1_000_000) < 667.0);
    // Sur CPU, la bande passante domine toujours : moins de 1 % d'écart pour 5 Go.
    let cpu = tok_s(RunMode::Cpu, 5_000_000_000, 1.0, &b, &p);
    assert!((cpu - 5.632).abs() / 5.632 < 0.01, "{cpu}");
    // Efficacité nulle (paramètre aberrant) : pas de vitesse infinie.
    let broken = EstimateParams {
        efficiency: 0.0,
        ..p.clone()
    };
    assert_eq!(tok_s(RunMode::Gpu, 5_000_000_000, 1.0, &b, &broken), 0.0);
}

#[test]
fn vitesse_moe_dans_l_estimation() {
    let mut m = model(30_532_122_624, Some(arch(48, 4, 128)), 40_960);
    let v = variant("Q4_K_M", 17 * GIB, 4.8);
    let b = budget(80 * GIB, 32 * GIB);
    for (p, min, max) in [
        (bandwidth_only(), 9.99, 10.01),
        (EstimateParams::default(), 5.0, 9.5),
    ] {
        let dense = estimate(&m, &v, &b, work(8192), &p);
        m.params_active = Some(3_053_212_262);
        let moe = estimate(&m, &v, &b, work(8192), &p);
        m.params_active = None;
        assert_eq!(
            moe.mem_gpu, dense.mem_gpu,
            "même placement qu'un modèle dense"
        );
        // 10 fois moins d'octets lus ; le temps fixe réduit l'écart.
        let ratio = moe.tok_s / dense.tok_s;
        assert!((min..=max).contains(&ratio), "{ratio}");
    }
}

// ── Adéquation (§9) ─────────────────────────────────────────────────────────────────────────

#[test]
fn niveaux_d_adequation() {
    use FitLevel::*;
    assert_eq!(fit_level(RunMode::Gpu, 0.8), Perfect);
    assert_eq!(fit_level(RunMode::Gpu, 0.81), Good);
    assert_eq!(fit_level(RunMode::Gpu, 0.95), Good);
    assert_eq!(fit_level(RunMode::Gpu, 0.96), Marginal);
    let hybrid = |gpu_layers| RunMode::Hybrid {
        gpu_layers,
        layers: 32,
    };
    assert_eq!(fit_level(hybrid(24), 0.5), Good, "75 % des couches");
    assert_eq!(fit_level(hybrid(23), 0.5), Marginal);
    assert_eq!(fit_level(RunMode::Cpu, 0.1), Marginal);
    assert_eq!(fit_level(RunMode::TooBig, 2.0), TooTight);
    assert!(Perfect < Good && Good < Marginal && Marginal < TooTight);
}

#[test]
fn gpu_avec_de_la_marge() {
    let e = run(budget(12 * GIB, 32 * GIB), 8192);
    assert_eq!(e.mode, RunMode::Gpu);
    assert!((e.mem_usage - 5.5 / 12.0).abs() < 1e-6);
    assert_eq!(e.fit, FitLevel::Perfect);
}

// ── Vision, repli, cas limites ──────────────────────────────────────────────────────────────

#[test]
fn vision_ajoute_le_projecteur() {
    let mut m = llama8b();
    m.mmproj_bytes = Some(800 * MIB);
    let v = variant("Q4_K_M", 4 * GIB, 4.3);
    let b = budget(12 * GIB, 32 * GIB);
    let p = EstimateParams::default();
    let text = estimate(&m, &v, &b, work(8192), &p);
    let vision = estimate(
        &m,
        &v,
        &b,
        Workload {
            ctx: 8192,
            vision: true,
        },
        &p,
    );
    assert_eq!(text.breakdown.vision, 0);
    assert_eq!(vision.breakdown.vision, 800 * MIB);
    assert_eq!(vision.mem_gpu, text.mem_gpu + 800 * MIB);
    assert!(vision.ctx_max < text.ctx_max);
}

#[test]
fn architecture_inconnue() {
    let m = model(8_030_261_312, None, 131_072);
    let e = estimate(
        &m,
        &variant("Q4_K_M", 4 * GIB, 4.3),
        &budget(12 * GIB, 32 * GIB),
        work(8192),
        &EstimateParams::default(),
    );
    assert!(e.approximate);
    assert_eq!(
        e.kv_per_token,
        fallback_kv_bytes_per_token(8_030_261_312.0, KvCacheType::F16)
    );
    assert!(e.warnings[0].starts_with("Architecture inconnue"));

    // Une architecture incomplète (0 tête KV) passe aussi par le repli.
    let m = model(8_030_261_312, Some(arch(32, 0, 128)), 131_072);
    let v = variant("Q4_K_M", 4 * GIB, 4.3);
    assert!(
        estimate(
            &m,
            &v,
            &budget(0, 32 * GIB),
            work(8192),
            &Default::default()
        )
        .approximate
    );
}

#[test]
fn attention_glissante_signalee() {
    let mut a = arch(48, 8, 256);
    a.sliding_window = Some(1024);
    let m = model(11_766_034_176, Some(a), 131_072);
    let e = estimate(
        &m,
        &variant("Q4_K_M", 7 * GIB, 4.9),
        &budget(12 * GIB, 32 * GIB),
        work(8192),
        &EstimateParams::default(),
    );
    assert!(!e.approximate);
    assert!(e.warnings[0].contains("fenêtre glissante"));
}

#[test]
fn variante_vide_sans_panique() {
    for b in [budget(GIB / 2, 16 * GIB), cpu_only(GIB / 4)] {
        let e = estimate(
            &llama8b(),
            &variant("Q4_K_M", 0, 0.0),
            &b,
            work(8192),
            &EstimateParams::default(),
        );
        assert!(e.tok_s.is_finite() && e.mem_usage.is_finite());
    }
}

#[test]
fn variantes_candidates() {
    let p = EstimateParams::default();
    let table = QuantTable::embedded();
    let mut m = llama8b();
    m.variants = vec![
        variant("IQ1_S", GIB, 1.8),
        variant("IQ2_XXS", 2 * GIB, 2.2),
        variant("Q2_K", 3 * GIB, 3.0),
        variant("Q4_K_M", 4 * GIB, 4.8),
        variant("EXOTIQUE", 5 * GIB, 0.0),
    ];
    let labels: Vec<String> = candidate_variants(&m, &p, table)
        .into_iter()
        .map(|v| v.label)
        .collect();
    assert_eq!(labels, ["Q2_K", "Q4_K_M", "EXOTIQUE"]);

    // Sans tailles réelles : échelle de repli, de la plus petite à la plus grosse.
    m.variants.clear();
    let fallback = candidate_variants(&m, &p, table);
    let labels: Vec<&str> = fallback.iter().map(|v| v.label.as_str()).collect();
    assert_eq!(
        labels,
        ["Q2_K", "Q3_K_M", "Q4_K_M", "Q5_K_M", "Q6_K", "Q8_0"]
    );
    let q4 = &fallback[2];
    assert!(!q4.exact && q4.files.is_empty());
    assert_eq!(q4.repo_id, m.repo_id);
    assert_eq!(
        q4.size_bytes,
        (8_030_261_312.0 * f64::from(4.9_f32) / 8.0).ceil() as u64
    );
    let e = estimate(&m, q4, &budget(12 * GIB, 32 * GIB), work(8192), &p);
    assert!(e.approximate);
    assert!(e.warnings[0].starts_with("Taille estimée"));

    // Minimum de bits par poids relevé : Q2_K (3,2) disparaît.
    let strict = EstimateParams {
        min_bpw: 3.5,
        ..p.clone()
    };
    assert_eq!(candidate_variants(&m, &strict, table).len(), 5);

    // Ni variantes ni nombre de paramètres : rien à estimer.
    m.params_total = 0;
    assert!(candidate_variants(&m, &p, table).is_empty());
}

#[test]
fn estimation_de_toutes_les_variantes() {
    let mut m = llama8b();
    m.variants = vec![
        variant("Q4_K_M", 4 * GIB, 4.8),
        variant("Q8_0", 8 * GIB, 8.5),
    ];
    let all = estimate_variants(
        &m,
        &budget(8 * GIB, 32 * GIB),
        work(8192),
        &EstimateParams::default(),
        QuantTable::embedded(),
    );
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].estimate.mode, RunMode::Gpu);
    assert!(matches!(all[1].estimate.mode, RunMode::Hybrid { .. }));
}

#[test]
fn table_des_quantifications() {
    let curve = r#""quality": [{ "bpw": 8, "factor": 1 }, { "bpw": 3, "factor": 0.8 }]"#;
    let table = |fallback: &str, quality: &str| {
        QuantTable::from_json(&format!(r#"{{ "fallback": {fallback}, {quality} }}"#))
    };
    let q4 = r#"[{ "label": "Q4_K_M", "bpw": 4.9 }]"#;
    assert!(table("[]", curve).is_err());
    assert!(table(r#"[{ "label": "Q4_K_M", "bpw": 0 }]"#, curve).is_err());
    assert!(table(q4, r#""quality": []"#).is_err());
    assert!(table(q4, r#""quality": [{ "bpw": 4, "factor": 1.5 }]"#).is_err());
    let t = table(q4, curve).unwrap();
    assert_eq!(t.fallback()[0].label, "Q4_K_M");
    // Points triés à la lecture, interpolation linéaire.
    assert!((t.quality_factor(5.5) - 0.9).abs() < 1e-6);
    let embedded = QuantTable::embedded().fallback();
    assert_eq!(embedded.first().unwrap().label, "Q8_0");
    assert!(embedded.windows(2).all(|w| w[0].bpw > w[1].bpw));
}

// ── Sérialisation ───────────────────────────────────────────────────────────────────────────

#[test]
fn format_json() {
    assert_eq!(
        serde_json::to_value(RunMode::Hybrid {
            gpu_layers: 20,
            layers: 32
        })
        .unwrap(),
        json!({ "kind": "hybrid", "gpuLayers": 20, "layers": 32 })
    );
    assert_eq!(
        serde_json::to_value(RunMode::TooBig).unwrap(),
        json!({ "kind": "tooBig" })
    );
    assert_eq!(serde_json::to_value(KvCacheType::Q8_0).unwrap(), "q8_0");
    assert_eq!(KvCacheType::Q4_0.label(), "q4_0");

    let p: EstimateParams = serde_json::from_str("{}").unwrap();
    assert_eq!(p, EstimateParams::default());
    let p: EstimateParams =
        serde_json::from_str(r#"{ "kvCache": "q4_0", "minCtx": 4096 }"#).unwrap();
    assert_eq!((p.kv_cache, p.min_ctx), (KvCacheType::Q4_0, 4096));

    let e = run(budget(12 * GIB, 32 * GIB), 8192);
    let back: Estimate = serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap();
    assert_eq!(back, e);
}

#[test]
fn nombres_de_tokens() {
    assert_eq!(tokens(512), "512");
    assert_eq!(tokens(8192), "8\u{a0}192");
    assert_eq!(tokens(131_072), "131\u{a0}072");
    assert_eq!(tokens(1_048_576), "1\u{a0}048\u{a0}576");
}
