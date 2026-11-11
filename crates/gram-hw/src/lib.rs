//! Détection du matériel : CPU, RAM, GPU, backends, runtimes installés et disques (§6 du plan).
//!
//! Deux étapes bien séparées :
//!
//! 1. [`collect`] lit la machine et renvoie des [`RawHardware`]. C'est la seule partie propre à
//!    chaque système :
//!    - Windows : DXGI + DXCore (tous constructeurs), NVML (NVIDIA, optionnel), WMI (barrettes,
//!      CPU), sysinfo (RAM, disques) ;
//!    - macOS : Metal (GPU, limite de mémoire du GPU, mémoire unifiée), `system_profiler`
//!      (type de mémoire, barrettes d'un Mac Intel, cœurs du GPU), sysinfo ;
//!    - ailleurs, seuls le CPU, la RAM et les disques sont lus.
//! 2. [`build_profile`] transforme ces données en [`HardwareProfile`], **sans I/O** : canaux et
//!    bande passante de la RAM, tables `data/gpu_specs.json` et `data/apple_chips.json`,
//!    sélection du plus gros GPU, corrections manuelles et simulation ([`Overrides`]). Testé sur
//!    des fixtures.
//!
//! [`detect_runtimes`] cherche Ollama, LM Studio et llama.cpp.
//!
//! Tout est bloquant : depuis Tauri, appeler ces fonctions dans `spawn_blocking`.

pub mod apple;
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
