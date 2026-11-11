//! Fichiers JSON de l'application, dans son dossier (`%LOCALAPPDATA%\fr.gram.desktop\` sous
//! Windows, `~/Library/Application Support/fr.gram.desktop/` sous macOS) :
//!
//! - `settings.json` : paramètres utilisateur (§12), sans le token Hugging Face ;
//! - `corrections.json` : corrections manuelles du matériel (§6.6), sans la simulation ;
//! - `measurements.json` : mesures réelles faites avec Ollama (phase 7).

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::io;
use std::path::Path;

pub const SETTINGS_FILE: &str = "settings.json";
pub const CORRECTIONS_FILE: &str = "corrections.json";
pub const MEASUREMENTS_FILE: &str = "measurements.json";

/// Contenu enregistré ; fichier absent, illisible ou invalide : valeur par défaut. Un fichier
/// invalide n'est pas écrasé avant le prochain enregistrement. `what` nomme le fichier dans
/// le journal.
pub fn load<T: DeserializeOwned + Default>(path: &Path, what: &str) -> T {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return T::default(),
        Err(e) => {
            tracing::warn!(path = %path.display(), "{what} illisibles : {e}");
            return T::default();
        }
    };
    serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        tracing::warn!(path = %path.display(), "{what} invalides, valeurs par défaut : {e}");
        T::default()
    })
}

/// Écriture atomique (fichier temporaire, puis renommage).
pub fn save<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gram_core::{Overrides, Settings};

    #[test]
    fn absent_invalide_puis_enregistre() {
        let dir = std::env::temp_dir().join(format!("gram-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join(CORRECTIONS_FILE);
        assert_eq!(
            load::<Overrides>(&path, "corrections"),
            Overrides::default()
        );

        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, b"{ pas du json").unwrap();
        assert_eq!(
            load::<Overrides>(&path, "corrections"),
            Overrides::default()
        );
        assert!(path.exists(), "un fichier invalide n'est pas supprimé");

        let o = Overrides {
            ram_channels: Some(4),
            ..Overrides::default()
        };
        save(&path, &o).unwrap();
        assert_eq!(load::<Overrides>(&path, "corrections"), o);

        let settings = dir.join(SETTINGS_FILE);
        save(&settings, &Settings::default()).unwrap();
        assert_eq!(
            load::<Settings>(&settings, "paramètres"),
            Settings::default()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
