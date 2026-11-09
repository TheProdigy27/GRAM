//! Détection du matériel : CPU, RAM, GPU, backends, runtimes installés et disques (§6 du plan).
//!
//! Deux étapes bien séparées :
//!
//! 1. [`collect`] lit la machine et renvoie des [`RawHardware`] : DXGI + DXCore (tous
//!    constructeurs), NVML (NVIDIA, optionnel), WMI (barrettes, CPU), sysinfo (RAM, disques).
//!    C'est la seule partie propre à Windows (`#[cfg(windows)]`) ; ailleurs, seuls le CPU, la RAM
//!    et les disques sont lus.
//! 2. [`build_profile`] transforme ces données en [`HardwareProfile`], **sans I/O** : canaux et
//!    bande passante de la RAM, table `data/gpu_specs.json`, sélection du plus gros GPU,
//!    corrections manuelles et simulation ([`Overrides`]). Testé sur des fixtures.
//!
//! [`detect_runtimes`] cherche Ollama, LM Studio et llama.cpp.
//!
//! Tout est bloquant : depuis Tauri, appeler ces fonctions dans `spawn_blocking`.

mod collect;
mod gpu;
mod profile;
mod ram;
pub mod raw;
mod runtimes;
pub mod specs;

pub use collect::collect;
pub use profile::{DEFAULT_GPU_BANDWIDTH_GBS, build_profile, build_profile_with};
pub use raw::RawHardware;
pub use runtimes::detect_runtimes;
pub use specs::{GpuSpec, GpuSpecs};

use gram_core::{HardwareProfile, Overrides};

/// Raccourci : collecte puis construit le profil, sans correction manuelle.
pub fn detect() -> HardwareProfile {
    build_profile(&collect(), &Overrides::default())
}
