//! Ce que GRAM et GRAM Harness partagent, sans dépendre l'un de l'autre :
//!
//! - [`dirs`] : le dossier partagé `fr.gram.shared/`, dans le dossier des données locales ;
//! - [`manifest`] : `models.json`, écrit par GRAM seul, qui enrichit la liste des modèles
//!   d'Ollama (nom lisible, usage, vitesses) et ses règles (remplacement, retrait) ;
//! - [`presence`] : `harness.json`, écrit par GRAM Harness à chaque démarrage, qui dit à GRAM
//!   qu'il est installé ;
//! - [`deeplink`] : `gram-harness://open?model=…`, construit et analysé strictement ;
//! - [`json`] : lecture tolérante et écriture atomique des fichiers JSON (aussi ceux propres à
//!   chaque application).
//!
//! Ollama fait foi : le manifeste n'est qu'un complément, et chaque application fonctionne
//! sans lui. Les types exposés aux interfaces dérivent [`ts_rs::TS`] (en camelCase).

pub mod deeplink;
pub mod dirs;
pub mod json;
pub mod manifest;
pub mod presence;

pub use deeplink::{DeepLink, DeepLinkError};
pub use dirs::{SHARED_DIR_NAME, local_data_dir, shared_dir};
pub use manifest::{
    MANIFEST_FILE, MANIFEST_VERSION, SharedManifest, SharedModel, read_manifest, update_manifest,
};
pub use presence::{HarnessPresence, PRESENCE_FILE, read_presence, write_presence};

/// Même nom de modèle pour Ollama, qui ne tient pas compte de la casse.
pub fn same_model(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}
