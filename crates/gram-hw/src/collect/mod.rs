//! Collecte des données brutes sur la machine. Seule partie de `gram-hw` qui fait des I/O.

mod common;
#[cfg(target_os = "macos")]
mod macos;
// Lecture de la réponse testée sur tous les systèmes ; lancé seulement sous macOS.
#[cfg(any(target_os = "macos", test))]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod system_profiler;
#[cfg(windows)]
mod windows;

use crate::raw::RawHardware;
use std::time::Instant;

/// Lit le matériel de la machine. Bloquant (quelques centaines de ms, surtout WMI sous Windows
/// et `system_profiler` sous macOS) : à appeler depuis `spawn_blocking`. N'échoue jamais :
/// chaque source manquante est signalée dans [`RawHardware::notes`] ou laissée vide.
pub fn collect() -> RawHardware {
    let start = Instant::now();
    #[cfg(windows)]
    let raw = windows::collect(common::collect);
    #[cfg(target_os = "macos")]
    let raw = macos::collect(common::collect);
    #[cfg(not(any(windows, target_os = "macos")))]
    let raw = {
        let mut raw = common::collect();
        raw.notes.push(
            "Détection des GPU disponible uniquement sous Windows et macOS : CPU et RAM seulement."
                .into(),
        );
        raw
    };
    tracing::debug!(
        elapsed_ms = start.elapsed().as_millis() as u64,
        "matériel collecté"
    );
    raw
}
