//! API locale d'Ollama (phase 7 du plan) : modèles installés (`GET /api/tags`), mesure de la
//! vitesse de génération (`POST /api/generate`) et de la mémoire occupée (`GET /api/ps`).
//!
//! - [`addr`] : adresse de l'API d'après `OLLAMA_HOST` (partagée avec la détection des runtimes) ;
//! - [`api`] : seul module qui connaît le format des réponses d'Ollama (testé sur
//!   `tests/fixtures/ollama/`) ;
//! - [`client`] : [`OllamaClient`], requêtes HTTP vers `localhost` ;
//! - [`measure`] : le protocole de mesure (chargement, échauffement, génération, mémoire).
//!
//! La comparaison avec l'estimation et la calibration sont dans `gram-core::calibration` (pur).

pub mod addr;
pub mod api;
pub mod client;
mod error;
pub mod measure;

#[cfg(any(test, feature = "test-support"))]
pub mod testing;

pub use addr::{OLLAMA_DEFAULT_PORT, ollama_addr};
pub use client::OllamaClient;
pub use error::{OllamaError, Result};
pub use measure::{MEASURE_TOKENS, measure};
