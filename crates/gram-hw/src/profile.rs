//! Données brutes + corrections manuelles → [`HardwareProfile`]. Sans I/O.

use crate::gpu;
use crate::ram;
use crate::raw::{RawAdapter, RawCpu, RawHardware, RawMemory, RawNvmlDevice};
use crate::specs::GpuSpecs;
use gram_core::{Backends, CpuInfo, GpuInfo, HardwareProfile, Overrides, RamInfo, Source, Vendor};

/// Bande passante prudente d'un GPU discret absent de la table et sans NVML (§6.3).
pub const DEFAULT_GPU_BANDWIDTH_GBS: f32 = 200.0;

/// Construit le profil avec la table GPU embarquée.
pub fn build_profile(raw: &RawHardware, overrides: &Overrides) -> HardwareProfile {
    build_profile_with(raw, overrides, GpuSpecs::embedded())
}

/// Construit le profil. Les corrections (`overrides`) sont des entrées comme les autres :
/// on repart toujours des données brutes, ce qui rend l'opération idempotente.
pub fn build_profile_with(
    raw: &RawHardware,
    overrides: &Overrides,
    specs: &GpuSpecs,
) -> HardwareProfile {
    let mut warnings = raw.notes.clone();
    let cpu = build_cpu(&raw.cpu, &mut warnings);
    let ram = build_ram(&raw.memory, overrides, &mut warnings);
    let (gpu, other_gpus) = build_gpus(raw, &ram, overrides, specs, &mut warnings);
    HardwareProfile {
        cpu,
        ram,
        gpu,
        other_gpus,
        backends: Backends {
            cuda: raw.backends.nvcuda,
            cuda_driver_version: raw
                .nvml
                .as_ref()
                .and_then(|n| n.cuda_driver_version.clone()),
            vulkan: raw.backends.vulkan,
        },
        disks: raw.disks.clone(),
        overrides: overrides.clone(),
        warnings,
    }
}

fn build_cpu(c: &RawCpu, warnings: &mut Vec<String>) -> CpuInfo {
    if c.x86 && !c.avx2 {
        warnings.push(
            "Processeur sans AVX2 : la plupart des builds de llama.cpp (et donc d'Ollama) \
             le supposent ; l'exécution sur CPU risque d'échouer ou d'être très lente."
                .into(),
        );
    }
    CpuInfo {
        name: c.name.trim().to_string(),
        cores_physical: c.cores_physical,
        cores_logical: c.cores_logical,
        max_clock_mhz: c.max_clock_mhz,
        avx2: c.avx2,
        avx512: c.avx512,
        fma: c.fma,
        f16c: c.f16c,
    }
}

fn build_ram(m: &RawMemory, o: &Overrides, warnings: &mut Vec<String>) -> RamInfo {
    let kind = ram::kind_of(&m.modules);
    let speed_mts = ram::speed_of(kind, &m.modules);
    let (mut channels, mut channels_source) = ram::estimate_channels(kind, &m.modules);

    if let Some(c) = o.ram_channels.filter(|&c| c > 0) {
        channels = c;
        channels_source = Source::Manual;
    } else if m.modules.is_empty() {
        warnings.push(
            "Barrettes mémoire non détectées : type, vitesse et nombre de canaux inconnus \
             (à corriger à la main)."
                .into(),
        );
    } else if ram::is_lpddr(kind) {
        warnings.push(
            "Mémoire LPDDR soudée : le nombre de canaux est souvent mal décrit par le BIOS, \
             à vérifier."
                .into(),
        );
    }
    if speed_mts.is_none() && !m.modules.is_empty() {
        warnings.push("Vitesse de la RAM inconnue : bande passante non calculée.".into());
    }

    let mut installed = (!m.modules.is_empty()).then(|| m.modules.iter().map(|x| x.capacity).sum());
    let (mut total, mut available) = (m.total, m.available);
    if let Some(sim) = o.simulated_ram_total.filter(|&t| t > 0) {
        // Simulation : même consommation actuelle, sur une RAM plus grande (ou plus petite).
        let used = total.saturating_sub(available);
        total = sim;
        available = sim.saturating_sub(used);
        installed = Some(sim);
    }

    RamInfo {
        total,
        available,
        installed,
        kind,
        speed_mts,
        modules: m.modules.len() as u32,
        slots: m.slots,
        channels,
        channels_source,
        bandwidth_gbs: speed_mts.map(|s| ram::bandwidth_gbs(s, channels)),
    }
}

/// Un GPU en cours d'assemblage, avec la bande passante NVML gardée de côté.
struct Candidate {
    info: GpuInfo,
    nvml_bandwidth: Option<f32>,
}

fn candidate(a: &RawAdapter, dev: Option<&RawNvmlDevice>) -> Candidate {
    let vendor = gpu::vendor_from_pci(a.vendor_id);
    let vram_total = dev
        .map(|d| d.memory_total)
        .filter(|&t| t > 0)
        .unwrap_or(a.dedicated_video_memory);
    let vram_free = dev
        .map(|d| d.memory_free)
        .or(a.budget.map(|b| b.min(vram_total)));
    let integrated = a
        .integrated
        .unwrap_or_else(|| gpu::looks_integrated(vendor, &a.name, a.dedicated_video_memory));
    Candidate {
        info: GpuInfo {
            index: a.index,
            name: a.name.trim().to_string(),
            vendor,
            vram_total,
            vram_free,
            shared_mem: a.shared_system_memory,
            integrated,
            bandwidth_gbs: None,
            bandwidth_source: Source::Default,
            cuda_cc: dev.and_then(|d| d.cuda_cc),
        },
        nvml_bandwidth: dev.and_then(|d| {
            Some(gpu::nvml_bandwidth_gbs(
                d.memory_bus_width?,
                d.max_memory_clock_mhz?,
            ))
            .filter(|&bw| bw > 0.0)
        }),
    }
}

/// §6.3 : un iGPU utilise la bande passante de la RAM ; sinon table, puis NVML, puis défaut.
fn resolve_bandwidth(c: &Candidate, ram: &RamInfo, specs: &GpuSpecs) -> (Option<f32>, Source) {
    if c.info.integrated {
        return match ram.bandwidth_gbs {
            Some(bw) => (Some(bw), Source::Estimated),
            None => (None, Source::Default),
        };
    }
    if let Some(spec) = specs.lookup(&c.info.name, c.info.vram_total) {
        return (Some(spec.bandwidth_gbs), Source::Table);
    }
    if let Some(bw) = c.nvml_bandwidth {
        return (Some(bw), Source::Nvml);
    }
    (Some(DEFAULT_GPU_BANDWIDTH_GBS), Source::Default)
}

fn build_gpus(
    raw: &RawHardware,
    ram: &RamInfo,
    o: &Overrides,
    specs: &GpuSpecs,
    warnings: &mut Vec<String>,
) -> (Option<GpuInfo>, Vec<GpuInfo>) {
    let physical: Vec<&RawAdapter> = raw
        .adapters
        .iter()
        .filter(|a| gpu::is_physical(a))
        .collect();
    let adapters = gpu::dedupe(&physical);
    let devices = raw.nvml.as_ref().map_or(&[][..], |n| &n.devices[..]);
    let mut candidates: Vec<Candidate> = adapters
        .iter()
        .zip(gpu::match_nvml(&adapters, devices))
        .map(|(a, j)| candidate(a, j.map(|j| &devices[j])))
        .collect();

    // Sélection : GPU imposé par l'utilisateur, sinon le plus gros GPU discret.
    let forced = o.gpu_index.and_then(|wanted| {
        let pos = candidates.iter().position(|c| c.info.index == wanted);
        if pos.is_none() {
            warnings.push(format!(
                "GPU n° {wanted} introuvable : sélection automatique du GPU."
            ));
        }
        pos
    });
    let infos: Vec<GpuInfo> = candidates.iter().map(|c| c.info.clone()).collect();
    let selected = forced.or_else(|| gpu::select(&infos));

    if let (Some(i), Some(integrated)) = (selected, o.gpu_integrated) {
        candidates[i].info.integrated = integrated;
    }
    for c in &mut candidates {
        (c.info.bandwidth_gbs, c.info.bandwidth_source) = resolve_bandwidth(c, ram, specs);
    }

    let mut others: Vec<GpuInfo> = Vec::with_capacity(candidates.len());
    let mut chosen = None;
    for (i, c) in candidates.into_iter().enumerate() {
        if Some(i) == selected {
            chosen = Some(c.info);
        } else {
            others.push(c.info);
        }
    }

    let simulated = o.simulated_gpu.as_deref().and_then(|label| {
        let spec = specs.by_label(label);
        if spec.is_none() {
            warnings.push(format!(
                "GPU simulé « {label} » absent de la table : simulation ignorée."
            ));
        }
        spec
    });

    if let Some(spec) = simulated {
        // Le GPU simulé remplace le GPU retenu ; les corrections du GPU réel ne s'y appliquent pas.
        let index = chosen.as_ref().map_or(0, |g| g.index);
        chosen = Some(GpuInfo {
            index,
            name: spec.name.clone(),
            vendor: spec.vendor(),
            vram_total: spec.vram_bytes(),
            vram_free: None,
            shared_mem: 0,
            integrated: false,
            bandwidth_gbs: Some(spec.bandwidth_gbs),
            bandwidth_source: Source::Table,
            cuda_cc: None,
        });
    } else {
        if let (Some(g), Some(bw)) = (chosen.as_mut(), o.gpu_bandwidth_gbs.filter(|&b| b > 0.0)) {
            g.bandwidth_gbs = Some(bw);
            g.bandwidth_source = Source::Manual;
        }
        gpu_warnings(chosen.as_ref(), raw, warnings);
    }
    (chosen, others)
}

fn gpu_warnings(chosen: Option<&GpuInfo>, raw: &RawHardware, warnings: &mut Vec<String>) {
    let Some(g) = chosen else {
        warnings.push("Aucun GPU détecté : les modèles tourneront sur le CPU.".into());
        return;
    };
    if g.integrated {
        warnings.push(
            "Seul un GPU intégré est disponible : il partage la RAM. llama.cpp ou LM Studio \
             (Vulkan) peuvent l'exploiter ; Ollama l'ignore souvent par défaut."
                .into(),
        );
    }
    if g.bandwidth_source == Source::Default && g.bandwidth_gbs.is_some() {
        warnings.push(format!(
            "Bande passante de « {} » inconnue : {DEFAULT_GPU_BANDWIDTH_GBS} Go/s par défaut \
             (à corriger si besoin).",
            g.name
        ));
    }
    if g.vendor == Vendor::Nvidia && raw.nvml.is_none() {
        warnings.push(
            "NVML indisponible : VRAM libre estimée par Windows (DXGI), sans version CUDA.".into(),
        );
    }
}

#[cfg(test)]
mod tests;
