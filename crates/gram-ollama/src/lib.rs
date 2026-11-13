//! API locale d'Ollama (phase 7 du plan) : modèles installés (`GET /api/tags`), mesure de la
//! vitesse de génération (`POST /api/generate`) et de la mémoire occupée (`GET /api/ps`) ; ajout
//! d'un modèle pour GRAM Harness (téléchargement, modèle dérivé, suppression, capacités) ;
//! discussion en flux pour GRAM Harness (`POST /api/chat`).
//!
//! - [`addr`] : adresse de l'API d'après `OLLAMA_HOST` (partagée avec la détection des runtimes) ;
//! - [`api`] : seul module qui connaît le format des requêtes et des réponses d'Ollama (testé
//!   sur `tests/fixtures/ollama/`) ;
//! - [`client`] : [`OllamaClient`], requêtes HTTP vers `localhost`, flux NDJSON compris ;
//! - [`dirs`] : dossier des modèles, pour vérifier la place sur le disque ;
//! - [`install`] : l'ajout d'un modèle (téléchargement si besoin, modèle dérivé, capacités) ;
//! - [`measure`] : le protocole de mesure (chargement, échauffement, génération, mémoire).
//!
//! La comparaison avec l'estimation, la calibration, le suivi du téléchargement et la place sur
//! le disque sont dans `gram-core` (`calibration`, `harness`), sans I/O.

pub mod addr;
pub mod api;
pub mod client;
pub mod dirs;
mod error;
pub mod install;
pub mod measure;
mod ndjson;

#[cfg(any(test, feature = "test-support"))]
pub mod testing;

pub use addr::{OLLAMA_DEFAULT_PORT, ollama_addr};
pub use api::{
    Capability, ChatChunk, ChatDone, ChatMessage, ChatOptions, ChatRequest, ChatRole, ModelShow,
    PullEvent, TaggedModel,
};
pub use client::OllamaClient;
pub use dirs::models_dir;
pub use error::{OllamaError, Result};
pub use install::{Installed, install};
pub use measure::{MEASURE_TOKENS, measure};
