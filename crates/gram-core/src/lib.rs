//! Cœur de GRAM : types partagés, moteur d'estimation, scoring et classement.
//!
//! Ce crate est **pur** : aucune I/O, aucune dépendance à l'OS. Il compile partout
//! et toute sa logique est couverte par des tests unitaires.
//!
//! Les types exposés au frontend dérivent `Serialize`, `Deserialize` et [`ts_rs::TS`] ;
//! `cargo test` régénère leurs équivalents TypeScript dans
//! `app/ui/src/app/core/generated/` (voir `.cargo/config.toml`).

pub mod calibration;
pub mod catalog;
pub mod estimate;
pub mod export;
pub mod family;
pub mod fmt;
pub mod hardware;
pub mod launch;
pub mod model;
pub mod rank;
pub mod runtime;
pub mod score;
pub mod settings;

pub use calibration::{Calibration, LoadedModel, MeasureRequest, Measurement, OllamaRun};
pub use catalog::{
    DataSource, DataSourceKind, RateLimitInfo, SyncPhase, SyncProgress, SyncSummary,
};
pub use estimate::{
    Estimate, EstimateParams, FitLevel, KvCacheType, MemoryBreakdown, MemoryBudget, QuantTable,
    RunMode, VariantEstimate, Workload, estimate_variants,
};
pub use export::ExportFormat;
pub use hardware::{
    Backends, CpuArch, CpuInfo, DEFAULT_GPU_BANDWIDTH_GBS, DEFAULT_RAM_BANDWIDTH_GBS, DiskInfo,
    GpuInfo, HardwareProfile, Os, Overrides, RamInfo, RamKind, Source, Vendor,
};
pub use launch::{CommandLine, LaunchPlan, Runtime};
pub use model::{ArchInfo, ArchSource, ModelEntry, Pipeline, QuantVariant, RepoRef, UseCase};
pub use rank::{
    DetailRequest, ModelDetail, PickKind, RankContext, RankItem, RankPick, RankRequest,
    RankResponse, VariantDetail, model_detail, rank,
};
pub use runtime::{OllamaModel, RuntimeStatus, RuntimesStatus};
pub use score::{Curation, Score, ScoreWeights};
pub use settings::{Settings, UseCaseProfile, UseCaseProfiles};

/// Version du crate, affichée dans l'interface.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
