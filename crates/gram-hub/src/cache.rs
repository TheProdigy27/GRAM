//! Cache local (§7.7 du plan) : `%LOCALAPPDATA%\GRAM\cache\`
//!
//! - `catalog.json` : dernier catalogue synchronisé (durée de vie : 24 h par défaut) ;
//! - `details/{org}--{nom}@{sha}.json` : fichiers d'un dépôt ([`RepoDetail`]) ;
//! - `arch/{org}--{nom}@{sha}.json` : architecture ([`ArchRecord`]).
//!
//! Les détails et architectures sont indexés par commit, donc immuables et gardés
//! indéfiniment. Écritures atomiques (fichier temporaire puis renommage).

use crate::catalog::{ArchRecord, CATALOG_FORMAT, Catalog, RepoDetail};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Cache {
    root: PathBuf,
}

/// Identifiant de dépôt Hugging Face sûr pour un nom de fichier : `org/nom`, caractères
/// `[A-Za-z0-9._-]`, sans `..`.
pub fn valid_repo_id(repo: &str) -> bool {
    let ok = |part: &str| {
        !part.is_empty()
            && !part.starts_with('.')
            && !part.contains("..")
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    };
    matches!(repo.split_once('/'), Some((o, n)) if ok(o) && ok(n))
}

fn valid_sha(sha: &str) -> bool {
    (7..=64).contains(&sha.len()) && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

impl Cache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `%LOCALAPPDATA%\GRAM\cache` (équivalent sur les autres OS).
    pub fn default_root() -> Option<PathBuf> {
        directories::BaseDirs::new().map(|d| d.data_local_dir().join("GRAM").join("cache"))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn keyed(&self, dir: &str, repo: &str, sha: &str) -> Option<PathBuf> {
        (valid_repo_id(repo) && valid_sha(sha)).then(|| {
            self.root
                .join(dir)
                .join(format!("{}@{sha}.json", repo.replace('/', "--")))
        })
    }

    pub fn load_catalog(&self) -> Option<Catalog> {
        let c: Catalog = read_json(&self.root.join("catalog.json"))?;
        (c.format == CATALOG_FORMAT).then_some(c)
    }

    pub fn save_catalog(&self, c: &Catalog) -> io::Result<()> {
        write_json(&self.root.join("catalog.json"), c)
    }

    pub fn load_detail(&self, repo: &str, sha: &str) -> Option<RepoDetail> {
        read_json(&self.keyed("details", repo, sha)?)
    }

    pub fn save_detail(&self, d: &RepoDetail) -> io::Result<()> {
        match self.keyed("details", &d.repo_id, &d.sha) {
            Some(path) => write_json(&path, d),
            None => Ok(()),
        }
    }

    pub fn load_arch(&self, repo: &str, sha: &str) -> Option<ArchRecord> {
        read_json(&self.keyed("arch", repo, sha)?)
    }

    pub fn save_arch(&self, repo: &str, sha: &str, a: &ArchRecord) -> io::Result<()> {
        match self.keyed("arch", repo, sha) {
            Some(path) => write_json(&path, a),
            None => Ok(()),
        }
    }

    /// Vide le cache (bouton des paramètres).
    pub fn clear(&self) -> io::Result<()> {
        match fs::remove_dir_all(&self.root) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = fs::read(path).ok()?;
    match serde_json::from_slice(&bytes) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(path = %path.display(), "fichier de cache illisible, ignoré : {e}");
            None
        }
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec(value).map_err(io::Error::other)?)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    #[test]
    fn identifiants() {
        assert!(valid_repo_id("bartowski/Qwen_Qwen3-8B-GGUF"));
        assert!(valid_repo_id("Qwen/Qwen2.5-7B-Instruct-GGUF"));
        assert!(!valid_repo_id("../etc"));
        assert!(!valid_repo_id("a/../b"));
        assert!(!valid_repo_id("a/b/c"));
        assert!(!valid_repo_id("a/.hidden"));
        assert!(!valid_repo_id("a\\b/c"));
        assert!(!valid_repo_id("sans-slash"));
        assert!(valid_sha("7c41481f57cb95916b40956ab2f0b139b296d974"));
        assert!(!valid_sha("main"));
        assert!(!valid_sha("../../x"));
    }

    #[test]
    fn aller_retour() {
        let dir = TempDir::new();
        let cache = Cache::new(dir.path());
        let d = RepoDetail {
            repo_id: "a/b-GGUF".into(),
            sha: "0123456789abcdef".into(),
            gated: false,
            pipeline_tag: None,
            params_total: Some(8),
            ctx_train: None,
            files: vec![],
        };
        assert_eq!(cache.load_detail("a/b-GGUF", "0123456789abcdef"), None);
        cache.save_detail(&d).unwrap();
        assert_eq!(cache.load_detail("a/b-GGUF", "0123456789abcdef"), Some(d));
        assert!(
            dir.path()
                .join("details/a--b-GGUF@0123456789abcdef.json")
                .exists()
        );
        // Un sha invalide n'écrit rien.
        let bad = RepoDetail {
            sha: "../x".into(),
            ..cache.load_detail("a/b-GGUF", "0123456789abcdef").unwrap()
        };
        cache.save_detail(&bad).unwrap();
        // Fichier corrompu : ignoré.
        fs::write(dir.path().join("catalog.json"), b"{").unwrap();
        assert!(cache.load_catalog().is_none());
        cache.clear().unwrap();
        assert!(!dir.path().exists());
        cache.clear().unwrap();
    }
}
