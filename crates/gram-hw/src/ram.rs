//! Calculs sur la RAM (§6.2 du plan) : type, vitesse, canaux, bande passante. Sans I/O.

use crate::raw::RawMemoryModule;
use gram_core::{RamKind, Source};
use std::collections::BTreeSet;

/// `SMBIOSMemoryType` → type de RAM (SMBIOS 3.x, structure de type 17).
pub fn kind_from_smbios(code: u32) -> RamKind {
    match code {
        0x18 => RamKind::Ddr3,
        0x1A => RamKind::Ddr4,
        0x1E => RamKind::Lpddr4,
        0x22 => RamKind::Ddr5,
        0x23 => RamKind::Lpddr5,
        _ => RamKind::Unknown,
    }
}

/// Type commun aux barrettes (le plus fréquent).
pub fn kind_of(modules: &[RawMemoryModule]) -> RamKind {
    let kinds: Vec<RamKind> = modules
        .iter()
        .filter_map(|m| m.smbios_type)
        .map(kind_from_smbios)
        .collect();
    kinds
        .iter()
        .copied()
        .max_by_key(|k| kinds.iter().filter(|x| *x == k).count())
        .unwrap_or(RamKind::Unknown)
}

pub fn is_lpddr(kind: RamKind) -> bool {
    matches!(kind, RamKind::Lpddr4 | RamKind::Lpddr5)
}

/// Débit en dessous duquel la valeur est sûrement en MHz (horloge réelle = moitié du débit),
/// comme sur les BIOS antérieurs à SMBIOS 3.1.
fn mhz_threshold(kind: RamKind) -> Option<u32> {
    match kind {
        RamKind::Ddr3 => Some(1000),
        RamKind::Ddr4 => Some(2000),
        RamKind::Ddr5 => Some(4000),
        RamKind::Lpddr4 => Some(2400),
        RamKind::Lpddr5 => Some(5000),
        RamKind::Unknown => None,
    }
}

/// Vitesse d'une barrette en MT/s : la vitesse configurée, sinon la vitesse annoncée.
pub fn module_speed(kind: RamKind, m: &RawMemoryModule) -> Option<u32> {
    let raw = m
        .configured_speed
        .filter(|&v| v > 0)
        .or(m.speed.filter(|&v| v > 0))?;
    match mhz_threshold(kind) {
        Some(threshold) if raw < threshold => Some(raw * 2),
        _ => Some(raw),
    }
}

/// Vitesse effective de l'ensemble : celle de la barrette la plus lente.
pub fn speed_of(kind: RamKind, modules: &[RawMemoryModule]) -> Option<u32> {
    modules.iter().filter_map(|m| module_speed(kind, m)).min()
}

/// Nombre de canaux de 64 bits.
///
/// - LPDDR soudée : somme des largeurs de bus / 64 (souvent mal décrit, d'où l'avertissement).
/// - Sinon, canaux distincts lus dans `BankLabel` (« P0 CHANNEL A ») ou `DeviceLocator`
///   (« Controller0-ChannelA-DIMM0 », « DIMM_A1 ») quand toutes les barrettes en ont un.
/// - Sinon : 1 barrette → 1 canal, 2 ou plus → 2 canaux (plateforme grand public).
pub fn estimate_channels(kind: RamKind, modules: &[RawMemoryModule]) -> (u32, Source) {
    if modules.is_empty() {
        return (2, Source::Default);
    }
    if is_lpddr(kind) {
        let widths: Option<u32> = modules
            .iter()
            .map(|m| m.data_width.filter(|&w| w > 0))
            .sum();
        let channels = widths.map_or(2, |w| (w / 64).clamp(1, 8));
        return (channels, Source::Estimated);
    }
    let labelled = channel_count(modules.iter().map(|m| m.bank_label.as_str()))
        .or_else(|| channel_count(modules.iter().map(|m| m.device_locator.as_str())));
    let channels = labelled.unwrap_or_else(|| modules.len().min(2) as u32);
    (channels, Source::Estimated)
}

/// Nombre de canaux distincts, si chaque libellé en désigne un.
fn channel_count<'a>(labels: impl Iterator<Item = &'a str>) -> Option<u32> {
    let mut keys = BTreeSet::new();
    for label in labels {
        keys.insert(channel_key(label)?);
    }
    (!keys.is_empty()).then_some(keys.len() as u32)
}

/// Identifiant de canal dans un libellé : la lettre après « CHANNEL » (précédée de ce qui la
/// précède, pour distinguer « Controller0-ChannelA » de « Controller1-ChannelA »),
/// ou la lettre de « DIMM_A1 » / « DIMMA2 ».
fn channel_key(label: &str) -> Option<String> {
    let up = label.trim().to_ascii_uppercase();
    let seps: &[char] = &[' ', '-', '_'];
    if let Some(pos) = up.find("CHANNEL") {
        let letter = up[pos + "CHANNEL".len()..]
            .trim_start_matches(seps)
            .chars()
            .next()
            .filter(char::is_ascii_alphabetic)?;
        let prefix = up[..pos].trim_end_matches(seps);
        return Some(format!("{prefix}/{letter}"));
    }
    if let Some(pos) = up.find("DIMM") {
        let mut rest = up[pos + "DIMM".len()..].trim_start_matches(seps).chars();
        let (letter, digit) = (rest.next()?, rest.next()?);
        if letter.is_ascii_alphabetic() && digit.is_ascii_digit() {
            return Some(letter.to_string());
        }
    }
    None
}

/// Bande passante théorique en Go/s = MT/s × 8 octets × canaux.
pub fn bandwidth_gbs(speed_mts: u32, channels: u32) -> f32 {
    (f64::from(speed_mts) * 8.0 * f64::from(channels) / 1000.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(bank: &str, locator: &str) -> RawMemoryModule {
        RawMemoryModule {
            capacity: 16 << 30,
            speed: Some(3200),
            configured_speed: Some(3200),
            smbios_type: Some(26),
            data_width: Some(64),
            device_locator: locator.into(),
            bank_label: bank.into(),
        }
    }

    #[test]
    fn types_smbios() {
        assert_eq!(kind_from_smbios(24), RamKind::Ddr3);
        assert_eq!(kind_from_smbios(26), RamKind::Ddr4);
        assert_eq!(kind_from_smbios(30), RamKind::Lpddr4);
        assert_eq!(kind_from_smbios(34), RamKind::Ddr5);
        assert_eq!(kind_from_smbios(35), RamKind::Lpddr5);
        assert_eq!(kind_from_smbios(0), RamKind::Unknown);
    }

    #[test]
    fn exemples_de_bande_passante_du_plan() {
        assert_eq!(bandwidth_gbs(3200, 2), 51.2);
        assert_eq!(bandwidth_gbs(5600, 2), 89.6);
        assert_eq!(bandwidth_gbs(6000, 2), 96.0);
        assert_eq!(bandwidth_gbs(3200, 1), 25.6);
    }

    #[test]
    fn vitesse_configuree_prioritaire() {
        let mut m = module("", "");
        m.smbios_type = Some(34);
        m.speed = Some(4800);
        m.configured_speed = Some(6000);
        assert_eq!(module_speed(RamKind::Ddr5, &m), Some(6000));
        m.configured_speed = Some(0);
        assert_eq!(module_speed(RamKind::Ddr5, &m), Some(4800));
        m.configured_speed = None;
        m.speed = None;
        assert_eq!(module_speed(RamKind::Ddr5, &m), None);
    }

    #[test]
    fn vitesse_en_mhz_des_anciens_bios_corrigee() {
        let mut m = module("", "");
        m.configured_speed = Some(1600); // DDR4-3200 annoncée en MHz
        assert_eq!(module_speed(RamKind::Ddr4, &m), Some(3200));
        m.configured_speed = Some(2400);
        assert_eq!(module_speed(RamKind::Ddr4, &m), Some(2400));
        m.configured_speed = Some(667);
        assert_eq!(module_speed(RamKind::Ddr3, &m), Some(1334));
        assert_eq!(module_speed(RamKind::Unknown, &m), Some(667));
    }

    #[test]
    fn barrette_la_plus_lente() {
        let mut a = module("", "");
        let mut b = module("", "");
        a.configured_speed = Some(3600);
        b.configured_speed = Some(3200);
        assert_eq!(speed_of(RamKind::Ddr4, &[a, b]), Some(3200));
    }

    #[test]
    fn canaux_amd_bank_label() {
        // Ryzen 7 9800X3D de la machine de développement.
        let mods = [
            module("P0 CHANNEL A", "DIMMA2"),
            module("P0 CHANNEL B", "DIMMB2"),
        ];
        assert_eq!(
            estimate_channels(RamKind::Ddr5, &mods),
            (2, Source::Estimated)
        );
    }

    #[test]
    fn deux_barrettes_sur_le_meme_canal() {
        let mods = [
            module("BANK 0", "ChannelA-DIMM1"),
            module("BANK 1", "ChannelA-DIMM2"),
        ];
        assert_eq!(estimate_channels(RamKind::Ddr4, &mods).0, 1);
    }

    #[test]
    fn canaux_intel_par_controleur() {
        let mods = [
            module("BANK 0", "Controller0-ChannelA-DIMM0"),
            module("BANK 1", "Controller1-ChannelA-DIMM0"),
        ];
        assert_eq!(estimate_channels(RamKind::Ddr5, &mods).0, 2);
    }

    #[test]
    fn canaux_quatre_barrettes_double_canal() {
        let mods = [
            module("", "DIMM_A1"),
            module("", "DIMM_A2"),
            module("", "DIMM_B1"),
            module("", "DIMM_B2"),
        ];
        assert_eq!(estimate_channels(RamKind::Ddr4, &mods).0, 2);
    }

    #[test]
    fn libelles_illisibles_regle_par_defaut() {
        let one = [module("BANK 0", "DIMM 0")];
        assert_eq!(estimate_channels(RamKind::Ddr4, &one).0, 1);
        let four = [
            module("BANK 0", "DIMM 0"),
            module("BANK 1", "DIMM 1"),
            module("BANK 2", "DIMM 2"),
            module("BANK 3", "DIMM 3"),
        ];
        assert_eq!(estimate_channels(RamKind::Ddr4, &four).0, 2);
    }

    #[test]
    fn lpddr_par_largeur_de_bus() {
        let mut m = module("BANK 0", "Controller0-ChannelA");
        m.smbios_type = Some(35);
        m.data_width = Some(16);
        let mods = vec![m; 8];
        assert_eq!(kind_of(&mods), RamKind::Lpddr5);
        assert_eq!(
            estimate_channels(RamKind::Lpddr5, &mods),
            (2, Source::Estimated)
        );
        let mut unknown = mods.clone();
        unknown[0].data_width = None;
        assert_eq!(estimate_channels(RamKind::Lpddr5, &unknown).0, 2);
    }

    #[test]
    fn sans_barrette_valeur_par_defaut() {
        assert_eq!(
            estimate_channels(RamKind::Unknown, &[]),
            (2, Source::Default)
        );
        assert_eq!(kind_of(&[]), RamKind::Unknown);
    }
}
