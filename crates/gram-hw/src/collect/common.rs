//! Données disponibles sur tous les OS via `sysinfo` et `std::arch`.

use crate::raw::{RawCpu, RawHardware, RawMemory};
use gram_core::DiskInfo;
use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, RefreshKind, System};

pub fn collect() -> RawHardware {
    let sys = System::new_with_specifics(
        RefreshKind::nothing()
            .with_memory(MemoryRefreshKind::nothing().with_ram())
            .with_cpu(CpuRefreshKind::nothing()),
    );
    let cpu = RawCpu {
        name: sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .unwrap_or_default(),
        cores_physical: System::physical_core_count().unwrap_or(0) as u32,
        cores_logical: sys.cpus().len() as u32,
        ..cpu_features()
    };
    RawHardware {
        cpu,
        memory: RawMemory {
            total: sys.total_memory(),
            available: sys.available_memory(),
            ..Default::default()
        },
        disks: disks(),
        ..Default::default()
    }
}

/// Jeux d'instructions utiles à llama.cpp (§6.1).
fn cpu_features() -> RawCpu {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        RawCpu {
            x86: true,
            avx2: std::arch::is_x86_feature_detected!("avx2"),
            avx512: std::arch::is_x86_feature_detected!("avx512f"),
            fma: std::arch::is_x86_feature_detected!("fma"),
            f16c: std::arch::is_x86_feature_detected!("f16c"),
            ..Default::default()
        }
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        RawCpu::default()
    }
}

/// Disques fixes (les clés USB et lecteurs amovibles sont ignorés).
fn disks() -> Vec<DiskInfo> {
    let mut out: Vec<DiskInfo> = Vec::new();
    for d in Disks::new_with_refreshed_list().list() {
        if d.is_removable() || d.total_space() == 0 {
            continue;
        }
        let mount = d.mount_point().to_string_lossy().into_owned();
        if out.iter().any(|x| x.mount == mount) {
            continue;
        }
        out.push(DiskInfo {
            mount,
            total: d.total_space(),
            available: d.available_space(),
        });
    }
    out.sort_by(|a, b| a.mount.cmp(&b.mount));
    out
}
