//! Règles sur les GPU (§6.3 du plan) : constructeur, iGPU, association NVML ↔ DXGI, sélection.
//! Sans I/O.

use crate::raw::{RawAdapter, RawNvmlDevice};
use crate::specs::normalize_name;
use gram_core::{GpuInfo, Vendor};

pub const VENDOR_NVIDIA: u32 = 0x10DE;
pub const VENDOR_AMD: u32 = 0x1002;
pub const VENDOR_INTEL: u32 = 0x8086;
/// Microsoft Basic Render Driver et autres adaptateurs logiciels de Windows.
pub const VENDOR_MICROSOFT: u32 = 0x1414;

const GIB: u64 = 1 << 30;

pub fn vendor_from_pci(id: u32) -> Vendor {
    match id {
        VENDOR_NVIDIA => Vendor::Nvidia,
        VENDOR_AMD => Vendor::Amd,
        VENDOR_INTEL => Vendor::Intel,
        _ => Vendor::Other,
    }
}

/// Constructeur d'après le nom (Metal ne donne pas l'identifiant PCI) : « Apple M1 Pro »,
/// « AMD Radeon Pro 5500M », « Intel(R) UHD Graphics 630 », « NVIDIA GeForce GT 750M ».
pub fn vendor_from_name(name: &str) -> Vendor {
    let lower = name.trim().to_lowercase();
    let has = |w: &str| lower.split(|c: char| !c.is_alphanumeric()).any(|x| x == w);
    if lower.starts_with("apple") {
        Vendor::Apple
    } else if has("nvidia") || has("geforce") || has("quadro") {
        Vendor::Nvidia
    } else if has("amd") || has("radeon") || has("ati") {
        Vendor::Amd
    } else if has("intel") {
        Vendor::Intel
    } else {
        Vendor::Other
    }
}

/// Adaptateur matériel réel : ni logiciel (WARP, Basic Render), ni virtuel selon DXCore.
pub fn is_physical(a: &RawAdapter) -> bool {
    !a.software && a.vendor_id != VENDOR_MICROSOFT && a.hardware != Some(false)
}

/// Retire les doublons DXGI d'un même GPU physique.
///
/// Un écran virtuel (Parsec, Indirect Display Driver…) fait apparaître une seconde fois le GPU
/// qui le pilote : mêmes identifiants PCI et même VRAM, mais un LUID inconnu de DXCore.
/// On garde donc les adaptateurs confirmés par DXCore, puis les autres seulement s'ils ne
/// dupliquent pas un adaptateur déjà gardé. Sans DXCore, deux GPU strictement identiques sont
/// fusionnés : sans effet sur la sélection, seule la liste « autres GPU » est raccourcie.
pub fn dedupe<'a>(adapters: &[&'a RawAdapter]) -> Vec<&'a RawAdapter> {
    let same_device = |a: &RawAdapter, b: &RawAdapter| {
        a.vendor_id == b.vendor_id
            && a.device_id == b.device_id
            && a.sub_sys_id == b.sub_sys_id
            && a.dedicated_video_memory == b.dedicated_video_memory
    };
    let (confirmed, unknown): (Vec<&RawAdapter>, Vec<&RawAdapter>) =
        adapters.iter().partition(|a| a.hardware.is_some());
    let mut kept = confirmed;
    for a in unknown {
        if kept.iter().any(|k| same_device(k, a)) {
            tracing::debug!(index = a.index, name = %a.name, "doublon DXGI ignoré");
        } else {
            kept.push(a);
        }
    }
    kept.sort_by_key(|a| a.index);
    kept
}

/// Heuristique de secours quand DXCore ne dit pas si le GPU est intégré :
/// moins de 1 Gio de VRAM dédiée, ou nom typique d'un iGPU AMD / Intel.
/// (Les iGPU récents réservent souvent 2 Gio ou plus : le seuil seul ne suffit pas.)
pub fn looks_integrated(vendor: Vendor, name: &str, dedicated: u64) -> bool {
    if dedicated < GIB {
        return true;
    }
    let key = normalize_name(name).key;
    match vendor {
        // Discrets : « RX 7800 XT », « PRO W7900 », « Instinct ». iGPU : « Radeon(TM) Graphics », « 780M ».
        Vendor::Amd => {
            !(key.starts_with("rx ") || key.starts_with("pro ") || key.contains("instinct"))
        }
        // Discrets : « Arc A770 », « Arc B580 », « Arc Pro A60 ». iGPU : « UHD », « Iris Xe », « Arc 140V ».
        Vendor::Intel => !is_intel_arc_discrete(&key),
        // Les GPU d'Apple sont tous intégrés à la puce.
        Vendor::Apple => true,
        Vendor::Nvidia | Vendor::Other => false,
    }
}

fn is_intel_arc_discrete(key: &str) -> bool {
    let Some(model) = key.strip_prefix("arc ") else {
        return false;
    };
    let model = model.strip_prefix("pro ").unwrap_or(model);
    let mut chars = model.chars();
    matches!(chars.next(), Some('a' | 'b')) && chars.next().is_some_and(|c| c.is_ascii_digit())
}

/// Bande passante calculée depuis NVML : largeur de bus / 8 × 2 × horloge mémoire (DDR).
pub fn nvml_bandwidth_gbs(bus_width_bits: u32, max_memory_clock_mhz: u32) -> f32 {
    (f64::from(bus_width_bits) / 8.0 * 2.0 * f64::from(max_memory_clock_mhz) / 1000.0) as f32
}

/// Associe chaque adaptateur DXGI à un device NVML : par identifiants PCI (device + sous-système),
/// sinon par nom. Un device NVML n'est utilisé qu'une fois.
pub fn match_nvml(adapters: &[&RawAdapter], devices: &[RawNvmlDevice]) -> Vec<Option<usize>> {
    let mut used = vec![false; devices.len()];
    adapters
        .iter()
        .map(|a| {
            if a.vendor_id != VENDOR_NVIDIA {
                return None;
            }
            let by_pci = |d: &RawNvmlDevice| {
                d.pci_device_id & 0xFFFF == a.vendor_id
                    && d.pci_device_id >> 16 == a.device_id
                    && d.pci_sub_system_id.is_none_or(|s| s == a.sub_sys_id)
            };
            let by_name = |d: &RawNvmlDevice| d.name.trim().eq_ignore_ascii_case(a.name.trim());
            let found = (0..devices.len())
                .find(|&j| !used[j] && by_pci(&devices[j]))
                .or_else(|| (0..devices.len()).find(|&j| !used[j] && by_name(&devices[j])));
            if let Some(j) = found {
                used[j] = true;
            }
            found
        })
        .collect()
}

/// GPU retenu (décision §3) : parmi les GPU qui reçoivent des couches (discrets, ou à mémoire
/// unifiée), celui qui a le plus de VRAM ; à défaut, l'iGPU qui en a le plus. À égalité, le
/// premier énuméré.
pub fn select(gpus: &[GpuInfo]) -> Option<usize> {
    let best = |usable: bool| {
        gpus.iter()
            .enumerate()
            .filter(|(_, g)| (!g.integrated || g.unified_memory) == usable)
            .max_by(|(ia, a), (ib, b)| a.vram_total.cmp(&b.vram_total).then(ib.cmp(ia)))
            .map(|(i, _)| i)
    };
    best(true).or_else(|| best(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gram_core::Source;

    fn gpu(index: u32, vram_gib: u64, integrated: bool) -> GpuInfo {
        GpuInfo {
            index,
            name: format!("GPU {index}"),
            vendor: Vendor::Other,
            vram_total: vram_gib * GIB,
            vram_free: None,
            shared_mem: 0,
            integrated,
            unified_memory: false,
            bandwidth_gbs: None,
            bandwidth_source: Source::Default,
            cuda_cc: None,
        }
    }

    #[test]
    fn constructeurs() {
        assert_eq!(vendor_from_pci(0x10DE), Vendor::Nvidia);
        assert_eq!(vendor_from_pci(0x1002), Vendor::Amd);
        assert_eq!(vendor_from_pci(0x8086), Vendor::Intel);
        assert_eq!(vendor_from_pci(0x1414), Vendor::Other);
    }

    #[test]
    fn constructeurs_d_apres_le_nom() {
        assert_eq!(vendor_from_name("Apple M1 Pro"), Vendor::Apple);
        assert_eq!(vendor_from_name("AMD Radeon Pro 5500M"), Vendor::Amd);
        assert_eq!(vendor_from_name("Radeon Pro Vega 20"), Vendor::Amd);
        assert_eq!(vendor_from_name("Intel(R) UHD Graphics 630"), Vendor::Intel);
        assert_eq!(vendor_from_name("NVIDIA GeForce GT 750M"), Vendor::Nvidia);
        assert_eq!(vendor_from_name("Paravirtual GPU"), Vendor::Other);
        // « ati » seulement comme mot entier.
        assert_eq!(vendor_from_name("Corporation Graphics"), Vendor::Other);
    }

    #[test]
    fn selection_mac_apple_silicon() {
        // GPU à mémoire unifiée : intégré, mais il reçoit les couches.
        let mut apple = gpu(0, 10, true);
        apple.unified_memory = true;
        assert_eq!(select(&[gpu(1, 12, true), apple]), Some(1));
    }

    #[test]
    fn heuristique_igpu() {
        // Ryzen 9000 : iGPU avec 2 Gio réservés, au-dessus du seuil de 1 Gio.
        assert!(looks_integrated(
            Vendor::Amd,
            "AMD Radeon(TM) Graphics",
            2 * GIB
        ));
        assert!(looks_integrated(
            Vendor::Amd,
            "AMD Radeon 780M Graphics",
            4 * GIB
        ));
        assert!(!looks_integrated(
            Vendor::Amd,
            "AMD Radeon RX 7800 XT",
            16 * GIB
        ));
        assert!(!looks_integrated(
            Vendor::Amd,
            "AMD Radeon PRO W7900",
            48 * GIB
        ));
        assert!(looks_integrated(
            Vendor::Intel,
            "Intel(R) UHD Graphics 770",
            128 << 20
        ));
        assert!(looks_integrated(
            Vendor::Intel,
            "Intel(R) Arc(TM) Graphics",
            2 * GIB
        ));
        assert!(looks_integrated(
            Vendor::Intel,
            "Intel(R) Arc(TM) 140V GPU (16GB)",
            2 * GIB
        ));
        assert!(!looks_integrated(
            Vendor::Intel,
            "Intel(R) Arc(TM) A770 Graphics",
            16 * GIB
        ));
        assert!(!looks_integrated(
            Vendor::Intel,
            "Intel(R) Arc(TM) B580 Graphics",
            12 * GIB
        ));
        assert!(!looks_integrated(
            Vendor::Nvidia,
            "NVIDIA GeForce RTX 4060",
            8 * GIB
        ));
        assert!(looks_integrated(
            Vendor::Nvidia,
            "NVIDIA GeForce MX",
            512 << 20
        ));
    }

    #[test]
    fn bande_passante_nvml() {
        // RTX 5080 (GDDR7, 256 bits) : NVML annonce 15 001 MHz → 960 Go/s.
        assert!((nvml_bandwidth_gbs(256, 15001) - 960.06).abs() < 0.01);
        // RTX 3060 12 Go (GDDR6, 192 bits) : 7 501 MHz → 360 Go/s.
        assert!((nvml_bandwidth_gbs(192, 7501) - 360.05).abs() < 0.01);
    }

    #[test]
    fn selection_du_plus_gros_gpu_discret() {
        // Portable Optimus : l'iGPU est énuméré en premier.
        let gpus = [gpu(0, 2, true), gpu(1, 8, false), gpu(2, 12, false)];
        assert_eq!(select(&gpus), Some(2));
        // À VRAM égale, le premier énuméré.
        let gpus = [gpu(0, 12, false), gpu(1, 12, false)];
        assert_eq!(select(&gpus), Some(0));
    }

    #[test]
    fn selection_igpu_seul() {
        let gpus = [gpu(0, 1, true), gpu(1, 2, true)];
        assert_eq!(select(&gpus), Some(1));
        assert_eq!(select(&[]), None);
    }

    fn adapter(index: u32, name: &str, vendor: u32, device: u32, subsys: u32) -> RawAdapter {
        RawAdapter {
            index,
            name: name.into(),
            vendor_id: vendor,
            device_id: device,
            sub_sys_id: subsys,
            ..Default::default()
        }
    }

    fn nvml_dev(name: &str, device: u32, subsys: Option<u32>) -> RawNvmlDevice {
        RawNvmlDevice {
            name: name.into(),
            pci_device_id: device << 16 | VENDOR_NVIDIA,
            pci_sub_system_id: subsys,
            ..Default::default()
        }
    }

    #[test]
    fn association_nvml_par_pci() {
        let a0 = adapter(0, "AMD Radeon(TM) Graphics", VENDOR_AMD, 0x13C0, 1);
        let a1 = adapter(1, "NVIDIA GeForce RTX 3060", VENDOR_NVIDIA, 0x2504, 0xAAAA);
        let a2 = adapter(2, "NVIDIA GeForce RTX 3060", VENDOR_NVIDIA, 0x2504, 0xBBBB);
        // NVML énumère dans un autre ordre que DXGI.
        let devices = [
            nvml_dev("NVIDIA GeForce RTX 3060", 0x2504, Some(0xBBBB)),
            nvml_dev("NVIDIA GeForce RTX 3060", 0x2504, Some(0xAAAA)),
        ];
        assert_eq!(
            match_nvml(&[&a0, &a1, &a2], &devices),
            vec![None, Some(1), Some(0)]
        );
    }

    #[test]
    fn association_nvml_par_nom_en_secours() {
        let a = adapter(0, "NVIDIA GeForce RTX 4090", VENDOR_NVIDIA, 0x2684, 1);
        let devices = [nvml_dev("NVIDIA GeForce RTX 4090", 0x9999, None)];
        assert_eq!(match_nvml(&[&a], &devices), vec![Some(0)]);
        assert_eq!(match_nvml(&[&a], &[]), vec![None]);
    }

    #[test]
    fn doublon_d_ecran_virtuel_retire() {
        let mut real = adapter(
            0,
            "NVIDIA GeForce RTX 5080",
            VENDOR_NVIDIA,
            0x2C02,
            0x5315_1462,
        );
        real.dedicated_video_memory = 16 * GIB;
        real.hardware = Some(true);
        let mut igpu = adapter(1, "AMD Radeon(TM) Graphics", VENDOR_AMD, 0x13C0, 1);
        igpu.hardware = Some(true);
        let phantom = RawAdapter {
            index: 2,
            hardware: None,
            ..real.clone()
        };
        let kept: Vec<u32> = dedupe(&[&real, &igpu, &phantom])
            .iter()
            .map(|a| a.index)
            .collect();
        assert_eq!(kept, vec![0, 1]);
        // Le doublon peut aussi être énuméré avant le vrai GPU.
        let kept: Vec<u32> = dedupe(&[&phantom, &real]).iter().map(|a| a.index).collect();
        assert_eq!(kept, vec![0]);
    }

    #[test]
    fn deux_gpu_identiques_confirmes_par_dxcore() {
        let mut a = adapter(0, "NVIDIA GeForce RTX 3090", VENDOR_NVIDIA, 0x2204, 7);
        a.hardware = Some(true);
        let b = RawAdapter {
            index: 1,
            ..a.clone()
        };
        assert_eq!(dedupe(&[&a, &b]).len(), 2);
    }

    #[test]
    fn sans_dxcore_les_gpu_differents_restent() {
        let a = adapter(0, "NVIDIA GeForce RTX 4070", VENDOR_NVIDIA, 0x2786, 1);
        let b = adapter(1, "Intel(R) UHD Graphics 770", VENDOR_INTEL, 0x4680, 1);
        assert_eq!(dedupe(&[&a, &b]).len(), 2);
    }

    #[test]
    fn adaptateurs_logiciels_ignores() {
        let mut basic = adapter(
            0,
            "Microsoft Basic Render Driver",
            VENDOR_MICROSOFT,
            0x8C,
            0,
        );
        assert!(!is_physical(&basic));
        basic.vendor_id = 0x1234;
        basic.software = true;
        assert!(!is_physical(&basic));
        let mut virt = adapter(1, "Écran virtuel", 0x1234, 1, 0);
        virt.hardware = Some(false);
        assert!(!is_physical(&virt));
        virt.hardware = None;
        assert!(is_physical(&virt));
    }
}
