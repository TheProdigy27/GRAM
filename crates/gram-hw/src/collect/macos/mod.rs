//! Collecte propre à macOS : Metal (GPU), `system_profiler` (mémoire, cœurs du GPU), sysinfo.

mod metal;

use super::system_profiler;
use crate::raw::RawHardware;
use sysinfo::{CpuRefreshKind, RefreshKind, System};

/// Lance `system_profiler` sur son propre thread (c'est la source la plus lente : ≈ 0,3 s),
/// pendant que `base` (sysinfo) et Metal tournent sur le thread appelant.
pub fn collect(base: impl FnOnce() -> RawHardware) -> RawHardware {
    let profiler_thread = std::thread::Builder::new()
        .name("gram-system-profiler".into())
        .spawn(system_profiler::query);

    let mut raw = base();
    raw.cpu.max_clock_mhz = cpu_max_mhz();
    raw.metal = metal::devices();
    raw.backends.metal = !raw.metal.is_empty();
    if raw.metal.is_empty() {
        tracing::warn!("aucun GPU Metal");
    }

    let profile = profiler_thread.map_err(|e| e.to_string()).and_then(|h| {
        h.join()
            .map_err(|_| "le thread system_profiler a paniqué".to_string())?
    });
    match profile {
        Ok(p) => p.apply(&mut raw),
        Err(e) => {
            tracing::warn!(error = %e, "system_profiler indisponible");
            raw.notes
                .push(format!("system_profiler indisponible : {e}"));
        }
    }
    raw
}

/// Fréquence annoncée par sysinfo : celle des cœurs performance d'une puce Apple (IOKit), la
/// fréquence de base d'un processeur Intel (`hw.cpufrequency`).
fn cpu_max_mhz() -> Option<u32> {
    let sys = System::new_with_specifics(
        RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing().with_frequency()),
    );
    sys.cpus()
        .iter()
        .map(|c| c.frequency())
        .max()
        .filter(|&f| f > 0)
        .and_then(|f| u32::try_from(f).ok())
}
