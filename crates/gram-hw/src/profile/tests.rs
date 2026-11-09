use super::*;
use crate::raw::{RawBackends, RawMemoryModule, RawNvml};
use gram_core::RamKind;

const GIB: u64 = 1 << 30;

/// Dump réel (`gram-cli hw --raw`) : Ryzen 7 9800X3D, 2 × 32 Go DDR5-6000, RTX 5080,
/// iGPU Radeon avec 2 Gio réservés, écran virtuel Parsec (doublon DXGI de la RTX 5080).
fn dev_machine() -> RawHardware {
    serde_json::from_str(include_str!(
        "../../../../tests/fixtures/hardware/raw-9800x3d-rtx5080-parsec.json"
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
        },
        adapters,
        ..Default::default()
    }
}

#[test]
fn machine_de_developpement() {
    let hw = build(&dev_machine());

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
    };
    raw.nvml = Some(RawNvml::default());
    let hw = build(&raw);
    assert!(!hw.backends.cuda && hw.backends.vulkan);
    assert_eq!(hw.backends.cuda_driver_version, None);
}
