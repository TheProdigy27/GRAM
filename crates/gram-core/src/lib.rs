//! Cœur de GRAM : types partagés, moteur d'estimation et scoring.
//!
//! Ce crate est **pur** : aucune I/O, aucune dépendance à l'OS. Il compile partout
//! et toute sa logique est couverte par des tests unitaires.
//!
//! Les types exposés au frontend dérivent `Serialize`, `Deserialize` et [`ts_rs::TS`] ;
//! `cargo test` régénère leurs équivalents TypeScript dans
//! `app/ui/src/app/core/generated/` (voir `.cargo/config.toml`).

pub mod estimate;
pub mod hardware;
pub mod model;
pub mod runtime;

pub use estimate::{Estimate, FitLevel, RunMode};
pub use hardware::{
    Backends, CpuInfo, DiskInfo, GpuInfo, HardwareProfile, Overrides, RamInfo, RamKind, Source,
    Vendor,
};
pub use model::{ArchInfo, ArchSource, ModelEntry, Pipeline, QuantVariant, UseCase};
pub use runtime::{RuntimeStatus, RuntimesStatus};

/// Version du crate, affichée dans l'interface.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
