//! Enrichissement NVIDIA via NVML (optionnel).
//!
//! `nvml.dll` n'existe qu'avec un pilote NVIDIA. Elle est chargée depuis un chemin absolu
//! (System32, ou l'ancien dossier NVSMI) et jamais depuis le dossier courant.

use crate::raw::{RawNvml, RawNvmlDevice};
use nvml_wrapper::enum_wrappers::device::Clock;
use nvml_wrapper::{Nvml, cuda_driver_version_major, cuda_driver_version_minor};
use std::path::{Path, PathBuf};

pub fn collect(system32: &Path) -> Option<RawNvml> {
    let nvml = init(system32)?;
    let cuda_driver_version = nvml.sys_cuda_driver_version().ok().map(|v| {
        format!(
            "{}.{}",
            cuda_driver_version_major(v),
            cuda_driver_version_minor(v)
        )
    });
    let count = nvml
        .device_count()
        .inspect_err(|e| tracing::warn!(error = %e, "NVML : nombre de GPU illisible"))
        .unwrap_or(0);

    let mut devices = Vec::new();
    for i in 0..count {
        let device = match nvml.device_by_index(i) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(index = i, error = %e, "NVML : GPU illisible");
                continue;
            }
        };
        let memory = device.memory_info().ok();
        let pci = device.pci_info().ok();
        devices.push(RawNvmlDevice {
            name: device.name().unwrap_or_default(),
            pci_device_id: pci.as_ref().map_or(0, |p| p.pci_device_id),
            pci_sub_system_id: pci.and_then(|p| p.pci_sub_system_id),
            memory_total: memory.as_ref().map_or(0, |m| m.total),
            memory_free: memory.map_or(0, |m| m.free),
            cuda_cc: device.cuda_compute_capability().ok().and_then(|cc| {
                Some((u32::try_from(cc.major).ok()?, u32::try_from(cc.minor).ok()?))
            }),
            memory_bus_width: device.memory_bus_width().ok().filter(|&w| w > 0),
            max_memory_clock_mhz: device.max_clock_info(Clock::Memory).ok().filter(|&c| c > 0),
        });
    }
    Some(RawNvml {
        cuda_driver_version,
        devices,
    })
}

fn init(system32: &Path) -> Option<Nvml> {
    let mut candidates = vec![system32.join("nvml.dll")];
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        candidates.push(PathBuf::from(pf).join(r"NVIDIA Corporation\NVSMI\nvml.dll"));
    }
    let Some(path) = candidates.into_iter().find(|p| p.is_file()) else {
        tracing::debug!("nvml.dll absente : pas de pilote NVIDIA");
        return None;
    };
    match Nvml::builder().lib_path(path.as_os_str()).init() {
        Ok(nvml) => Some(nvml),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "NVML présente mais inutilisable");
            None
        }
    }
}
