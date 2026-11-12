//! Fichiers JSON de l'application, dans son dossier (`%LOCALAPPDATA%\fr.gram.desktop\` sous
//! Windows, `~/Library/Application Support/fr.gram.desktop/` sous macOS) :
//!
//! - `settings.json` : paramètres utilisateur (§12), sans le token Hugging Face ;
//! - `corrections.json` : corrections manuelles du matériel (§6.6), sans la simulation ;
//! - `measurements.json` : mesures réelles faites avec Ollama (phase 7).
//!
//! Lecture tolérante et écriture atomique : `gram_link::json`, partagé avec GRAM Harness et
//! `gram-cli`. Le manifeste de GRAM Harness est dans le dossier partagé (`gram_link`).

pub use gram_link::json::{load, save};

pub const SETTINGS_FILE: &str = "settings.json";
pub const CORRECTIONS_FILE: &str = "corrections.json";
pub const MEASUREMENTS_FILE: &str = "measurements.json";

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
