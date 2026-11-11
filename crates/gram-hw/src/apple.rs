//! Table `data/apple_chips.json` : mémoire unifiée des puces Apple Silicon. Sans I/O.
//!
//! macOS ne donne ni la vitesse ni la largeur du bus de la mémoire d'un Mac Apple Silicon
//! (`system_profiler` ne cite que son type) : elles viennent de la table, d'après le nom de la
//! puce, qui est aussi le nom du processeur (« Apple M1 Pro »).

use gram_core::RamKind;
use serde::Deserialize;
use std::sync::OnceLock;

const EMBEDDED: &str = include_str!("../../../data/apple_chips.json");

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AppleChip {
    /// Nom commercial (ex. « Apple M4 Max »).
    pub name: String,
    pub memory: RamKind,
    pub speed_mts: u32,
    pub bus_bits: u32,
    /// Version au bus réduit : retenue si le GPU a au plus ce nombre de cœurs.
    #[serde(default)]
    pub max_gpu_cores: Option<u32>,
}

impl AppleChip {
    /// Canaux de 64 bits (comme la RAM d'un PC) : 2 pour un M1, 16 pour un Ultra.
    pub fn channels(&self) -> u32 {
        (self.bus_bits / 64).max(1)
    }

    /// Bande passante théorique en Go/s.
    pub fn bandwidth_gbs(&self) -> f32 {
        crate::ram::bandwidth_gbs(self.speed_mts, self.channels())
    }
}

/// Puce trouvée dans la table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChipMatch<'a> {
    pub chip: &'a AppleChip,
    /// `false` : puce absente de la table, remplacée par la dernière de la même gamme.
    pub exact: bool,
}

#[derive(Deserialize)]
struct ChipFile {
    chips: Vec<AppleChip>,
}

#[derive(Debug, Clone)]
pub struct AppleChips {
    chips: Vec<AppleChip>,
}

/// Nom de puce comparable : minuscules, espaces simples, sans mention entre parenthèses
/// (« Apple M2 (Virtual) » dans une machine virtuelle).
fn normalize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut depth = 0u32;
    for c in name.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.extend(c.to_lowercase()),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Le processeur est une puce Apple Silicon (« Apple M1 », « Apple A18 Pro »…).
pub fn is_apple_chip(cpu_name: &str) -> bool {
    normalize(cpu_name).starts_with("apple ")
}

/// Famille, génération et gamme d'un nom normalisé : « apple m4 max » → (`m`, 4, `max`).
fn family(normalized: &str) -> Option<(char, u32, String)> {
    let rest = normalized.strip_prefix("apple ")?;
    let mut chars = rest.chars();
    let letter = chars.next().filter(char::is_ascii_alphabetic)?;
    let rest = chars.as_str();
    let digits = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let generation = rest[..digits].parse().ok()?;
    Some((letter, generation, rest[digits..].trim().to_string()))
}

impl AppleChips {
    /// Table embarquée dans le binaire.
    pub fn embedded() -> &'static AppleChips {
        static CHIPS: OnceLock<AppleChips> = OnceLock::new();
        CHIPS.get_or_init(|| {
            AppleChips::from_json(EMBEDDED).expect("data/apple_chips.json invalide")
        })
    }

    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        let file: ChipFile = serde_json::from_str(json)?;
        Ok(Self { chips: file.chips })
    }

    pub fn all(&self) -> impl Iterator<Item = &AppleChip> {
        self.chips.iter()
    }

    /// Puce d'un processeur. Parmi les versions d'une même puce, celle dont `max_gpu_cores`
    /// couvre les cœurs du GPU ; sans nombre de cœurs, la version réduite (prudent).
    /// Une puce inconnue (« Apple M7 Max ») prend la dernière connue de la même gamme
    /// (« Apple M5 Max »), signalée par `exact: false`.
    pub fn lookup(&self, cpu_name: &str, gpu_cores: Option<u32>) -> Option<ChipMatch<'_>> {
        let wanted = normalize(cpu_name);
        if let Some(chip) = self.pick(|c| normalize(&c.name) == wanted, gpu_cores) {
            return Some(ChipMatch { chip, exact: true });
        }
        let (letter, generation, tier) = family(&wanted)?;
        let same_range = |c: &AppleChip| {
            family(&normalize(&c.name))
                .filter(|(l, g, t)| *l == letter && *t == tier && *g < generation)
                .map(|(_, g, _)| g)
        };
        let latest = self.chips.iter().filter_map(same_range).max()?;
        let chip = self.pick(|c| same_range(c) == Some(latest), gpu_cores)?;
        Some(ChipMatch { chip, exact: false })
    }

    /// Version d'une puce : la première (les versions réduites, qui portent `max_gpu_cores`,
    /// d'abord) dont le GPU couvre `gpu_cores`.
    fn pick(
        &self,
        same: impl Fn(&AppleChip) -> bool,
        gpu_cores: Option<u32>,
    ) -> Option<&AppleChip> {
        let mut versions: Vec<&AppleChip> = self.chips.iter().filter(|c| same(c)).collect();
        versions.sort_by_key(|c| c.max_gpu_cores.unwrap_or(u32::MAX));
        versions
            .into_iter()
            .find(|c| match (c.max_gpu_cores, gpu_cores) {
                (Some(max), Some(cores)) => cores <= max,
                _ => true,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn bw(name: &str, cores: Option<u32>) -> Option<f32> {
        AppleChips::embedded()
            .lookup(name, cores)
            .map(|m| m.chip.bandwidth_gbs())
    }

    #[test]
    fn table_embarquee_coherente() {
        let chips = AppleChips::embedded();
        let mut seen = HashSet::new();
        for c in chips.all() {
            assert!(is_apple_chip(&c.name), "{}", c.name);
            assert!(family(&normalize(&c.name)).is_some(), "{}", c.name);
            assert!(c.bus_bits % 64 == 0 && c.bus_bits >= 64, "{}", c.name);
            assert!((4000..=12_000).contains(&c.speed_mts), "{}", c.name);
            assert!(
                matches!(
                    c.memory,
                    RamKind::Lpddr4x | RamKind::Lpddr5 | RamKind::Lpddr5x
                ),
                "{}",
                c.name
            );
            assert!(seen.insert((c.name.clone(), c.max_gpu_cores)), "{}", c.name);
        }
        assert!(seen.len() >= 20);
    }

    #[test]
    fn bandes_passantes_annoncees_par_apple() {
        // Valeurs théoriques ; Apple arrondit (« 200 Go/s » pour 204,8, « 800 » pour 819,2).
        assert_eq!(bw("Apple M1", None), Some(68.256));
        assert_eq!(bw("Apple M1 Pro", Some(16)), Some(204.8));
        assert_eq!(bw("Apple M1 Max", Some(32)), Some(409.6));
        assert_eq!(bw("Apple M2 Ultra", Some(76)), Some(819.2));
        assert_eq!(bw("Apple M3 Pro", Some(18)), Some(153.6));
        assert_eq!(bw("Apple M4", Some(10)), Some(120.0));
        assert_eq!(bw("Apple M4 Pro", Some(20)), Some(273.056));
        assert_eq!(bw("Apple M5", Some(10)), Some(153.6));
        assert_eq!(bw("Apple M5 Pro", None), Some(307.2));
        assert_eq!(bw("Apple M5 Ultra", None), Some(1228.8));
        assert_eq!(bw("Apple A18 Pro", Some(5)), Some(60.0));
    }

    #[test]
    fn versions_d_une_meme_puce_par_coeurs_gpu() {
        assert_eq!(bw("Apple M3 Max", Some(30)), Some(307.2));
        assert_eq!(bw("Apple M3 Max", Some(40)), Some(409.6));
        assert_eq!(bw("Apple M4 Max", Some(32)), Some(409.584));
        assert_eq!(bw("Apple M4 Max", Some(40)), Some(546.112));
        assert_eq!(bw("Apple M5 Max", Some(32)), Some(460.8));
        assert_eq!(bw("Apple M5 Max", Some(40)), Some(614.4));
        // Cœurs inconnus : la version réduite, par prudence.
        assert_eq!(bw("Apple M4 Max", None), Some(409.584));
    }

    #[test]
    fn nom_tolere_casse_espaces_et_machine_virtuelle() {
        let chips = AppleChips::embedded();
        let m = chips.lookup("  apple   m2 pro ", None).unwrap();
        assert_eq!((m.chip.name.as_str(), m.exact), ("Apple M2 Pro", true));
        let m = chips.lookup("Apple M2 (Virtual)", None).unwrap();
        assert_eq!(m.chip.name, "Apple M2");
    }

    #[test]
    fn puce_inconnue_d_apres_sa_gamme() {
        let chips = AppleChips::embedded();
        let m = chips.lookup("Apple M7 Max", Some(48)).unwrap();
        assert!(!m.exact);
        assert_eq!(m.chip.name, "Apple M5 Max");
        assert_eq!(m.chip.bandwidth_gbs(), 614.4);
        let m = chips.lookup("Apple M9", None).unwrap();
        assert_eq!(m.chip.name, "Apple M5");
        assert_eq!(
            chips.lookup("Apple A19 Pro", None).unwrap().chip.name,
            "Apple A18 Pro"
        );
        // Gamme inconnue, ou pas une puce Apple : rien.
        assert!(chips.lookup("Apple M7 Hyper", None).is_none());
        assert!(
            chips
                .lookup("Intel(R) Core(TM) i9-9980HK CPU @ 2.40GHz", None)
                .is_none()
        );
        assert!(chips.lookup("", None).is_none());
    }

    #[test]
    fn canaux_de_64_bits() {
        let chips = AppleChips::embedded();
        let channels = |name| chips.lookup(name, None).unwrap().chip.channels();
        assert_eq!(channels("Apple M1"), 2);
        assert_eq!(channels("Apple M3 Pro"), 3);
        assert_eq!(channels("Apple M1 Ultra"), 16);
        assert_eq!(channels("Apple A18 Pro"), 1);
        assert!(is_apple_chip("Apple M1 Pro") && !is_apple_chip("AppleWebKit"));
    }
}
