//! État de GRAM Harness (`tauri::State<HarnessState>`) : client de l'API d'Ollama, dossier
//! partagé avec GRAM, capacités des modèles déjà lues. La détection d'Ollama est injectable
//! pour les tests.

use crate::error::{CmdResult, CommandError};
use crate::models::ShowCache;
use gram_link::{SharedManifest, read_manifest};
use gram_ollama::OllamaClient;
use std::path::PathBuf;
use std::sync::Arc;

/// Ollama est-il installé ? Bloquant (≈ 100 ms) : appelé seulement quand son API ne répond pas.
pub type DetectOllama = Arc<dyn Fn() -> bool + Send + Sync>;

pub struct Config {
    /// API locale d'Ollama (`OLLAMA_HOST`) ; `None` si l'adresse n'est pas utilisable.
    pub ollama: Option<OllamaClient>,
    /// Dossier partagé avec GRAM (`fr.gram.shared`) ; `None` s'il est introuvable : la liste
    /// garde alors les noms techniques.
    pub shared_dir: Option<PathBuf>,
}

pub struct HarnessState {
    config: Config,
    detect_ollama: DetectOllama,
    shows: ShowCache,
}

impl HarnessState {
    pub fn new(config: Config, detect_ollama: DetectOllama) -> Self {
        Self {
            config,
            detect_ollama,
            shows: ShowCache::default(),
        }
    }

    /// Variables d'environnement (`OLLAMA_HOST`, dossier des données locales), vraie détection.
    pub fn production() -> Self {
        Self::new(
            Config {
                ollama: OllamaClient::from_env(),
                shared_dir: gram_link::shared_dir(),
            },
            Arc::new(|| gram_hw::detect_runtimes().ollama.installed),
        )
    }

    /// Client d'Ollama, ou l'erreur à afficher si son adresse n'est pas utilisable.
    pub fn require_ollama(&self) -> CmdResult<&OllamaClient> {
        self.config.ollama.as_ref().ok_or_else(|| {
            CommandError::msg(
                "API d'Ollama inaccessible : la variable OLLAMA_HOST désigne une adresse non \
                 prise en charge (HTTPS ou hôte inconnu).",
            )
        })
    }

    /// Manifeste de GRAM, relu à chaque fois (GRAM peut l'avoir changé) ; vide sans dossier
    /// partagé, sans fichier ou s'il est invalide.
    pub fn manifest(&self) -> SharedManifest {
        self.config
            .shared_dir
            .as_deref()
            .map(read_manifest)
            .unwrap_or_default()
    }

    pub fn shows(&self) -> &ShowCache {
        &self.shows
    }

    pub async fn ollama_installed(&self) -> bool {
        let detect = Arc::clone(&self.detect_ollama);
        tauri::async_runtime::spawn_blocking(move || detect())
            .await
            .unwrap_or(false)
    }
}
