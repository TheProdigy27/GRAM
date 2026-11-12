//! Erreurs de l'API d'Ollama, en français (affichées telles quelles dans l'interface).

#[derive(Debug, thiserror::Error)]
pub enum OllamaError {
    #[error("Ollama ne répond pas sur {0} : le lancer, puis réessayer")]
    Unreachable(String),
    #[error("Ollama n'a pas répondu à temps ({0})")]
    Timeout(&'static str),
    /// Erreur renvoyée par Ollama (`{"error": "…"}`), par exemple un modèle introuvable ou
    /// impossible à charger.
    #[error("Ollama a refusé la requête (HTTP {status}) : {message}")]
    Api { status: u16, message: String },
    /// Erreur signalée au milieu d'un flux (`{"error": "…"}` après le statut HTTP 200), par
    /// exemple un manifeste introuvable ou un dépôt protégé au début d'un téléchargement.
    #[error("Ollama a interrompu l'opération : {0}")]
    Interrupted(String),
    #[error("réponse inattendue d'Ollama : {0}")]
    Invalid(String),
    #[error("requête vers Ollama impossible : {0}")]
    Client(String),
}

impl OllamaError {
    /// Échec d'un téléchargement depuis Hugging Face que l'utilisateur peut corriger (dépôt
    /// protégé, quantification absente, modèle découpé), signalé au milieu du flux ou, sans
    /// flux, par une erreur HTTP.
    pub fn pull_failure(&self) -> Option<gram_core::harness::PullFailure> {
        match self {
            Self::Interrupted(message) | Self::Api { message, .. } => {
                crate::api::pull_failure(message)
            }
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, OllamaError>;
