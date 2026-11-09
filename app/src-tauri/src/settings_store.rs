//! `%LOCALAPPDATA%\GRAM\settings.json` : paramètres utilisateur (sans le token).

use gram_core::Settings;
use std::io;
use std::path::Path;

/// Paramètres enregistrés, complétés par les valeurs par défaut et ramenés dans leurs bornes.
/// Fichier absent ou illisible : valeurs par défaut (le fichier illisible n'est pas écrasé
/// avant le prochain enregistrement).
pub fn load(path: &Path) -> Settings {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Settings::default(),
        Err(e) => {
            tracing::warn!(path = %path.display(), "paramètres illisibles : {e}");
            return Settings::default();
        }
    };
    match serde_json::from_slice::<Settings>(&bytes) {
        Ok(s) => s.sanitized(),
        Err(e) => {
            tracing::warn!(path = %path.display(), "paramètres invalides, valeurs par défaut : {e}");
            Settings::default()
        }
    }
}

/// Écriture atomique (fichier temporaire, puis renommage).
pub fn save(path: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(settings).map_err(io::Error::other)?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}
