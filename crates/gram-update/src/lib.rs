//! Mises à jour de GRAM et de GRAM Harness depuis les releases GitHub (plugin updater de Tauri).
//!
//! Chaque release GitHub (`scripts/release.mjs`, lancé sur le Mac et sur le PC Windows) contient
//! les installeurs des deux applications et un manifeste par application (`latest.json` pour
//! GRAM, `latest-harness.json` pour GRAM Harness) : version, notes, et pour chaque plateforme
//! l'adresse du paquet et sa signature. [`check`] lit le manifeste de la dernière release
//! (adresse et clé publique dans le `tauri.conf.json` de l'application) ; [`install`] télécharge
//! le paquet, vérifie sa signature avec la clé publique, l'installe et relance l'application :
//!
//! - Windows : l'installeur (NSIS, ou MSI si c'est lui qui a installé l'application) est lancé
//!   en mode passif, l'application se ferme et l'installeur la relance ;
//! - macOS : l'application (`.app`) est remplacée sur place, puis elle redémarre.
//!
//! Chaque application expose ces fonctions par ses commandes `check_update` et `install_update`
//! (mêmes noms, mêmes types, partagés par les deux interfaces) : la WebView n'a aucune permission
//! sur le plugin.

use serde::Serialize;
use std::sync::Mutex;
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_updater::{Error as UpdaterError, Update};
use ts_rs::TS;

pub use tauri_plugin_updater::UpdaterExt;

/// Téléchargement d'une mise à jour ([`install`]) : `UpdateProgress { downloaded, total }`.
pub const UPDATE_PROGRESS: &str = "update-progress";

/// Plugin updater, à enregistrer par chaque application (`tauri::Builder::plugin`).
pub fn init<R: Runtime>() -> TauriPlugin<R, tauri_plugin_updater::Config> {
    tauri_plugin_updater::Builder::new().build()
}

/// Échec d'une vérification ou d'une installation, en français (le détail technique, en
/// anglais, est journalisé).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct UpdateError(String);

impl UpdateError {
    fn msg(text: impl Into<String>) -> Self {
        Self(text.into())
    }
}

/// Résultat de `check_update`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateCheck {
    /// Version en cours d'exécution.
    pub current_version: String,
    /// Mise à jour proposée par la dernière release ; `None` si l'application est à jour.
    pub update: Option<UpdateInfo>,
}

/// Mise à jour proposée par la dernière release.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateInfo {
    /// Version proposée.
    pub version: String,
    /// Notes de la release (Markdown), si le manifeste en contient.
    pub notes: Option<String>,
    /// Date de publication (RFC 3339), si le manifeste l'indique.
    pub date: Option<String>,
}

impl From<&Update> for UpdateInfo {
    fn from(u: &Update) -> Self {
        Self {
            version: u.version.clone(),
            notes: u.body.clone().filter(|n| !n.trim().is_empty()),
            // Telle qu'écrite dans le manifeste (le plugin la lit dans un type de `time`).
            date: u.raw_json["pub_date"].as_str().map(str::to_owned),
        }
    }
}

/// Progression du téléchargement (événement `update-progress`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateProgress {
    /// Octets reçus.
    pub downloaded: u64,
    /// Taille du paquet, si le serveur l'annonce.
    pub total: Option<u64>,
}

/// Dernière mise à jour trouvée par [`check`], reprise par [`install`] (état Tauri géré par
/// chaque application : `tauri::Builder::manage`).
pub struct UpdateState {
    pending: Mutex<Option<Update>>,
    /// Une seule installation à la fois.
    installing: tokio::sync::Mutex<()>,
    /// `false` en développement : l'exécutable de `target/debug` n'est pas une application
    /// installée, le plugin remplacerait le dossier qui le contient.
    can_install: bool,
}

impl UpdateState {
    pub fn new(can_install: bool) -> Self {
        Self {
            pending: Mutex::new(None),
            installing: tokio::sync::Mutex::new(()),
            can_install,
        }
    }

    /// Installation permise dans les builds de production seulement.
    pub fn production() -> Self {
        Self::new(!cfg!(debug_assertions))
    }
}

/// Lit le manifeste de la dernière release.
pub async fn check<R: Runtime>(
    app: &AppHandle<R>,
    state: &UpdateState,
) -> Result<UpdateCheck, UpdateError> {
    let found = latest(app)
        .await
        .map_err(|e| failure("vérification impossible", &e))?;
    let info = app.package_info();
    let current_version = info.version.to_string();
    let update = found.as_ref().map(UpdateInfo::from);
    match &update {
        Some(u) => tracing::info!(
            current = current_version,
            version = u.version,
            "mise à jour disponible"
        ),
        None => tracing::info!(current = current_version, "{} est à jour", info.name),
    }
    *state.pending.lock().expect("verrou de la mise à jour") = found;
    Ok(UpdateCheck {
        current_version,
        update,
    })
}

/// Télécharge la mise à jour trouvée par [`check`] (ou cherchée à nouveau), vérifie sa
/// signature, l'installe et relance l'application. Ne revient qu'en cas d'erreur.
pub async fn install<R: Runtime>(
    app: &AppHandle<R>,
    state: &UpdateState,
) -> Result<(), UpdateError> {
    if !state.can_install {
        return Err(UpdateError::msg(format!(
            "installation impossible en mode développement : lancer une version installée de {}",
            app.package_info().name
        )));
    }
    let Ok(_installing) = state.installing.try_lock() else {
        return Err(UpdateError::msg(
            "une mise à jour est déjà en cours d'installation",
        ));
    };

    let pending = state
        .pending
        .lock()
        .expect("verrou de la mise à jour")
        .clone();
    let update = match pending {
        Some(u) => u,
        None => latest(app)
            .await
            .map_err(|e| failure("vérification impossible", &e))?
            .ok_or_else(|| UpdateError::msg("aucune mise à jour disponible"))?,
    };

    tracing::info!(version = update.version, url = %update.download_url, "téléchargement de la mise à jour");
    let mut progress = Progress::new(app);
    // `download` vérifie la signature : un paquet modifié est refusé avant toute installation.
    let bytes = update
        .download(|len, total| progress.chunk(len, total), || {})
        .await
        .map_err(|e| failure("téléchargement impossible", &e))?;
    progress.finish();

    tracing::info!(bytes = bytes.len(), "paquet vérifié, installation");
    let installer = update.clone();
    // Windows : `install` lance l'installeur puis ferme l'application, qu'il relancera.
    tauri::async_runtime::spawn_blocking(move || installer.install(bytes))
        .await
        .map_err(|e| {
            UpdateError::msg(format!("installation impossible : tâche interrompue ({e})"))
        })?
        .map_err(|e| failure("installation impossible", &e))?;

    // macOS : la nouvelle version est en place, on la lance.
    tracing::info!("mise à jour installée, redémarrage");
    app.restart()
}

async fn latest<R: Runtime>(app: &AppHandle<R>) -> Result<Option<Update>, UpdaterError> {
    app.updater()?.check().await
}

/// Message en français pour l'interface ; le détail technique (en anglais) est journalisé.
fn failure(context: &str, e: &UpdaterError) -> UpdateError {
    tracing::warn!("{context} : {e}");
    let reason = match e {
        UpdaterError::ReleaseNotFound => "aucune release trouvée sur GitHub".to_owned(),
        UpdaterError::Reqwest(_) | UpdaterError::Network(_) => {
            format!("GitHub injoignable (connexion ?) : {e}")
        }
        UpdaterError::TargetNotFound(_) | UpdaterError::TargetsNotFound(_) => {
            "la dernière release ne contient pas de paquet pour ce système".to_owned()
        }
        UpdaterError::Minisign(_) | UpdaterError::Base64(_) | UpdaterError::SignatureUtf8(_) => {
            "signature invalide, paquet refusé".to_owned()
        }
        _ => e.to_string(),
    };
    UpdateError::msg(format!("{context} : {reason}"))
}

/// Progression envoyée à l'interface : au plus une centaine d'événements par téléchargement.
struct Progress<'a, R: Runtime> {
    app: &'a AppHandle<R>,
    downloaded: u64,
    total: Option<u64>,
    emitted: u64,
}

impl<'a, R: Runtime> Progress<'a, R> {
    /// Taille d'un pas quand la taille totale est inconnue.
    const STEP_UNKNOWN: u64 = 256 * 1024;

    fn new(app: &'a AppHandle<R>) -> Self {
        let progress = Self {
            app,
            downloaded: 0,
            total: None,
            emitted: 0,
        };
        progress.emit();
        progress
    }

    fn chunk(&mut self, len: usize, total: Option<u64>) {
        self.downloaded += len as u64;
        self.total = total;
        let step = total.map_or(Self::STEP_UNKNOWN, |t| (t / 100).max(1));
        if self.downloaded - self.emitted >= step {
            self.emitted = self.downloaded;
            self.emit();
        }
    }

    fn finish(&mut self) {
        if self.emitted != self.downloaded {
            self.emitted = self.downloaded;
            self.emit();
        }
    }

    /// Un échec est journalisé, jamais bloquant.
    fn emit(&self) {
        let payload = UpdateProgress {
            downloaded: self.downloaded,
            total: self.total,
        };
        if let Err(e) = self.app.emit(UPDATE_PROGRESS, payload) {
            tracing::warn!(event = UPDATE_PROGRESS, "événement non émis : {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    /// Le plugin active `reqwest/rustls-no-provider` : un client reqwest (celui d'Ollama dans
    /// GRAM Harness) se crée quand même, sans fournisseur installé à la main.
    #[test]
    fn client_reqwest_avec_fournisseur_tls() {
        reqwest::Client::builder().build().unwrap();
    }
}
