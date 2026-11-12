//! Dossier partagé par GRAM, GRAM Harness et `gram-cli`.

use std::ffi::OsString;
use std::path::PathBuf;

/// Nom du dossier partagé, dans le dossier des données locales.
pub const SHARED_DIR_NAME: &str = "fr.gram.shared";

/// `fr.gram.shared/` dans le dossier des données locales, d'après les variables
/// d'environnement : `%LOCALAPPDATA%` (Windows), `~/Library/Application Support` (macOS),
/// `$XDG_DATA_HOME` ou `~/.local/share` (Linux). `None` si la variable manque.
///
/// Sans API propre à une application : la même fonction sert à GRAM, à GRAM Harness et à
/// `gram-cli`, qui trouvent donc le même dossier.
pub fn shared_dir() -> Option<PathBuf> {
    data_local_dir(std::env::consts::OS, |key| std::env::var_os(key))
        .map(|d| d.join(SHARED_DIR_NAME))
}

fn data_local_dir(os: &str, var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let get = |key: &str| var(key).filter(|v| !v.is_empty()).map(PathBuf::from);
    match os {
        "windows" => get("LOCALAPPDATA"),
        "macos" => get("HOME").map(|h| h.join("Library").join("Application Support")),
        // La spécification XDG ignore un chemin relatif.
        _ => get("XDG_DATA_HOME")
            .filter(|p| p.to_string_lossy().starts_with('/'))
            .or_else(|| get("HOME").map(|h| h.join(".local").join("share"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |key| {
            vars.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn dossier_des_donnees_locales() {
        let windows = [
            ("LOCALAPPDATA", "C:\\Users\\gram\\AppData\\Local"),
            ("HOME", "C:\\Users\\gram"),
        ];
        assert_eq!(
            data_local_dir("windows", env(&windows)),
            Some(PathBuf::from("C:\\Users\\gram\\AppData\\Local"))
        );
        assert_eq!(data_local_dir("windows", env(&[("HOME", "C:\\x")])), None);
        assert_eq!(
            data_local_dir("windows", env(&[("LOCALAPPDATA", "")])),
            None
        );

        let home = [("HOME", "/Users/gram")];
        assert_eq!(
            data_local_dir("macos", env(&home)),
            Some(
                PathBuf::from("/Users/gram")
                    .join("Library")
                    .join("Application Support")
            )
        );

        let linux = [("HOME", "/home/gram"), ("XDG_DATA_HOME", "/data/gram")];
        assert_eq!(
            data_local_dir("linux", env(&linux)),
            Some(PathBuf::from("/data/gram"))
        );
        let relative = [("HOME", "/home/gram"), ("XDG_DATA_HOME", "data")];
        let fallback = Some(PathBuf::from("/home/gram").join(".local").join("share"));
        assert_eq!(data_local_dir("linux", env(&relative)), fallback);
        assert_eq!(
            data_local_dir("freebsd", env(&[("HOME", "/home/gram")])),
            fallback
        );
        assert_eq!(data_local_dir("linux", env(&[])), None);
    }

    #[test]
    fn dossier_partage() {
        if let Some(dir) = shared_dir() {
            assert!(dir.ends_with(SHARED_DIR_NAME));
        }
    }
}
