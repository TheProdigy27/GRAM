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
    /// Système de la machine : il décide des commandes de lancement (variables d'environnement
    /// d'Ollama) et de ce que les runtimes savent faire. Absent des anciens profils : Windows.
    #[serde(default)]
    pub os: Os,
    pub cpu: CpuInfo,
    pub ram: RamInfo,
    /// Le GPU retenu pour le calcul : celui qui a le plus de VRAM dédiée (ou le GPU à mémoire
    /// unifiée d'un Mac Apple Silicon).
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

/// Système d'exploitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Os {
    #[default]
    Windows,
    #[serde(rename = "macos")]
    MacOs,
    Linux,
}

impl Os {
    /// Système pour lequel le programme est compilé.
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }

    /// Nom affiché.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Windows => "Windows",
            Self::MacOs => "macOS",
            Self::Linux => "Linux",
        }
    }
}

/// Architecture du processeur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CpuArch {
    /// x86 / x86-64 : les drapeaux AVX ont un sens.
    #[default]
    X86,
    /// ARM 64 bits (Apple Silicon, Snapdragon) : NEON toujours présent, pas d'AVX.
    Arm,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CpuInfo {
    pub name: String,
    /// Absent des anciens profils : x86.
    #[serde(default)]
    pub arch: CpuArch,
    pub cores_physical: u32,
    pub cores_logical: u32,
    pub max_clock_mhz: Option<u32>,
    pub avx2: bool,
    pub avx512: bool,
    pub fma: bool,
    pub f16c: bool,
}

/// Type de mémoire vive (d'après `SMBIOSMemoryType`, ou la table des puces Apple).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RamKind {
    Ddr3,
    Ddr4,
    Ddr5,
    Lpddr4,
    Lpddr4x,
    Lpddr5,
    Lpddr5x,
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
            RamKind::Lpddr4x => "LPDDR4X",
            RamKind::Lpddr5 => "LPDDR5",
            RamKind::Lpddr5x => "LPDDR5X",
            RamKind::Unknown => "type inconnu",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RamInfo {
    /// Mémoire vue par le système (sous Windows, installée moins la mémoire réservée au
    /// matériel), en octets.
    pub total: u64,
    /// Octets.
    pub available: u64,
    /// Somme des capacités des barrettes (ce qu'affiche le Gestionnaire des tâches), ou mémoire
    /// unifiée d'un Mac, en octets.
    pub installed: Option<u64>,
    pub kind: RamKind,
    /// Vitesse configurée en MT/s.
    pub speed_mts: Option<u32>,
    /// Nombre de barrettes détectées.
    pub modules: u32,
    /// Nombre d'emplacements de la carte mère.
    pub slots: Option<u32>,
    /// Nombre de canaux estimé (64 bits chacun ; largeur du bus / 64 sur Apple Silicon).
    pub channels: u32,
    pub channels_source: Source,
    /// Bande passante théorique en Go/s.
    pub bandwidth_gbs: Option<f32>,
}

/// Constructeur d'un GPU (d'après le vendor ID PCI, ou le nom sous macOS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    Other,
}

/// Origine d'une valeur, affichée dans l'interface à côté de celle-ci.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Source {
    /// Lue directement sur la machine (WMI, DXGI, Metal, sysinfo).
    Detected,
    /// Lue ou calculée via NVML.
    Nvml,
    /// Trouvée dans une table de référence : `data/gpu_specs.json` (GPU) ou
    /// `data/apple_chips.json` (puces Apple Silicon).
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
    /// Position de l'adaptateur dans l'énumération DXGI (Metal sous macOS) ; référence de
    /// `Overrides::gpu_index`.
    pub index: u32,
    pub name: String,
    pub vendor: Vendor,
    /// VRAM dédiée, en octets. En mémoire unifiée : la part de la RAM que le GPU peut utiliser
    /// (`recommendedMaxWorkingSetSize` de Metal : 11,8 Gio sur un Mac de 16 Go).
    pub vram_total: u64,
    /// VRAM libre (NVML) ou budget DXGI, en octets.
    pub vram_free: Option<u64>,
    /// Mémoire système partagée, en octets.
    pub shared_mem: u64,
    pub integrated: bool,
    /// Mémoire unifiée (Apple Silicon) : GPU intégré qui calcule directement dans la RAM, à sa
    /// bande passante, jusqu'à `vram_total` octets. Contrairement aux autres GPU intégrés, il
    /// reçoit les couches du modèle (Metal) ; sa mémoire est prise sur la RAM.
    #[serde(default)]
    pub unified_memory: bool,
    /// Bande passante mémoire en Go/s.
    pub bandwidth_gbs: Option<f32>,
    pub bandwidth_source: Source,
    /// Compute capability CUDA (majeure, mineure).
    pub cuda_cc: Option<(u32, u32)>,
}

impl GpuInfo {
    /// Le GPU peut recevoir des couches du modèle : GPU discret, ou GPU à mémoire unifiée.
    /// Un autre GPU intégré compte comme du CPU (même mémoire, même bande passante).
    pub fn offload(&self) -> bool {
        (!self.integrated || self.unified_memory) && self.vram_total > 0
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Backends {
    pub cuda: bool,
    /// Version CUDA prise en charge par le driver (ex. « 12.8 »).
    pub cuda_driver_version: Option<String>,
    pub vulkan: bool,
    /// Au moins un GPU Metal (macOS).
    #[serde(default)]
    pub metal: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiskInfo {
    /// Point de montage (ex. « C:\ », « / »).
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

impl Overrides {
    /// Une simulation est en cours (GPU ou RAM).
    pub fn is_simulation(&self) -> bool {
        self.simulated_gpu.is_some() || self.simulated_ram_total.is_some()
    }

    /// Les corrections seules, sans la simulation : ce qui est enregistré d'un lancement à
    /// l'autre (une simulation est un essai, elle s'arrête à la fermeture).
    pub fn corrections(&self) -> Self {
        Self {
            simulated_gpu: None,
            simulated_ram_total: None,
            ..self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> HardwareProfile {
        HardwareProfile {
            os: Os::Windows,
            cpu: CpuInfo {
                name: "AMD Ryzen 7 5800X".into(),
                arch: CpuArch::X86,
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
                unified_memory: false,
                bandwidth_gbs: Some(360.0),
                bandwidth_source: Source::Table,
                cuda_cc: Some((8, 6)),
            }),
            other_gpus: vec![],
            backends: Backends {
                cuda: true,
                cuda_driver_version: Some("12.8".into()),
                vulkan: true,
                metal: false,
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
    fn anciens_profils_lisibles() {
        // Profil enregistré avant la prise en charge de macOS : sans `os`, `arch`,
        // `unifiedMemory` ni `metal`.
        let mut json = serde_json::to_value(sample()).unwrap();
        json.as_object_mut().unwrap().remove("os");
        json["cpu"].as_object_mut().unwrap().remove("arch");
        json["gpu"].as_object_mut().unwrap().remove("unifiedMemory");
        json["backends"].as_object_mut().unwrap().remove("metal");
        let back: HardwareProfile = serde_json::from_value(json).unwrap();
        assert_eq!(back, sample());
    }

    #[test]
    fn noms_serialises() {
        assert_eq!(serde_json::to_value(Os::MacOs).unwrap(), "macos");
        assert_eq!(serde_json::to_value(Os::Windows).unwrap(), "windows");
        assert_eq!(serde_json::to_value(CpuArch::Arm).unwrap(), "arm");
        assert_eq!(serde_json::to_value(RamKind::Lpddr5x).unwrap(), "lpddr5x");
        assert_eq!(serde_json::to_value(Vendor::Apple).unwrap(), "apple");
    }

    #[test]
    fn gpu_utilisable_pour_les_couches() {
        let mut g = sample().gpu.unwrap();
        assert!(g.offload());
        g.integrated = true;
        assert!(!g.offload(), "un iGPU compte comme du CPU");
        g.unified_memory = true;
        assert!(g.offload(), "sauf en mémoire unifiée");
        g.vram_total = 0;
        assert!(!g.offload());
    }

    #[test]
    fn overrides_vides_acceptes() {
        let o: Overrides = serde_json::from_str("{}").unwrap();
        assert_eq!(o, Overrides::default());
        assert!(!o.is_simulation());
    }

    #[test]
    fn corrections_sans_simulation() {
        let o = Overrides {
            ram_channels: Some(4),
            gpu_bandwidth_gbs: Some(900.0),
            simulated_gpu: Some("NVIDIA GeForce RTX 4090 24 Go".into()),
            simulated_ram_total: Some(128 << 30),
            ..Overrides::default()
        };
        assert!(o.is_simulation());
        let c = o.corrections();
        assert!(!c.is_simulation());
        assert_eq!(
            (c.ram_channels, c.gpu_bandwidth_gbs),
            (Some(4), Some(900.0))
        );
    }
}
