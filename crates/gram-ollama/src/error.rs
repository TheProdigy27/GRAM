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
    #[error("réponse inattendue d'Ollama : {0}")]
    Invalid(String),
    #[error("requête vers Ollama impossible : {0}")]
    Client(String),
}

pub type Result<T> = std::result::Result<T, OllamaError>;
