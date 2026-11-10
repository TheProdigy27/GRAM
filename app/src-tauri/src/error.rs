//! Erreur des commandes Tauri : renvoyée au frontend sous forme de message en français.
//!
//! Aucun message ne contient le token Hugging Face (`gram-hub` le garantit pour ses erreurs,
//! et le trousseau Windows ne renvoie jamais le secret dans les siennes).

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Hub(#[from] gram_hub::HubError),
    #[error(transparent)]
    Ollama(#[from] gram_ollama::OllamaError),
    #[error("tâche interrompue : {0}")]
    Task(#[from] tauri::Error),
}

impl CommandError {
    pub fn msg(text: impl Into<String>) -> Self {
        Self::Message(text.into())
    }
}

/// Le frontend reçoit le message seul (`invoke` rejette sa promesse avec cette chaîne).
impl Serialize for CommandError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type CmdResult<T> = Result<T, CommandError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialise_en_message() {
        let e = CommandError::from(gram_hub::HubError::NoCatalog);
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            "aucun catalogue disponible : ni connexion, ni cache, ni catalogue embarqué"
        );
        assert_eq!(serde_json::to_value(CommandError::msg("x")).unwrap(), "x");
    }
}
