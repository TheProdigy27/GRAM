//! « Ouvrir dans GRAM Harness » : présence et lien profond.
//!
//! - **Présence** : à chaque démarrage, GRAM Harness écrit `harness.json` dans le dossier
//!   partagé (version, exécutable) ; GRAM n'ouvre le lien que s'il le trouve.
//! - **Lien profond** `gram-harness://open?model=…` : reçu au démarrage (`get_current` du
//!   plugin `deep-link`) ou pendant que l'application tourne (`on_open_url` : un deuxième
//!   lancement passe son lien à la première instance par le plugin `single-instance`).
//!
//! N'importe quelle page web peut ouvrir ce lien : il est analysé strictement
//! (`gram_link::DeepLink`), un lien refusé est ignoré et journalisé, et un lien valide ne fait
//! que choisir un modèle. Le modèle demandé est mis de côté ([`OpenRequests`]), l'interface est
//! prévenue ([`OPEN_MODEL_EVENT`]) et vient le prendre (`take_opened_model`) : un lien reçu
//! avant qu'elle n'écoute n'est pas perdu. C'est elle qui vérifie que le modèle est dans
//! Ollama.

use chrono::Utc;
use gram_link::{DeepLink, HarnessPresence, write_presence};
use std::io;
use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// Événement émis vers l'interface à chaque lien valide (sans charge utile : le modèle est
/// pris par `take_opened_model`).
pub const OPEN_MODEL_EVENT: &str = "open-model";

/// Modèle demandé par le dernier lien valide, en attendant que l'interface le prenne.
#[derive(Debug, Default)]
pub struct OpenRequests(Mutex<Option<String>>);

impl OpenRequests {
    /// Le modèle demandé, une seule fois.
    pub fn take(&self) -> Option<String> {
        self.0.lock().expect("verrou des liens").take()
    }

    fn set(&self, model: String) {
        *self.0.lock().expect("verrou des liens") = Some(model);
    }
}

/// Modèle demandé par des liens reçus ensemble : celui du dernier lien valide. Les liens
/// refusés sont journalisés et ignorés.
pub fn requested_model<'a>(links: impl IntoIterator<Item = &'a str>) -> Option<String> {
    links
        .into_iter()
        .filter_map(|link| {
            DeepLink::parse(link)
                .inspect_err(|e| tracing::warn!(link, "lien profond ignoré : {e}"))
                .ok()
        })
        .last()
        .map(|l| l.model)
}

/// Traite des liens reçus : le modèle demandé est mis de côté, l'interface prévenue et la
/// fenêtre ramenée au premier plan. Renvoie le modèle ; `None` (rien n'est fait) si aucun lien
/// n'est valide.
pub fn receive<'a, R: Runtime>(
    app: &AppHandle<R>,
    requests: &OpenRequests,
    links: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    let model = requested_model(links)?;
    tracing::info!(model, "lien profond reçu");
    requests.set(model.clone());
    if let Err(e) = app.emit(OPEN_MODEL_EVENT, ()) {
        tracing::warn!("{OPEN_MODEL_EVENT} non émis : {e}");
    }
    show_window(app);
    Some(model)
}

/// Ramène la fenêtre principale au premier plan (lien reçu, deuxième lancement).
pub fn show_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let shown = window
        .unminimize()
        .and_then(|()| window.show())
        .and_then(|()| window.set_focus());
    if let Err(e) = shown {
        tracing::debug!("fenêtre non ramenée au premier plan : {e}");
    }
}

/// Écrit la présence de GRAM Harness (cet exécutable, cette version) dans le dossier partagé.
pub fn announce(shared_dir: &Path, app_version: &str) -> io::Result<HarnessPresence> {
    let presence = HarnessPresence::new(app_version, std::env::current_exe()?, Utc::now());
    write_presence(shared_dir, &presence)?;
    Ok(presence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversations::tests::TempDir;
    use gram_link::read_presence;

    #[test]
    fn modele_demande() {
        let link = "gram-harness://open?model=gram%2Fqwen3-8b%3Aq4_k_m";
        assert_eq!(
            requested_model([link]).as_deref(),
            Some("gram/qwen3-8b:q4_k_m")
        );
        // Le dernier lien valide l'emporte ; les autres sont ignorés.
        assert_eq!(
            requested_model([
                "gram-harness://open?model=qwen3",
                link,
                "gram-harness://open?model=a%20b",
                "https://exemple.fr/?model=qwen3",
            ])
            .as_deref(),
            Some("gram/qwen3-8b:q4_k_m")
        );
        assert_eq!(requested_model([]), None);
        assert_eq!(
            requested_model([
                "gram-harness://delete?model=qwen3",
                "gram-harness://open?model=qwen3&pull=1",
                "gram-harness://open?model=..%2F..%2Fx",
                "pas un lien",
            ]),
            None
        );
    }

    #[test]
    fn presence_annoncee() {
        let dir = TempDir::new("presence");
        let written = announce(&dir.0, "0.2.0").unwrap();
        assert_eq!(written.app_version, "0.2.0");
        // L'exécutable des tests existe : GRAM considère GRAM Harness comme installé.
        assert_eq!(read_presence(&dir.0), Some(written));
    }
}
