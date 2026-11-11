use super::*;
use crate::raw::{RawBackends, RawMemory, RawMemoryModule, RawNvml};

const GIB: u64 = 1 << 30;

/// Dump réel (`gram-cli hw --raw`) : Ryzen 7 9800X3D, 2 × 32 Go DDR5-6000, RTX 5080,
/// iGPU Radeon avec 2 Gio réservés, écran virtuel Parsec (doublon DXGI de la RTX 5080).
fn dev_machine() -> RawHardware {
    serde_json::from_str(include_str!(
        "../../../../tests/fixtures/hardware/raw-9800x3d-rtx5080-parsec.json"
    ))
    .unwrap()
}

/// Dump réel (`gram-cli hw --raw`) : MacBook Pro M1 Pro 16 Go, GPU 16 cœurs, limite de Metal
/// de 11,84 Gio.
fn macbook_m1_pro() -> RawHardware {
    serde_json::from_str(include_str!(
        "../../../../tests/fixtures/hardware/raw-m1pro-16go-macos.json"
    ))
    .unwrap()
}

fn build(raw: &RawHardware) -> HardwareProfile {
    build_profile(raw, &Overrides::default())
}

fn module(mts: u32, smbios: u32, bank: &str) -> RawMemoryModule {
    RawMemoryModule {
        capacity: 8 * GIB,
        speed: Some(mts),
        configured_speed: Some(mts),
        smbios_type: Some(smbios),
        type_name: None,
        data_width: Some(64),
        device_locator: String::new(),
        bank_label: bank.into(),
    }
}

fn adapter(index: u32, name: &str, vendor: u32, vram: u64) -> RawAdapter {
    RawAdapter {
        index,
        name: name.into(),
        vendor_id: vendor,
        device_id: 0x1000 + index,
        sub_sys_id: 1,
        dedicated_video_memory: vram,
        shared_system_memory: 8 * GIB,
        budget: Some(vram * 9 / 10),
        ..Default::default()
    }
}

fn machine(modules: Vec<RawMemoryModule>, adapters: Vec<RawAdapter>) -> RawHardware {
    RawHardware {
        cpu: RawCpu {
            name: "CPU de test".into(),
            cores_physical: 8,
            cores_logical: 16,
            x86: true,
            avx2: true,
            ..Default::default()
        },
        memory: RawMemory {
            total: 31 * GIB,
            available: 20 * GIB,
            modules,
            slots: Some(4),
            type_name: None,
        },
        adapters,
        ..Default::default()
    }
}

#[test]
fn machine_de_developpement() {
    let hw = build(&dev_machine());

    // Dump antérieur à la prise en charge de macOS : sans `os`.
    assert_eq!(hw.os, Os::Windows);
    assert_eq!(hw.cpu.arch, CpuArch::X86);
    assert_eq!(hw.cpu.name, "AMD Ryzen 7 9800X3D 8-Core Processor");
    assert_eq!((hw.cpu.cores_physical, hw.cpu.cores_logical), (8, 16));
    assert!(hw.cpu.avx2 && hw.cpu.avx512);

    // Gestionnaire des tâches : 64 Go, 6000 MT/s, 2 emplacements utilisés sur 4.
    assert_eq!(hw.ram.installed, Some(64 * GIB));
    assert_eq!(hw.ram.kind, RamKind::Ddr5);
    assert_eq!(hw.ram.speed_mts, Some(6000));
    assert_eq!((hw.ram.modules, hw.ram.slots), (2, Some(4)));
    assert_eq!(
        (hw.ram.channels, hw.ram.channels_source),
        (2, Source::Estimated)
    );
    assert_eq!(hw.ram.bandwidth_gbs, Some(96.0));

    let gpu = hw.gpu.as_ref().unwrap();
    assert_eq!(gpu.name, "NVIDIA GeForce RTX 5080");
    assert_eq!(gpu.index, 0);
    assert!(!gpu.integrated);
    assert_eq!(gpu.vendor, Vendor::Nvidia);
    let nvml = &dev_machine().nvml.unwrap().devices[0];
    assert_eq!(gpu.vram_total, 17_094_934_528); // NVML, comme nvidia-smi (16 303 Mio)
    assert_eq!(gpu.vram_free, Some(nvml.memory_free));
    assert_eq!(gpu.cuda_cc, Some((12, 0)));
    assert_eq!(
        (gpu.bandwidth_gbs, gpu.bandwidth_source),
        (Some(960.0), Source::Table)
    );

    // Le doublon Parsec et le Basic Render Driver sont écartés ; l'iGPU reste pour information.
    assert_eq!(hw.other_gpus.len(), 1);
    let igpu = &hw.other_gpus[0];
    assert_eq!(igpu.name, "AMD Radeon(TM) Graphics");
    assert!(
        igpu.integrated,
        "DXCore signale l'iGPU malgré ses 2 Gio réservés"
    );
    assert_eq!(
        (igpu.bandwidth_gbs, igpu.bandwidth_source),
        (Some(96.0), Source::Estimated)
    );

    assert!(hw.backends.cuda && hw.backends.vulkan);
    assert_eq!(hw.backends.cuda_driver_version.as_deref(), Some("13.4"));
    assert!(hw.warnings.is_empty(), "{:?}", hw.warnings);
}

#[test]
fn sans_nvml_pas_de_plantage() {
    let mut raw = dev_machine();
    raw.nvml = None;
    let hw = build(&raw);
    let gpu = hw.gpu.unwrap();
    assert_eq!(gpu.name, "NVIDIA GeForce RTX 5080");
    assert_eq!(gpu.vram_total, 16_753_098_752); // DXGI
    assert_eq!(gpu.vram_free, Some(15_947_792_384)); // budget DXGI
    assert_eq!(gpu.cuda_cc, None);
    assert_eq!(gpu.bandwidth_source, Source::Table);
    assert_eq!(hw.backends.cuda_driver_version, None);
    assert!(hw.warnings.iter().any(|w| w.contains("NVML indisponible")));
}

#[test]
fn sans_dxcore_heuristiques_de_secours() {
    let mut raw = dev_machine();
    for a in &mut raw.adapters {
        a.integrated = None;
        a.hardware = None;
    }
    let hw = build(&raw);
    assert_eq!(hw.gpu.as_ref().unwrap().name, "NVIDIA GeForce RTX 5080");
    assert_eq!(hw.other_gpus.len(), 1, "doublon fusionné même sans DXCore");
    assert!(hw.other_gpus[0].integrated, "iGPU reconnu à son nom");
}

#[test]
fn bande_passante_nvml_si_absent_de_la_table() {
    let mut raw = dev_machine();
    for a in &mut raw.adapters {
        a.name = a.name.replace("RTX 5080", "RTX 5080 Prototype");
    }
    let gpu = build(&raw).gpu.unwrap();
    assert_eq!(gpu.bandwidth_source, Source::Nvml);
    assert!((gpu.bandwidth_gbs.unwrap() - 960.06).abs() < 0.01);
}

#[test]
fn p1_portable_igpu_seul() {
    let raw = machine(
        vec![
            module(3200, 26, "ChannelA-DIMM0"),
            module(3200, 26, "ChannelB-DIMM0"),
        ],
        vec![adapter(
            0,
            "Intel(R) Iris(R) Xe Graphics",
            gpu::VENDOR_INTEL,
            128 << 20,
        )],
    );
    let hw = build(&raw);
    assert_eq!(hw.ram.bandwidth_gbs, Some(51.2));
    let gpu = hw.gpu.unwrap();
    assert!(gpu.integrated);
    assert_eq!(
        (gpu.bandwidth_gbs, gpu.bandwidth_source),
        (Some(51.2), Source::Estimated)
    );
    assert!(hw.warnings.iter().any(|w| w.contains("GPU intégré")));
}

#[test]
fn p4_amd_sans_nvml() {
    let raw = machine(
        vec![
            module(6000, 34, "P0 CHANNEL A"),
            module(6000, 34, "P0 CHANNEL B"),
        ],
        vec![
            adapter(0, "AMD Radeon RX 7800 XT", gpu::VENDOR_AMD, 16 * GIB),
            adapter(1, "AMD Radeon(TM) Graphics", gpu::VENDOR_AMD, 512 << 20),
        ],
    );
    let hw = build(&raw);
    let gpu = hw.gpu.unwrap();
    assert_eq!(gpu.name, "AMD Radeon RX 7800 XT");
    assert_eq!(
        (gpu.bandwidth_gbs, gpu.bandwidth_source),
        (Some(624.0), Source::Table)
    );
    assert_eq!(gpu.vram_free, Some(16 * GIB * 9 / 10));
    assert!(hw.warnings.is_empty(), "{:?}", hw.warnings);
}

#[test]
fn portable_optimus_choisit_le_gpu_discret() {
    let raw = machine(
        vec![module(5600, 34, "")],
        vec![
            adapter(0, "Intel(R) UHD Graphics", gpu::VENDOR_INTEL, 128 << 20),
            adapter(
                1,
                "NVIDIA GeForce RTX 4060 Laptop GPU",
                gpu::VENDOR_NVIDIA,
                8 * GIB,
            ),
        ],
    );
    let hw = build(&raw);
    assert_eq!(hw.ram.channels, 1);
    let gpu = hw.gpu.unwrap();
    assert_eq!(gpu.index, 1);
    assert_eq!(gpu.bandwidth_gbs, Some(256.0));
    assert_eq!(hw.other_gpus[0].index, 0);
}

#[test]
fn gpu_inconnu_valeur_prudente() {
    let raw = machine(
        vec![module(3200, 26, "")],
        vec![adapter(0, "Moore Threads MTT S80", 0x1ED5, 16 * GIB)],
    );
    let hw = build(&raw);
    let gpu = hw.gpu.unwrap();
    assert_eq!(gpu.vendor, Vendor::Other);
    assert_eq!(
        (gpu.bandwidth_gbs, gpu.bandwidth_source),
        (Some(DEFAULT_GPU_BANDWIDTH_GBS), Source::Default)
    );
    assert!(hw.warnings.iter().any(|w| w.contains("inconnue")));
}

#[test]
fn machine_minimale_sans_gpu_ni_wmi() {
    let mut raw = machine(vec![], vec![]);
    raw.notes.push("WMI indisponible : test".into());
    let hw = build(&raw);
    assert!(hw.gpu.is_none() && hw.other_gpus.is_empty());
    assert_eq!(hw.ram.kind, RamKind::Unknown);
    assert_eq!(
        (hw.ram.channels, hw.ram.channels_source),
        (2, Source::Default)
    );
    assert_eq!((hw.ram.installed, hw.ram.bandwidth_gbs), (None, None));
    assert_eq!(hw.warnings[0], "WMI indisponible : test");
    assert!(hw.warnings.iter().any(|w| w.contains("Barrettes")));
    assert!(hw.warnings.iter().any(|w| w.contains("Aucun GPU")));
}

#[test]
fn avertissement_avx2_uniquement_sur_x86() {
    let mut raw = machine(vec![module(3200, 26, "")], vec![]);
    raw.cpu.avx2 = false;
    assert!(build(&raw).warnings.iter().any(|w| w.contains("AVX2")));
    raw.cpu.x86 = false;
    assert!(!build(&raw).warnings.iter().any(|w| w.contains("AVX2")));
}

#[test]
fn lpddr_signalee() {
    let mut m = module(6400, 35, "");
    m.data_width = Some(32);
    let raw = machine(vec![m; 4], vec![]);
    let hw = build(&raw);
    assert_eq!(hw.ram.channels, 2);
    assert!(hw.warnings.iter().any(|w| w.contains("LPDDR")));
}

#[test]
fn correction_des_canaux_ram() {
    let o = Overrides {
        ram_channels: Some(1),
        ..Default::default()
    };
    let hw = build_profile(&dev_machine(), &o);
    assert_eq!(
        (hw.ram.channels, hw.ram.channels_source),
        (1, Source::Manual)
    );
    assert_eq!(hw.ram.bandwidth_gbs, Some(48.0));
    // L'iGPU suit la bande passante de la RAM.
    assert_eq!(hw.other_gpus[0].bandwidth_gbs, Some(48.0));
    assert_eq!(hw.overrides, o);
}

#[test]
fn choix_manuel_du_gpu() {
    let o = Overrides {
        gpu_index: Some(1),
        ..Default::default()
    };
    let hw = build_profile(&dev_machine(), &o);
    assert_eq!(hw.gpu.as_ref().unwrap().name, "AMD Radeon(TM) Graphics");
    assert_eq!(hw.other_gpus[0].name, "NVIDIA GeForce RTX 5080");
    assert!(hw.warnings.iter().any(|w| w.contains("GPU intégré")));

    let o = Overrides {
        gpu_index: Some(7),
        ..Default::default()
    };
    let hw = build_profile(&dev_machine(), &o);
    assert_eq!(hw.gpu.unwrap().index, 0);
    assert!(hw.warnings.iter().any(|w| w.contains("n° 7 introuvable")));
}

#[test]
fn corrections_igpu_et_bande_passante() {
    let o = Overrides {
        gpu_integrated: Some(true),
        ..Default::default()
    };
    let gpu = build_profile(&dev_machine(), &o).gpu.unwrap();
    assert!(gpu.integrated);
    assert_eq!(
        (gpu.bandwidth_gbs, gpu.bandwidth_source),
        (Some(96.0), Source::Estimated)
    );

    let o = Overrides {
        gpu_bandwidth_gbs: Some(900.0),
        ..Default::default()
    };
    let gpu = build_profile(&dev_machine(), &o).gpu.unwrap();
    assert_eq!(
        (gpu.bandwidth_gbs, gpu.bandwidth_source),
        (Some(900.0), Source::Manual)
    );
}

#[test]
fn simulation_gpu_et_ram() {
    let raw = dev_machine();
    let o = Overrides {
        simulated_gpu: Some("NVIDIA GeForce RTX 3060 12 Go".into()),
        simulated_ram_total: Some(128 * GIB),
        gpu_bandwidth_gbs: Some(900.0), // correction du GPU réel : ignorée en simulation
        ..Default::default()
    };
    let hw = build_profile(&raw, &o);
    let gpu = hw.gpu.as_ref().unwrap();
    assert_eq!(gpu.name, "NVIDIA GeForce RTX 3060");
    assert_eq!(gpu.vram_total, 12 * GIB);
    assert_eq!(
        (gpu.bandwidth_gbs, gpu.bandwidth_source),
        (Some(360.0), Source::Table)
    );
    assert_eq!(gpu.vram_free, None);

    let used = raw.memory.total - raw.memory.available;
    assert_eq!(hw.ram.total, 128 * GIB);
    assert_eq!(hw.ram.installed, Some(128 * GIB));
    assert_eq!(hw.ram.available, 128 * GIB - used);
    assert!(hw.warnings.is_empty(), "{:?}", hw.warnings);
}

#[test]
fn simulation_sur_machine_sans_gpu() {
    let raw = machine(vec![module(3200, 26, "")], vec![]);
    let o = Overrides {
        simulated_gpu: Some("AMD Radeon RX 7900 XTX 24 Go".into()),
        ..Default::default()
    };
    let hw = build_profile(&raw, &o);
    assert_eq!(hw.gpu.unwrap().vendor, Vendor::Amd);
    assert!(!hw.warnings.iter().any(|w| w.contains("Aucun GPU")));
}

#[test]
fn simulation_inconnue_ignoree() {
    let o = Overrides {
        simulated_gpu: Some("GPU imaginaire 99 Go".into()),
        ..Default::default()
    };
    let hw = build_profile(&dev_machine(), &o);
    assert_eq!(hw.gpu.unwrap().name, "NVIDIA GeForce RTX 5080");
    assert!(hw.warnings.iter().any(|w| w.contains("simulation ignorée")));
}

#[test]
fn construction_idempotente() {
    let raw = dev_machine();
    let o = Overrides {
        ram_channels: Some(4),
        gpu_index: Some(1),
        ..Default::default()
    };
    assert_eq!(build_profile(&raw, &o), build_profile(&raw, &o));
    // Revenir aux valeurs détectées redonne le profil d'origine.
    assert_eq!(build_profile(&raw, &Overrides::default()), build(&raw));
}

#[test]
fn backends_sans_nvml() {
    let mut raw = machine(vec![], vec![]);
    raw.backends = RawBackends {
        nvcuda: false,
        vulkan: true,
        metal: false,
    };
    raw.nvml = Some(RawNvml::default());
    let hw = build(&raw);
    assert!(!hw.backends.cuda && hw.backends.vulkan);
    assert_eq!(hw.backends.cuda_driver_version, None);
}

// ── macOS ───────────────────────────────────────────────────────────────────────────────────

#[test]
fn mac_apple_silicon() {
    let raw = macbook_m1_pro();
    let hw = build(&raw);
    assert_eq!(hw.os, Os::MacOs);
    assert_eq!(hw.cpu.name, "Apple M1 Pro");
    assert_eq!(hw.cpu.arch, CpuArch::Arm);
    assert_eq!((hw.cpu.cores_physical, hw.cpu.cores_logical), (10, 10));
    assert!(!hw.cpu.avx2);

    // Mémoire unifiée : type, débit et bus de la table des puces (bus de 256 bits).
    assert_eq!(hw.ram.kind, RamKind::Lpddr5);
    assert_eq!(hw.ram.speed_mts, Some(6400));
    assert_eq!(
        (hw.ram.channels, hw.ram.channels_source),
        (4, Source::Table)
    );
    assert_eq!(hw.ram.bandwidth_gbs, Some(204.8));
    assert_eq!(hw.ram.installed, Some(16 * GIB));
    assert_eq!((hw.ram.modules, hw.ram.slots), (0, None));

    let gpu = hw.gpu.as_ref().unwrap();
    assert_eq!(gpu.name, "Apple M1 Pro");
    assert_eq!(gpu.vendor, Vendor::Apple);
    assert!(gpu.integrated && gpu.unified_memory && gpu.offload());
    assert_eq!(gpu.vram_total, 12_713_115_648, "limite de Metal");
    assert_eq!(gpu.vram_free, None);
    assert_eq!(
        (gpu.bandwidth_gbs, gpu.bandwidth_source),
        (Some(204.8), Source::Table)
    );
    assert!(hw.other_gpus.is_empty());
    assert!(hw.backends.metal && !hw.backends.cuda && !hw.backends.vulkan);
    assert_eq!(hw.disks.len(), 1, "volumes système de macOS écartés");
    assert!(hw.warnings.is_empty(), "{:?}", hw.warnings);
}

#[test]
fn mac_apple_silicon_budget_et_estimation() {
    use gram_core::{EstimateParams, MemoryBudget};
    let raw = macbook_m1_pro();
    let hw = build(&raw);
    let b = MemoryBudget::from_profile(&hw, &EstimateParams::default());
    assert!(b.unified && b.gpu_offload());
    // 90 % de la limite de Metal (10,66 Gio), sous les 12 Gio de RAM utilisables.
    assert_eq!(
        b.vram,
        (12_713_115_648f64 * (1.0 - f64::from(0.10f32))) as u64
    );
    assert_eq!(b.ram, raw.memory.available.min(12 * GIB));
    assert_eq!(
        (b.gpu_bandwidth_gbs, b.ram_bandwidth_gbs),
        (Some(204.8), 204.8)
    );
}

/// MacBook Pro 16 pouces 2019 : Core i9, 2 × 8 Go DDR4-2667, Radeon Pro 5500M + UHD 630.
fn intel_mac() -> RawHardware {
    let dimm = |bank: &str, locator: &str| RawMemoryModule {
        capacity: 8 * GIB,
        speed: Some(2667),
        type_name: Some("DDR4".into()),
        bank_label: bank.into(),
        device_locator: locator.into(),
        ..Default::default()
    };
    RawHardware {
        os: Os::MacOs,
        cpu: RawCpu {
            name: "Intel(R) Core(TM) i9-9880H CPU @ 2.30GHz".into(),
            cores_physical: 8,
            cores_logical: 16,
            x86: true,
            avx2: true,
            fma: true,
            f16c: true,
            ..Default::default()
        },
        memory: RawMemory {
            total: 16 * GIB,
            available: 9 * GIB,
            modules: vec![
                dimm("BANK 0", "ChannelA-DIMM0"),
                dimm("BANK 2", "ChannelB-DIMM0"),
            ],
            slots: Some(2),
            type_name: None,
        },
        metal: vec![
            RawMetalDevice {
                index: 0,
                name: "AMD Radeon Pro 5500M".into(),
                recommended_max_working_set: 4 * GIB,
                vram: Some(4 * GIB),
                ..Default::default()
            },
            RawMetalDevice {
                index: 1,
                name: "Intel(R) UHD Graphics 630".into(),
                recommended_max_working_set: 1536 << 20,
                unified_memory: true,
                low_power: true,
                vram: Some(1536 << 20),
                ..Default::default()
            },
        ],
        backends: RawBackends {
            metal: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn mac_intel_gpu_dedie_et_barrettes() {
    let hw = build(&intel_mac());
    assert_eq!(hw.cpu.arch, CpuArch::X86);
    assert_eq!(hw.ram.kind, RamKind::Ddr4);
    assert_eq!(hw.ram.speed_mts, Some(2667));
    assert_eq!(hw.ram.channels, 2, "ChannelA / ChannelB");
    assert_eq!(hw.ram.installed, Some(16 * GIB));

    let gpu = hw.gpu.as_ref().unwrap();
    assert_eq!((gpu.index, gpu.vendor), (0, Vendor::Amd));
    assert!(!gpu.integrated && !gpu.unified_memory);
    assert_eq!(gpu.vram_total, 4 * GIB);
    // Absent de la table : valeur prudente, signalée.
    assert_eq!(gpu.bandwidth_source, Source::Default);
    let igpu = &hw.other_gpus[0];
    assert_eq!(igpu.vendor, Vendor::Intel);
    assert!(igpu.integrated && !igpu.unified_memory && !igpu.offload());
    assert!(hw.warnings.iter().any(|w| w.contains("inconnue")));
    assert!(!hw.warnings.iter().any(|w| w.contains("NVML")));
}

#[test]
fn mac_intel_igpu_seul() {
    let mut raw = intel_mac();
    raw.metal.remove(0);
    raw.metal[0].index = 0;
    let hw = build(&raw);
    let gpu = hw.gpu.unwrap();
    assert!(gpu.integrated && !gpu.offload());
    assert!(
        hw.warnings
            .iter()
            .any(|w| w.contains("GPU intégré") && w.contains("Metal"))
    );
}

#[test]
fn versions_d_une_puce_par_coeurs_gpu() {
    let mut raw = macbook_m1_pro();
    raw.cpu.name = "Apple M3 Max".into();
    raw.metal[0].name = "Apple M3 Max".into();
    raw.metal[0].gpu_cores = Some(30);
    assert_eq!(build(&raw).ram.bandwidth_gbs, Some(307.2));
    raw.metal[0].gpu_cores = Some(40);
    let hw = build(&raw);
    assert_eq!(hw.ram.bandwidth_gbs, Some(409.6));
    assert_eq!(hw.gpu.unwrap().bandwidth_gbs, Some(409.6));
}

#[test]
fn puce_apple_inconnue() {
    let mut raw = macbook_m1_pro();
    raw.cpu.name = "Apple M7 Max".into();
    raw.metal[0].name = "Apple M7 Max".into();
    let hw = build(&raw);
    // La dernière puce connue de la gamme, signalée comme estimée.
    assert_eq!(hw.ram.channels_source, Source::Estimated);
    assert_eq!(hw.ram.kind, RamKind::Lpddr5x);
    let gpu = hw.gpu.unwrap();
    assert_eq!(gpu.bandwidth_source, Source::Estimated);
    assert!(gpu.bandwidth_gbs.unwrap() > 400.0);
    assert!(hw.warnings.iter().any(|w| w.contains("Apple M5 Max")));

    // Gamme inconnue : type lu par system_profiler, bande passante à corriger.
    raw.cpu.name = "Apple Z1".into();
    let hw = build(&raw);
    assert_eq!(hw.ram.kind, RamKind::Lpddr5);
    assert_eq!((hw.ram.speed_mts, hw.ram.bandwidth_gbs), (None, None));
    assert_eq!(hw.ram.installed, Some(16 * GIB));
    assert!(
        hw.warnings
            .iter()
            .any(|w| w.contains("absente de la table"))
    );
    assert!(!hw.warnings.iter().any(|w| w.contains("Barrettes")));
    assert_eq!(hw.gpu.unwrap().bandwidth_gbs, None);
}

#[test]
fn mac_corrections_et_simulation() {
    let raw = macbook_m1_pro();
    // « GPU intégré : oui » ne retire pas les couches d'un GPU à mémoire unifiée.
    let o = Overrides {
        gpu_integrated: Some(false),
        ..Default::default()
    };
    let gpu = build_profile(&raw, &o).gpu.unwrap();
    assert!(gpu.integrated && gpu.unified_memory);

    // Canaux corrigés : la bande passante du GPU suit celle de la mémoire.
    let o = Overrides {
        ram_channels: Some(2),
        ..Default::default()
    };
    let hw = build_profile(&raw, &o);
    assert_eq!(hw.ram.bandwidth_gbs, Some(102.4));
    assert_eq!(
        hw.gpu.map(|g| (g.bandwidth_gbs, g.bandwidth_source)),
        Some((Some(102.4), Source::Estimated))
    );

    // RAM simulée : la limite de Metal suit, dans la même proportion.
    let o = Overrides {
        simulated_ram_total: Some(64 * GIB),
        ..Default::default()
    };
    let hw = build_profile(&raw, &o);
    assert_eq!(hw.ram.total, 64 * GIB);
    assert_eq!(hw.gpu.unwrap().vram_total, 12_713_115_648 * 4);

    // GPU simulé : un GPU dédié remplace le GPU à mémoire unifiée.
    let o = Overrides {
        simulated_gpu: Some("NVIDIA GeForce RTX 4090 24 Go".into()),
        ..Default::default()
    };
    let gpu = build_profile(&raw, &o).gpu.unwrap();
    assert!(!gpu.unified_memory && !gpu.integrated);
    assert_eq!(gpu.vram_total, 24 * GIB);
}
