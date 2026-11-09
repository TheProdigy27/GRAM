//! Collecte des données brutes sur la machine. Seule partie de `gram-hw` qui fait des I/O.

mod common;
#[cfg(windows)]
mod windows;

use crate::raw::RawHardware;
use std::time::Instant;

/// Lit le matériel de la machine. Bloquant (quelques centaines de ms, surtout WMI) :
/// à appeler depuis `spawn_blocking`. N'échoue jamais : chaque source manquante est
/// signalée dans [`RawHardware::notes`] ou laissée vide.
pub fn collect() -> RawHardware {
    let start = Instant::now();
    #[cfg(windows)]
    let raw = windows::collect(common::collect);
    #[cfg(not(windows))]
    let raw = {
        let mut raw = common::collect();
        raw.notes.push(
            "Détection des GPU disponible uniquement sous Windows : CPU et RAM seulement.".into(),
        );
        raw
    };
    tracing::debug!(
        elapsed_ms = start.elapsed().as_millis() as u64,
        "matériel collecté"
    );
    raw
}
