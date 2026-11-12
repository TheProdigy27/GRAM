//! `harness.json` : GRAM Harness l'écrit à chaque démarrage pour dire à GRAM qu'il est
//! installé. Nécessaire parce que sous Windows, ouvrir un schéma inconnu
//! (`gram-harness://…`) affiche « Choisir une application » au lieu d'échouer.

use crate::json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// Nom du fichier, dans le dossier partagé.
pub const PRESENCE_FILE: &str = "harness.json";
/// Version du format.
pub const PRESENCE_VERSION: u32 = 1;

/// Contenu de `harness.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HarnessPresence {
    pub version: u32,
    /// Version de GRAM Harness.
    pub app_version: String,
    /// Exécutable de GRAM Harness : s'il a disparu, l'application a été désinstallée.
    pub exe_path: PathBuf,
    pub updated_at: DateTime<Utc>,
}

impl HarnessPresence {
    pub fn new(app_version: impl Into<String>, exe_path: PathBuf, now: DateTime<Utc>) -> Self {
        Self {
            version: PRESENCE_VERSION,
            app_version: app_version.into(),
            exe_path,
            updated_at: now,
        }
    }
}

/// GRAM Harness, s'il est installé : le fichier est lisible et son exécutable existe encore.
pub fn read_presence(dir: &Path) -> Option<HarnessPresence> {
    let path = dir.join(PRESENCE_FILE);
    let bytes = match json::read(&path) {
        Ok(b) => b?,
        Err(e) => {
            tracing::warn!(path = %path.display(), "{PRESENCE_FILE} illisible : {e}");
            return None;
        }
    };
    let presence: HarnessPresence = serde_json::from_slice(&bytes)
        .inspect_err(|e| tracing::warn!(path = %path.display(), "{PRESENCE_FILE} invalide : {e}"))
        .ok()?;
    if presence.version > PRESENCE_VERSION {
        tracing::info!(
            version = presence.version,
            "{PRESENCE_FILE} d'une version plus récente de GRAM Harness"
        );
    }
    if !presence.exe_path.is_file() {
        tracing::info!(
            exe = %presence.exe_path.display(),
            "GRAM Harness désinstallé : son exécutable a disparu"
        );
        return None;
    }
    Some(presence)
}

/// Écrit la présence de GRAM Harness (à chaque démarrage).
pub fn write_presence(dir: &Path, presence: &HarnessPresence) -> io::Result<()> {
    json::save(&dir.join(PRESENCE_FILE), presence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::tests::TempDir;

    #[test]
    fn presence() {
        let dir = TempDir::new("presence");
        assert_eq!(read_presence(&dir.0), None, "fichier absent");

        let exe = dir.0.join("gram-harness.exe");
        let p = HarnessPresence::new("0.2.0", exe.clone(), Utc::now());
        write_presence(&dir.0, &p).unwrap();
        assert_eq!(read_presence(&dir.0), None, "exécutable disparu");

        std::fs::write(&exe, b"").unwrap();
        assert_eq!(read_presence(&dir.0), Some(p.clone()));
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.0.join(PRESENCE_FILE)).unwrap()).unwrap();
        assert_eq!(json["appVersion"], "0.2.0");
        assert!(
            json["exePath"]
                .as_str()
                .unwrap()
                .ends_with("gram-harness.exe")
        );

        // Version future lisible : gardée.
        let mut future = p.clone();
        future.version = 2;
        write_presence(&dir.0, &future).unwrap();
        assert_eq!(read_presence(&dir.0).map(|p| p.version), Some(2));

        std::fs::write(dir.0.join(PRESENCE_FILE), b"[]").unwrap();
        assert_eq!(read_presence(&dir.0), None, "fichier invalide");
    }
}
