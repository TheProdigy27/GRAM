//! Erreurs de `gram-hub`. Les messages sont en français et ne contiennent jamais le token.

use crate::gguf::GgufError;

#[derive(Debug, thiserror::Error)]
pub enum HubError {
    /// Pas de réponse du serveur (pas de connexion, DNS, délai dépassé…).
    #[error("Hugging Face est injoignable : {0}")]
    Network(String),
    #[error("Hugging Face a répondu {status} pour {url}")]
    Http { status: u16, url: String },
    #[error("quota Hugging Face atteint, réessayer dans {reset_s} s")]
    RateLimited { reset_s: u64 },
    #[error("réponse Hugging Face illisible ({context}) : {message}")]
    Parse { context: String, message: String },
    #[error("en-tête GGUF illisible : {0}")]
    Gguf(#[from] GgufError),
    #[error("cache local : {0}")]
    Io(#[from] std::io::Error),
    #[error("aucun catalogue disponible : ni connexion, ni cache, ni catalogue embarqué")]
    NoCatalog,
}

impl HubError {
    pub(crate) fn parse(context: impl Into<String>, err: impl std::fmt::Display) -> Self {
        Self::Parse {
            context: context.into(),
            message: err.to_string(),
        }
    }

    /// Erreur qui touche toutes les requêtes (réseau coupé, serveur en panne, quota) :
    /// inutile de continuer la synchronisation en ligne.
    pub fn is_global(&self) -> bool {
        match self {
            Self::Network(_) | Self::RateLimited { .. } => true,
            Self::Http { status, .. } => *status >= 500,
            _ => false,
        }
    }
}

pub type Result<T, E = HubError> = std::result::Result<T, E>;
