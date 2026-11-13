//! Erreur des commandes : renvoyée à l'interface sous forme de message en français.

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Ollama(#[from] gram_ollama::OllamaError),
}

impl CommandError {
    pub fn msg(text: impl Into<String>) -> Self {
        Self::Message(text.into())
    }
}

/// L'interface reçoit le message seul (`invoke` rejette sa promesse avec cette chaîne).
impl Serialize for CommandError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type CmdResult<T> = Result<T, CommandError>;
