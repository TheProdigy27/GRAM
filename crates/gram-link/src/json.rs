//! Fichiers JSON : lecture tolérante, écriture atomique. Servent au dossier partagé comme aux
//! fichiers propres à chaque application (paramètres, corrections, mesures de GRAM ;
//! conversations de GRAM Harness).

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::io;
use std::path::{Path, PathBuf};

/// Contenu enregistré ; fichier absent, illisible ou invalide : valeur par défaut. Un fichier
/// invalide n'est pas écrasé avant le prochain enregistrement. `what` nomme le fichier dans
/// le journal.
pub fn load<T: DeserializeOwned + Default>(path: &Path, what: &str) -> T {
    match read(path) {
        Ok(Some(bytes)) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            tracing::warn!(path = %path.display(), "{what} invalides, valeurs par défaut : {e}");
            T::default()
        }),
        Ok(None) => T::default(),
        Err(e) => {
            tracing::warn!(path = %path.display(), "{what} illisibles : {e}");
            T::default()
        }
    }
}

/// Contenu brut ; `None` si le fichier n'existe pas.
pub fn read(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Écriture atomique : fichier temporaire propre au processus, puis renommage. Un lecteur voit
/// l'ancien contenu ou le nouveau, jamais un fichier à moitié écrit, même quand deux processus
/// (GRAM et `gram-cli`) écrivent en même temps.
pub fn save<T: Serialize + ?Sized>(path: &Path, value: &T) -> io::Result<()> {
    let json = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    save_bytes(path, &json)
}

/// Comme [`save`], pour un contenu quelconque (image jointe à une conversation…).
pub fn save_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = temp_path(path);
    let written = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// `models.json` → `models.json.1234.tmp`.
fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    path.with_file_name(name)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde::Deserialize;

    /// Dossier temporaire supprimé à la fin du test.
    pub struct TempDir(pub PathBuf);

    impl TempDir {
        pub fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("gram-link-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Data {
        n: u32,
    }

    #[test]
    fn absent_invalide_puis_enregistre() {
        let dir = TempDir::new("json");
        let path = dir.0.join("sous-dossier").join("data.json");
        assert_eq!(load::<Data>(&path, "données"), Data::default());
        assert_eq!(read(&path).unwrap(), None);

        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{ pas du json").unwrap();
        assert_eq!(load::<Data>(&path, "données"), Data::default());
        assert!(path.exists(), "un fichier invalide n'est pas supprimé");

        save(&path, &Data { n: 3 }).unwrap();
        assert_eq!(load::<Data>(&path, "données"), Data { n: 3 });
        // Le fichier temporaire a été renommé.
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["data.json"]);
        assert_eq!(
            temp_path(&path).file_name().unwrap().to_string_lossy(),
            format!("data.json.{}.tmp", std::process::id())
        );
    }

    #[test]
    fn octets_bruts() {
        let dir = TempDir::new("octets");
        let path = dir.0.join("conversation").join("image.png");
        save_bytes(&path, b"\x89PNG\r\n").unwrap();
        save_bytes(&path, b"\x89PNG\r\n\x1a\n").unwrap();
        assert_eq!(read(&path).unwrap().unwrap(), b"\x89PNG\r\n\x1a\n");
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["image.png"]);
    }
}
