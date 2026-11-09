//! Token Hugging Face, stocké dans le Gestionnaire d'identifiants Windows (`keyring`).
//!
//! Il ne quitte jamais le côté Rust : les commandes ne renvoient que « configuré ou non », et il
//! n'est écrit ni dans un fichier, ni dans un journal, ni dans un message d'erreur.

/// Où le token est conservé (le trousseau du système, ou la mémoire dans les tests).
pub trait TokenStore: Send + Sync {
    fn get(&self) -> Result<Option<String>, String>;
    fn set(&self, token: &str) -> Result<(), String>;
    fn clear(&self) -> Result<(), String>;
}

/// Gestionnaire d'identifiants Windows : entrée « GRAM » / « huggingface-token ».
pub struct KeyringStore;

const SERVICE: &str = "GRAM";
const USER: &str = "huggingface-token";

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, USER).map_err(|e| format!("trousseau indisponible : {e}"))
}

impl TokenStore for KeyringStore {
    fn get(&self) -> Result<Option<String>, String> {
        match entry()?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("lecture du token impossible : {e}")),
        }
    }

    fn set(&self, token: &str) -> Result<(), String> {
        entry()?
            .set_password(token)
            .map_err(|e| format!("enregistrement du token impossible : {e}"))
    }

    fn clear(&self) -> Result<(), String> {
        match entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("suppression du token impossible : {e}")),
        }
    }
}

/// Stockage en mémoire, pour les tests (ne touche pas au trousseau de la machine).
#[cfg(test)]
#[derive(Default)]
pub struct MemoryStore(std::sync::Mutex<Option<String>>);

#[cfg(test)]
impl TokenStore for MemoryStore {
    fn get(&self) -> Result<Option<String>, String> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn set(&self, token: &str) -> Result<(), String> {
        *self.0.lock().unwrap() = Some(token.to_owned());
        Ok(())
    }

    fn clear(&self) -> Result<(), String> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}
