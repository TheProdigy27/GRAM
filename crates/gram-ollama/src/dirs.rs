//! Dossier des modèles d'Ollama, pour vérifier la place sur le disque avant un téléchargement
//! (`gram_core::harness::disk_check`).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Dossier du service qu'installe le script Linux d'Ollama (utilisateur `ollama`).
const LINUX_SERVICE_DIR: &str = "/usr/share/ollama/.ollama/models";

/// Dossier des modèles : `OLLAMA_MODELS`, sinon `~/.ollama/models`. Sous Linux, Ollama tourne
/// souvent en service avec son propre dossier, pris quand celui de l'utilisateur n'existe pas.
///
/// `OLLAMA_MODELS` n'est vu que s'il est défini pour l'utilisateur (sous Windows, c'est là que
/// la documentation d'Ollama le place), pas seulement pour un service.
pub fn models_dir() -> Option<PathBuf> {
    resolve(
        std::env::var_os("OLLAMA_MODELS"),
        std::env::home_dir(),
        cfg!(target_os = "linux").then(|| Path::new(LINUX_SERVICE_DIR)),
        Path::is_dir,
    )
}

fn resolve(
    env: Option<OsString>,
    home: Option<PathBuf>,
    service: Option<&Path>,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if let Some(dir) = env.filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let user = home.map(|h| h.join(".ollama").join("models"));
    match (user, service) {
        (Some(user), _) if exists(&user) => Some(user),
        (_, Some(service)) if exists(service) => Some(service.to_path_buf()),
        (user, _) => user,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dossier_des_modeles() {
        let home = || Some(PathBuf::from("/home/gram"));
        let user = Path::new("/home/gram/.ollama/models");
        let service = Some(Path::new(LINUX_SERVICE_DIR));
        let none = |_: &Path| false;
        let all = |_: &Path| true;

        // `OLLAMA_MODELS` d'abord, s'il n'est pas vide.
        let env = Some(OsString::from("D:\\ollama"));
        assert_eq!(
            resolve(env, home(), service, all),
            Some(PathBuf::from("D:\\ollama"))
        );
        assert_eq!(
            resolve(Some(OsString::new()), home(), None, none).as_deref(),
            Some(user)
        );
        // Dossier de l'utilisateur, même s'il n'existe pas encore.
        assert_eq!(resolve(None, home(), service, all).as_deref(), Some(user));
        assert_eq!(resolve(None, home(), service, none).as_deref(), Some(user));
        // Service Linux, quand l'utilisateur n'a pas de dossier.
        let only_service = |p: &Path| p == Path::new(LINUX_SERVICE_DIR);
        assert_eq!(
            resolve(None, home(), service, only_service).as_deref(),
            service
        );
        assert_eq!(
            resolve(None, None, service, only_service).as_deref(),
            service
        );
        assert_eq!(resolve(None, None, None, all), None);
    }
}
