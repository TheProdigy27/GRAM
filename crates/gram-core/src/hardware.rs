//! Description du matériel détecté (rempli par `gram-hw`).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Bande passante prudente d'un GPU discret absent de la table et sans NVML (§6.3).
pub const DEFAULT_GPU_BANDWIDTH_GBS: f32 = 200.0;

/// Bande passante prudente de la RAM quand sa vitesse est inconnue : DDR4-2400 en double canal.
pub const DEFAULT_RAM_BANDWIDTH_GBS: f32 = 38.4;

/// Profil matériel complet de la machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HardwareProfile {
    pub cpu: CpuInfo,
    pub ram: RamInfo,
    /// Le GPU retenu pour le calcul : celui qui a le plus de VRAM dédiée.
    pub gpu: Option<GpuInfo>,
    /// Les autres GPU, affichés pour information et non utilisés pour le calcul.
    pub other_gpus: Vec<GpuInfo>,
    pub backends: Backends,
    pub disks: Vec<DiskInfo>,
    /// Corrections manuelles et simulation.
    pub overrides: Overrides,
    /// Avertissements de détection, en français, affichés tels quels.
    pub warnings: Vec<String>,
}

impl HardwareProfile {
    /// Tous les GPU détectés (le GPU retenu en premier), pour choisir le GPU retenu.
    pub fn all_gpus(&self) -> impl Iterator<Item = &GpuInfo> {
        self.gpu.iter().chain(&self.other_gpus)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CpuInfo {
    pub name: String,
    pub cores_physical: u32,
    pub cores_logical: u32,
    pub max_clock_mhz: Option<u32>,
    pub avx2: bool,
    pub avx512: bool,
    pub fma: bool,
    pub f16c: bool,
}

/// Type de mémoire vive (d'après `SMBIOSMemoryType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RamKind {
    Ddr3,
    Ddr4,
    Ddr5,
    Lpddr4,
    Lpddr5,
    Unknown,
}

impl RamKind {
    /// Nom affiché.
    pub const fn label(self) -> &'static str {
        match self {
            RamKind::Ddr3 => "DDR3",
            RamKind::Ddr4 => "DDR4",
            RamKind::Ddr5 => "DDR5",
            RamKind::Lpddr4 => "LPDDR4",
            RamKind::Lpddr5 => "LPDDR5",
            RamKind::Unknown => "type inconnu",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RamInfo {
    /// Mémoire vue par Windows (installée moins la mémoire réservée au matériel), en octets.
    pub total: u64,
    /// Octets.
    pub available: u64,
    /// Somme des capacités des barrettes (ce qu'affiche le Gestionnaire des tâches), en octets.
    pub installed: Option<u64>,
    pub kind: RamKind,
    /// Vitesse configurée en MT/s.
    pub speed_mts: Option<u32>,
    /// Nombre de barrettes détectées.
    pub modules: u32,
    /// Nombre d'emplacements de la carte mère.
    pub slots: Option<u32>,
    /// Nombre de canaux estimé (64 bits chacun).
    pub channels: u32,
    pub channels_source: Source,
    /// Bande passante théorique en Go/s.
    pub bandwidth_gbs: Option<f32>,
}

/// Constructeur d'un GPU (d'après le vendor ID PCI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

/// Origine d'une valeur, affichée dans l'interface à côté de celle-ci.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Source {
    /// Lue directement sur la machine (WMI, DXGI, sysinfo).
    Detected,
    /// Lue ou calculée via NVML.
    Nvml,
    /// Trouvée dans `data/gpu_specs.json`.
    Table,
    /// Déduite par une heuristique.
    Estimated,
    /// Valeur par défaut prudente, faute de mieux.
    Default,
    /// Saisie par l'utilisateur.
    Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GpuInfo {
    /// Position de l'adaptateur dans l'énumération DXGI ; référence de `Overrides::gpu_index`.
    pub index: u32,
    pub name: String,
    pub vendor: Vendor,
    /// VRAM dédiée, en octets.
    pub vram_total: u64,
    /// VRAM libre (NVML) ou budget DXGI, en octets.
    pub vram_free: Option<u64>,
    /// Mémoire système partagée, en octets.
    pub shared_mem: u64,
    pub integrated: bool,
    /// Bande passante mémoire en Go/s.
    pub bandwidth_gbs: Option<f32>,
    pub bandwidth_source: Source,
    /// Compute capability CUDA (majeure, mineure).
    pub cuda_cc: Option<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Backends {
    pub cuda: bool,
    /// Version CUDA prise en charge par le driver (ex. « 12.8 »).
    pub cuda_driver_version: Option<String>,
    pub vulkan: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiskInfo {
    /// Point de montage (ex. « C:\ »).
    pub mount: String,
    /// Octets.
    pub total: u64,
    /// Octets.
    pub available: u64,
}

/// Corrections manuelles et simulation (§6.6 du plan). `None` = valeur détectée.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Overrides {
    pub ram_channels: Option<u32>,
    pub gpu_bandwidth_gbs: Option<f32>,
    pub gpu_integrated: Option<bool>,
    /// GPU retenu (`GpuInfo::index`) à la place du plus gros GPU.
    pub gpu_index: Option<u32>,
    /// Simulation : libellé d'un GPU de `gpu_specs.json` (ex. « NVIDIA GeForce RTX 3060 12 Go »)
    /// à la place du GPU réel.
    pub simulated_gpu: Option<String>,
    /// Simulation : quantité de RAM totale, en octets.
    pub simulated_ram_total: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> HardwareProfile {
        HardwareProfile {
            cpu: CpuInfo {
                name: "AMD Ryzen 7 5800X".into(),
                cores_physical: 8,
                cores_logical: 16,
                max_clock_mhz: Some(3800),
                avx2: true,
                avx512: false,
                fma: true,
                f16c: true,
            },
            ram: RamInfo {
                total: 32 << 30,
                available: 20 << 30,
                installed: Some(32 << 30),
                kind: RamKind::Ddr4,
                speed_mts: Some(3600),
                modules: 2,
                slots: Some(4),
                channels: 2,
                channels_source: Source::Estimated,
                bandwidth_gbs: Some(57.6),
            },
            gpu: Some(GpuInfo {
                index: 0,
                name: "NVIDIA GeForce RTX 3060".into(),
                vendor: Vendor::Nvidia,
                vram_total: 12 << 30,
                vram_free: Some(11 << 30),
                shared_mem: 16 << 30,
                integrated: false,
                bandwidth_gbs: Some(360.0),
                bandwidth_source: Source::Table,
                cuda_cc: Some((8, 6)),
            }),
            other_gpus: vec![],
            backends: Backends {
                cuda: true,
                cuda_driver_version: Some("12.8".into()),
                vulkan: true,
            },
            disks: vec![DiskInfo {
                mount: "C:\\".into(),
                total: 1 << 40,
                available: 210 << 30,
            }],
            overrides: Overrides::default(),
            warnings: vec![],
        }
    }

    #[test]
    fn aller_retour_json() {
        let hw = sample();
        let json = serde_json::to_string(&hw).unwrap();
        let back: HardwareProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(hw, back);
    }

    #[test]
    fn champs_en_camel_case() {
        let json = serde_json::to_value(sample()).unwrap();
        assert!(json.get("otherGpus").is_some());
        assert_eq!(json["gpu"]["vramTotal"], 12u64 << 30);
        assert_eq!(json["gpu"]["vendor"], "nvidia");
        assert_eq!(json["gpu"]["cudaCc"], serde_json::json!([8, 6]));
    }

    #[test]
    fn overrides_vides_acceptes() {
        let o: Overrides = serde_json::from_str("{}").unwrap();
        assert_eq!(o, Overrides::default());
    }
}
