//! Données brutes de détection, telles que lues sur la machine.
//!
//! [`crate::collect`] les remplit (seule partie qui touche au système) ;
//! [`crate::build_profile`] les transforme en [`gram_core::HardwareProfile`] sans aucune I/O.
//! Elles sont sérialisables pour servir de fixtures (`gram-cli hw --raw`).

use gram_core::DiskInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RawHardware {
    pub cpu: RawCpu,
    pub memory: RawMemory,
    /// Adaptateurs DXGI, dans l'ordre d'énumération (logiciels compris).
    pub adapters: Vec<RawAdapter>,
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
    pub avx2: bool,
    pub avx512: bool,
    pub fma: bool,
    pub f16c: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RawMemory {
    /// Mémoire vue par Windows, en octets.
    pub total: u64,
    /// Octets.
    pub available: u64,
    /// Barrettes (`Win32_PhysicalMemory`) ; vide si WMI a échoué.
    pub modules: Vec<RawMemoryModule>,
    /// Emplacements de la carte mère (`Win32_PhysicalMemoryArray.MemoryDevices`).
    pub slots: Option<u32>,
}

/// Une ligne de `Win32_PhysicalMemory`.
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
}
