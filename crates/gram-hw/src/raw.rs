//! Données brutes de détection, telles que lues sur la machine.
//!
//! [`crate::collect`] les remplit (seule partie qui touche au système) ;
//! [`crate::build_profile`] les transforme en [`gram_core::HardwareProfile`] sans aucune I/O.
//! Elles sont sérialisables pour servir de fixtures (`gram-cli hw --raw`).

use gram_core::{DiskInfo, Os};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RawHardware {
    /// Système de la machine (absent des anciens dumps : Windows).
    pub os: Os,
    pub cpu: RawCpu,
    pub memory: RawMemory,
    /// Adaptateurs DXGI, dans l'ordre d'énumération (logiciels compris). Windows.
    pub adapters: Vec<RawAdapter>,
    /// GPU vus par Metal (`MTLCopyAllDevices`), dans l'ordre d'énumération. macOS.
    pub metal: Vec<RawMetalDevice>,
    /// `None` si NVML n'a pas pu être chargée (pas de pilote NVIDIA, ou échec).
    pub nvml: Option<RawNvml>,
    pub backends: RawBackends,
    pub disks: Vec<DiskInfo>,
    /// Problèmes rencontrés pendant la collecte, repris dans les avertissements.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RawCpu {
    pub name: String,
    pub cores_physical: u32,
    pub cores_logical: u32,
    pub max_clock_mhz: Option<u32>,
    /// Architecture x86 / x86-64 (les drapeaux AVX n'ont de sens que là).
    pub x86: bool,
    /// Architecture ARM (Apple Silicon, y compris sous Rosetta).
    pub arm: bool,
    pub avx2: bool,
    pub avx512: bool,
    pub fma: bool,
    pub f16c: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RawMemory {
    /// Mémoire vue par le système, en octets.
    pub total: u64,
    /// Octets.
    pub available: u64,
    /// Barrettes (`Win32_PhysicalMemory`, ou `system_profiler` sur un Mac Intel) ; vide si WMI
    /// a échoué, et sur un Mac Apple Silicon (mémoire soudée à la puce).
    pub modules: Vec<RawMemoryModule>,
    /// Emplacements de la carte mère (`Win32_PhysicalMemoryArray.MemoryDevices`).
    pub slots: Option<u32>,
    /// Type de mémoire annoncé sans barrette (`system_profiler` sur Apple Silicon : « LPDDR5 »).
    pub type_name: Option<String>,
}

/// Une ligne de `Win32_PhysicalMemory` (ou une barrette d'un Mac Intel).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RawMemoryModule {
    /// Octets.
    pub capacity: u64,
    /// `Speed` : vitesse maximale annoncée par la barrette.
    pub speed: Option<u32>,
    /// `ConfiguredClockSpeed` : vitesse réellement configurée (MT/s, ou MHz sur d'anciens BIOS).
    pub configured_speed: Option<u32>,
    /// `SMBIOSMemoryType` (26 = DDR4, 34 = DDR5…).
    pub smbios_type: Option<u32>,
    /// Type en clair, sans code SMBIOS (`system_profiler` : « DDR4 », « LPDDR4X »).
    pub type_name: Option<String>,
    /// `DataWidth`, en bits.
    pub data_width: Option<u32>,
    /// Ex. « DIMMA2 », « Controller0-ChannelA-DIMM0 ».
    pub device_locator: String,
    /// Ex. « P0 CHANNEL A », « BANK 0 ».
    pub bank_label: String,
}

/// Un adaptateur DXGI (`DXGI_ADAPTER_DESC1`), enrichi par DXCore quand c'est possible.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RawAdapter {
    pub index: u32,
    pub name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub sub_sys_id: u32,
    /// Octets.
    pub dedicated_video_memory: u64,
    /// Octets.
    pub dedicated_system_memory: u64,
    /// Octets.
    pub shared_system_memory: u64,
    /// `QueryVideoMemoryInfo(LOCAL).Budget`, en octets.
    pub budget: Option<u64>,
    /// `DXGI_ADAPTER_FLAG_SOFTWARE`.
    pub software: bool,
    /// DXCore `IsIntegrated`.
    pub integrated: Option<bool>,
    /// DXCore `IsHardware`.
    pub hardware: Option<bool>,
}

/// Un GPU vu par Metal (`MTLDevice`), complété par `system_profiler`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RawMetalDevice {
    /// Position dans `MTLCopyAllDevices` ; référence de `Overrides::gpu_index`.
    pub index: u32,
    pub name: String,
    pub registry_id: u64,
    /// `recommendedMaxWorkingSetSize` : mémoire que le GPU peut utiliser sans dégrader les
    /// performances, en octets. Sur Apple Silicon, une grande part de la RAM (11,8 Gio sur
    /// 16 Go ; réglable avec `iogpu.wired_limit_mb`) : c'est la « VRAM » que comptent llama.cpp
    /// et Ollama.
    pub recommended_max_working_set: u64,
    /// `hasUnifiedMemory` (Apple Silicon, mais aussi l'iGPU Intel d'un Mac Intel).
    pub unified_memory: bool,
    /// `isLowPower` : GPU intégré d'un Mac Intel à deux GPU.
    pub low_power: bool,
    /// `isRemovable` : GPU externe (eGPU).
    pub removable: bool,
    /// Cœurs du GPU (`system_profiler`, Apple Silicon).
    pub gpu_cores: Option<u32>,
    /// VRAM annoncée par `system_profiler` (GPU des Mac Intel), en octets.
    pub vram: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RawNvml {
    /// Version CUDA prise en charge par le pilote (ex. « 13.4 »).
    pub cuda_driver_version: Option<String>,
    pub devices: Vec<RawNvmlDevice>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RawNvmlDevice {
    pub name: String,
    /// Identifiant PCI combiné : `device << 16 | vendor`.
    pub pci_device_id: u32,
    pub pci_sub_system_id: Option<u32>,
    /// Octets.
    pub memory_total: u64,
    /// Octets.
    pub memory_free: u64,
    pub cuda_cc: Option<(u32, u32)>,
    pub memory_bus_width: Option<u32>,
    /// Horloge mémoire maximale en MHz (débit effectif = 2 × cette valeur).
    pub max_memory_clock_mhz: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RawBackends {
    /// `nvcuda.dll` présente dans System32.
    pub nvcuda: bool,
    /// `vulkan-1.dll` présente dans System32.
    pub vulkan: bool,
    /// Au moins un GPU Metal (macOS).
    pub metal: bool,
}
