//! GPU vus par Metal : nom, limite de mémoire recommandée, mémoire unifiée.

use crate::raw::RawMetalDevice;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSArray;
use objc2_metal::MTLDevice;

#[link(name = "Metal", kind = "framework")]
unsafe extern "C-unwind" {
    // Déclarée ici plutôt que `objc2_metal::MTLCopyAllDevices`, qui panique si Metal renvoie
    // NULL (machine virtuelle sans GPU) : le binaire de release s'arrêterait (`panic = "abort"`).
    fn MTLCopyAllDevices() -> *mut NSArray<ProtocolObject<dyn MTLDevice>>;
}

/// Tous les GPU Metal, dans l'ordre de `MTLCopyAllDevices`. Contrairement à
/// `MTLCreateSystemDefaultDevice`, cet appel ne fait pas basculer un Mac Intel à deux GPU sur
/// le GPU dédié.
pub fn devices() -> Vec<RawMetalDevice> {
    autoreleasepool(|_| {
        // SAFETY : fonction de Metal sans argument (macOS 10.11+). Le tableau rendu est déjà
        // retenu (NS_RETURNS_RETAINED), d'où `from_raw` ; NULL donne `None`.
        let Some(devices) = (unsafe { Retained::from_raw(MTLCopyAllDevices()) }) else {
            return Vec::new();
        };
        devices
            .iter()
            .enumerate()
            .map(|(i, d)| RawMetalDevice {
                index: i as u32,
                name: d.name().to_string(),
                registry_id: d.registryID(),
                recommended_max_working_set: d.recommendedMaxWorkingSetSize(),
                unified_memory: d.hasUnifiedMemory(),
                low_power: d.isLowPower(),
                removable: d.isRemovable(),
                gpu_cores: None,
                vram: None,
            })
            .collect()
    })
}
