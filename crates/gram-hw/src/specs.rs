//! Table `data/gpu_specs.json` : bande passante des GPU courants, et liste du mode simulation.

use gram_core::Vendor;
use serde::Deserialize;
use std::sync::OnceLock;

const EMBEDDED: &str = include_str!("../../../data/gpu_specs.json");

/// Écart de VRAM toléré entre la table et le GPU détecté (les pilotes annoncent un peu moins).
const VRAM_TOLERANCE_GIB: f64 = 1.5;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GpuSpec {
    /// Nom commercial complet (ex. « NVIDIA GeForce RTX 3060 »).
    pub name: String,
    pub vram_gb: u32,
    pub bandwidth_gbs: f32,
}

impl GpuSpec {
    /// Libellé unique (nom + VRAM), proposé dans le mode simulation.
    pub fn label(&self) -> String {
        format!("{} {} Go", self.name, self.vram_gb)
    }

    pub fn vendor(&self) -> Vendor {
        let lower = self.name.to_ascii_lowercase();
        if lower.starts_with("nvidia") {
            Vendor::Nvidia
        } else if lower.starts_with("amd") {
            Vendor::Amd
        } else if lower.starts_with("intel") {
            Vendor::Intel
        } else {
            Vendor::Other
        }
    }

    pub fn vram_bytes(&self) -> u64 {
        u64::from(self.vram_gb) << 30
    }
}

#[derive(Deserialize)]
struct SpecFile {
    gpus: Vec<GpuSpec>,
}

#[derive(Debug, Clone)]
pub struct GpuSpecs {
    entries: Vec<(NormalizedName, GpuSpec)>,
}

impl GpuSpecs {
    /// Table embarquée dans le binaire.
    pub fn embedded() -> &'static GpuSpecs {
        static SPECS: OnceLock<GpuSpecs> = OnceLock::new();
        SPECS.get_or_init(|| GpuSpecs::from_json(EMBEDDED).expect("data/gpu_specs.json invalide"))
    }

    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        let file: SpecFile = serde_json::from_str(json)?;
        Ok(Self {
            entries: file
                .gpus
                .into_iter()
                .map(|s| (normalize_name(&s.name), s))
                .collect(),
        })
    }

    pub fn all(&self) -> impl Iterator<Item = &GpuSpec> {
        self.entries.iter().map(|(_, s)| s)
    }

    /// Cherche un GPU détecté : même nom normalisé, version portable ou non,
    /// et VRAM la plus proche (à [`VRAM_TOLERANCE_GIB`] près).
    pub fn lookup(&self, detected_name: &str, vram_bytes: u64) -> Option<&GpuSpec> {
        let wanted = normalize_name(detected_name);
        if wanted.key.is_empty() {
            return None;
        }
        let vram_gib = vram_bytes as f64 / f64::from(1u32 << 30);
        self.entries
            .iter()
            .filter(|(n, _)| *n == wanted)
            .map(|(_, s)| (s, (f64::from(s.vram_gb) - vram_gib).abs()))
            .filter(|(_, gap)| *gap <= VRAM_TOLERANCE_GIB)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(s, _)| s)
    }

    /// Retrouve une entrée par son [`GpuSpec::label`] (mode simulation), sans tenir compte de la casse.
    pub fn by_label(&self, label: &str) -> Option<&GpuSpec> {
        let label = label.trim();
        self.all().find(|s| s.label().eq_ignore_ascii_case(label))
    }
}

/// Nom de GPU normalisé pour la comparaison (§13 du plan).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedName {
    /// Ex. « rtx 4060 ti », « rx 7800 xt », « arc a770 ».
    pub key: String,
    /// Version portable (« Laptop GPU ») : bande passante différente.
    pub laptop: bool,
}

/// Minuscules, sans marques (`nvidia`, `geforce`, `amd`, `radeon`, `intel`), sans `(tm)` / `(r)`,
/// sans `graphics` ni `gpu` ; « laptop » est retiré du nom et noté à part.
pub fn normalize_name(name: &str) -> NormalizedName {
    let lower = name
        .to_lowercase()
        .replace("(tm)", " ")
        .replace("(r)", " ")
        .replace(['®', '™'], " ");
    let mut laptop = false;
    let words: Vec<&str> = lower
        .split_whitespace()
        .filter(|w| match *w {
            "laptop" => {
                laptop = true;
                false
            }
            "nvidia" | "geforce" | "amd" | "radeon" | "intel" | "graphics" | "gpu" => false,
            _ => true,
        })
        .collect();
    NormalizedName {
        key: words.join(" "),
        laptop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const GIB: u64 = 1 << 30;

    fn key(name: &str) -> String {
        normalize_name(name).key
    }

    #[test]
    fn normalisation_des_noms() {
        assert_eq!(key("NVIDIA GeForce RTX 4060 Ti"), "rtx 4060 ti");
        assert_eq!(key("AMD Radeon RX 7800 XT"), "rx 7800 xt");
        assert_eq!(key("Intel(R) Arc(TM) A770 Graphics"), "arc a770");
        assert_eq!(key("NVIDIA RTX A4000"), "rtx a4000");
        assert_eq!(key("AMD Radeon(TM) Graphics"), "");
        let laptop = normalize_name("NVIDIA GeForce RTX 4070 Laptop GPU");
        assert_eq!(laptop.key, "rtx 4070");
        assert!(laptop.laptop);
        assert!(!normalize_name("NVIDIA GeForce RTX 4070").laptop);
    }

    #[test]
    fn table_embarquee_coherente() {
        let specs = GpuSpecs::embedded();
        let mut labels = HashSet::new();
        for s in specs.all() {
            assert!(
                labels.insert(s.label()),
                "libellé en double : {}",
                s.label()
            );
            assert_ne!(
                s.vendor(),
                Vendor::Other,
                "constructeur inconnu : {}",
                s.name
            );
            assert!(!normalize_name(&s.name).key.is_empty(), "{}", s.name);
            assert!(
                s.bandwidth_gbs > 50.0 && s.bandwidth_gbs < 3000.0,
                "{}",
                s.name
            );
            assert!(s.vram_gb >= 4, "{}", s.name);
        }
        assert!(labels.len() >= 50);
    }

    #[test]
    fn valeurs_du_plan() {
        let specs = GpuSpecs::embedded();
        let bw = |name: &str, gb: u64| specs.lookup(name, gb * GIB).map(|s| s.bandwidth_gbs);
        assert_eq!(bw("NVIDIA GeForce RTX 3060", 12), Some(360.0));
        assert_eq!(bw("NVIDIA GeForce RTX 3090", 24), Some(936.0));
        assert_eq!(bw("NVIDIA GeForce RTX 4060", 8), Some(272.0));
        assert_eq!(bw("NVIDIA GeForce RTX 4060 Ti", 16), Some(288.0));
        assert_eq!(bw("NVIDIA GeForce RTX 4070", 12), Some(504.0));
        assert_eq!(bw("NVIDIA GeForce RTX 4090", 24), Some(1008.0));
        assert_eq!(bw("NVIDIA GeForce RTX 5070", 12), Some(672.0));
        assert_eq!(bw("NVIDIA GeForce RTX 5080", 16), Some(960.0));
        assert_eq!(bw("NVIDIA GeForce RTX 5090", 32), Some(1792.0));
        assert_eq!(bw("AMD Radeon RX 7800 XT", 16), Some(624.0));
        assert_eq!(bw("AMD Radeon RX 7900 XTX", 24), Some(960.0));
        assert_eq!(bw("Intel(R) Arc(TM) A770 Graphics", 16), Some(560.0));
    }

    #[test]
    fn la_vram_departage_les_variantes() {
        let specs = GpuSpecs::embedded();
        // Les pilotes annoncent un peu moins que la capacité commerciale.
        let r12 = specs.lookup("NVIDIA GeForce RTX 3060", 12_884_901_888 - 300 * (1 << 20));
        assert_eq!(r12.unwrap().bandwidth_gbs, 360.0);
        let r8 = specs.lookup("NVIDIA GeForce RTX 3060", 8 * GIB);
        assert_eq!(r8.unwrap().bandwidth_gbs, 240.0);
        // Aucune variante proche : pas de correspondance plutôt qu'une valeur fausse.
        assert!(specs.lookup("NVIDIA GeForce RTX 3060", 4 * GIB).is_none());
    }

    #[test]
    fn portable_et_desktop_distincts() {
        let specs = GpuSpecs::embedded();
        let laptop = specs.lookup("NVIDIA GeForce RTX 4060 Laptop GPU", 8 * GIB);
        assert_eq!(laptop.unwrap().bandwidth_gbs, 256.0);
        assert!(
            specs
                .lookup("NVIDIA GeForce RTX 4090 Laptop GPU", 24 * GIB)
                .is_none()
        );
    }

    #[test]
    fn inconnus_et_igpu_sans_correspondance() {
        let specs = GpuSpecs::embedded();
        assert!(specs.lookup("AMD Radeon(TM) Graphics", 2 * GIB).is_none());
        assert!(specs.lookup("Microsoft Basic Render Driver", 0).is_none());
        assert!(
            specs
                .lookup("NVIDIA GeForce RTX 4090 D", 24 * GIB)
                .is_none()
        );
    }

    #[test]
    fn simulation_par_libelle() {
        let specs = GpuSpecs::embedded();
        let s = specs.by_label("nvidia geforce rtx 3060 12 go").unwrap();
        assert_eq!(s.bandwidth_gbs, 360.0);
        assert_eq!(s.vram_bytes(), 12 * GIB);
        assert_eq!(s.vendor(), Vendor::Nvidia);
        assert!(specs.by_label("GPU imaginaire 99 Go").is_none());
    }
}
