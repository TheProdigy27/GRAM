//! Collecte propre à Windows : DXGI (+ DXCore), NVML, WMI, DLL des backends.

mod dxgi;
mod nvml;
mod wmi;

use crate::raw::{RawBackends, RawHardware};
use std::path::PathBuf;

/// Lance WMI sur son propre thread (initialisation COM isolée, et c'est la source la plus
/// lente), pendant que `base` (sysinfo), DXGI et NVML tournent sur le thread appelant.
pub fn collect(base: impl FnOnce() -> RawHardware) -> RawHardware {
    let wmi_thread = std::thread::Builder::new()
        .name("gram-wmi".into())
        .spawn(wmi::query);

    let mut raw = base();
    let system32 = system32();

    match dxgi::adapters() {
        Ok(adapters) => raw.adapters = adapters,
        Err(e) => {
            tracing::warn!(error = %e, "énumération DXGI impossible");
            raw.notes
                .push(format!("Énumération des GPU (DXGI) impossible : {e}"));
        }
    }
    raw.nvml = nvml::collect(&system32);
    raw.backends = RawBackends {
        nvcuda: system32.join("nvcuda.dll").is_file(),
        vulkan: system32.join("vulkan-1.dll").is_file(),
    };

    let wmi = wmi_thread.map_err(|e| e.to_string()).and_then(|h| {
        h.join()
            .map_err(|_| "le thread WMI a paniqué".to_string())?
    });
    match wmi {
        Ok(data) => data.apply(&mut raw),
        Err(e) => {
            tracing::warn!(error = %e, "WMI indisponible");
            raw.notes.push(format!("WMI indisponible : {e}"));
        }
    }
    raw
}

/// `C:\Windows\System32`, lu via l'API plutôt que par une variable d'environnement.
fn system32() -> PathBuf {
    use ::windows::Win32::System::SystemInformation::GetSystemDirectoryW;
    let mut buf = [0u16; 260];
    // SAFETY : le tampon est valide et sa taille est transmise.
    let len = unsafe { GetSystemDirectoryW(Some(&mut buf)) } as usize;
    if len == 0 || len > buf.len() {
        return PathBuf::from(r"C:\Windows\System32");
    }
    PathBuf::from(String::from_utf16_lossy(&buf[..len]))
}
