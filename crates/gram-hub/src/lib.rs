//! Données Hugging Face (§7 du plan) : client HTTP avec limites de débit, pagination, cache
//! local, analyse des fichiers GGUF, en-têtes GGUF, dédoublonnage et catalogue embarqué.
//!
//! - [`api`] : seul module qui connaît le format de `/api/models` ;
//! - [`http`] : [`Hub`] (limiteur, 429, compteurs) au-dessus d'un [`Transport`] ;
//! - [`files`], [`gguf`], [`config`], [`dedup`], [`catalog`] : traitements purs, testés sur
//!   des réponses réelles enregistrées dans `tests/fixtures/hf/` ;
//! - [`cache`] : `%LOCALAPPDATA%\fr.gram.desktop\cache\` ;
//! - [`sync`] : la stratégie de collecte (§7.3), et le repli hors ligne ;
//! - [`snapshot`] : `data/seed_catalog.json`, embarqué dans le binaire.
//!
//! Le token Hugging Face ([`HfToken`]) ne sort jamais de ce crate : il n'est ni affiché, ni
//! sérialisé, ni écrit dans un fichier ou un message d'erreur.

pub mod api;
pub mod cache;
pub mod catalog;
pub mod config;
pub mod dedup;
mod error;
pub mod files;
pub mod gguf;
pub mod http;
pub mod snapshot;
pub mod sync;

#[cfg(test)]
mod test_support;

pub use cache::Cache;
pub use catalog::{ArchRecord, Catalog, RepoDetail};
pub use error::{HubError, Result};
pub use http::{HfToken, Hub, HubEvent, Policy, ReqwestTransport, Transport};
pub use sync::{SyncOptions, SyncOutcome, enrich, load_offline, max_params_for, sync};
