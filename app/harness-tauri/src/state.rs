//! État de GRAM Harness (`tauri::State<HarnessState>`) : client de l'API d'Ollama, dossier
//! partagé avec GRAM, capacités des modèles déjà lues, conversations et réponses en cours,
//! modèle demandé par un lien profond. La détection d'Ollama est injectable pour les tests.

use crate::chat::Generations;
use crate::conversations::ConversationStore;
use crate::error::{CmdResult, CommandError};
use crate::link::OpenRequests;
use crate::models::ShowCache;
use gram_link::{SharedManifest, read_manifest};
use gram_ollama::{Capability, OllamaClient};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Ollama est-il installé ? Bloquant (≈ 100 ms) : appelé seulement quand son API ne répond pas.
pub type DetectOllama = Arc<dyn Fn() -> bool + Send + Sync>;

/// Dossier de données de GRAM Harness, dans le dossier des données locales (son identifiant).
pub const APP_DIR_NAME: &str = "fr.gram.harness";
/// Dossier des conversations, dans celui de GRAM Harness.
pub const CONVERSATIONS_DIR_NAME: &str = "conversations";

pub struct Config {
    /// API locale d'Ollama (`OLLAMA_HOST`) ; `None` si l'adresse n'est pas utilisable.
    pub ollama: Option<OllamaClient>,
    /// Dossier partagé avec GRAM (`fr.gram.shared`) ; `None` s'il est introuvable : la liste
    /// garde alors les noms techniques.
    pub shared_dir: Option<PathBuf>,
    /// Dossier des conversations ; `None` s'il est introuvable : rien ne peut être enregistré.
    pub conversations_dir: Option<PathBuf>,
}

pub struct HarnessState {
    config: Config,
    detect_ollama: DetectOllama,
    shows: ShowCache,
    conversations: Option<ConversationStore>,
    generations: Generations,
    open_requests: OpenRequests,
}

impl HarnessState {
    pub fn new(config: Config, detect_ollama: DetectOllama) -> Self {
        Self {
            conversations: config.conversations_dir.clone().map(ConversationStore::new),
            config,
            detect_ollama,
            shows: ShowCache::default(),
            generations: Generations::default(),
            open_requests: OpenRequests::default(),
        }
    }

    /// Variables d'environnement (`OLLAMA_HOST`, dossier des données locales), vraie détection.
    pub fn production() -> Self {
        Self::new(
            Config {
                ollama: OllamaClient::from_env(),
                shared_dir: gram_link::shared_dir(),
                conversations_dir: gram_link::local_data_dir()
                    .map(|d| d.join(APP_DIR_NAME).join(CONVERSATIONS_DIR_NAME)),
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

    /// Dossier partagé avec GRAM, s'il est connu.
    pub fn shared_dir(&self) -> Option<&Path> {
        self.config.shared_dir.as_deref()
    }

    pub fn shows(&self) -> &ShowCache {
        &self.shows
    }

    /// Le modèle accepte-t-il des images ? D'après ses capacités (`/api/show`, déjà lues pour
    /// la liste ou lues maintenant) ; `None` si elles sont inconnues (Ollama ancien,
    /// `/api/show` en échec) : les images sont alors envoyées.
    pub async fn model_vision(&self, client: &OllamaClient, model: &str) -> Option<bool> {
        let show = match self.shows.find(model) {
            Some(show) => show,
            None => client
                .show(model)
                .await
                .inspect_err(|e| tracing::debug!(model, "/api/show : {e}"))
                .ok()?,
        };
        (!show.capabilities.is_empty()).then(|| show.has(&Capability::Vision))
    }

    /// Conversations enregistrées, ou l'erreur à afficher si leur dossier est introuvable.
    pub fn conversations(&self) -> CmdResult<&ConversationStore> {
        self.conversations.as_ref().ok_or_else(|| {
            CommandError::msg(
                "Dossier de données introuvable : les conversations ne peuvent pas être \
                 enregistrées (variable LOCALAPPDATA, HOME ou XDG_DATA_HOME absente)",
            )
        })
    }

    pub fn generations(&self) -> &Generations {
        &self.generations
    }

    /// Modèle demandé par le dernier lien profond, en attendant que l'interface le prenne.
    pub fn open_requests(&self) -> &OpenRequests {
        &self.open_requests
    }

    pub async fn ollama_installed(&self) -> bool {
        let detect = Arc::clone(&self.detect_ollama);
        tauri::async_runtime::spawn_blocking(move || detect())
            .await
            .unwrap_or(false)
    }
}
