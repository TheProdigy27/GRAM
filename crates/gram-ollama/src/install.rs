//! Ajout d'un modèle à Ollama pour GRAM Harness : téléchargement de la base si elle manque,
//! puis modèle dérivé qui la reprend avec le contexte calculé par GRAM. Partagé par
//! l'application et `gram-cli`.

use crate::api::{ModelShow, same_model};
use crate::client::OllamaClient;
use crate::error::{OllamaError, Result};
use gram_core::harness::{InstallPhase, PullProgress, PullTracker};
use std::future::Future;
use std::time::Instant;

/// Résultat d'un ajout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// Nom exact de la base dans `/api/tags` : Ollama garde la casse du premier `pull`.
    pub base: String,
    /// La base manquait et vient d'être téléchargée.
    pub pulled: bool,
    /// Capacités et contexte du modèle dérivé, d'après Ollama (à comparer au contexte demandé
    /// et aux capacités attendues).
    pub show: ModelShow,
}

/// Installe `base` (`hf.co/{dépôt}:{quant}`) si elle manque, puis crée ou remplace `alias`,
/// qui la reprend au contexte `ctx`.
///
/// `on_progress` reçoit chaque étape et chaque ligne du téléchargement : à l'appelant d'en
/// limiter l'affichage. Annuler, c'est terminer `cancel` : le téléchargement s'arrête (Ollama
/// garde ce qu'il a reçu, le prochain ajout reprend) et la fonction renvoie `None`.
/// L'annulation vaut jusqu'à la création du modèle dérivé, qui prend moins d'une seconde : un
/// ajout annulé ne crée ni ne modifie jamais de modèle dérivé.
pub async fn install(
    client: &OllamaClient,
    base: &str,
    alias: &str,
    ctx: u32,
    mut on_progress: impl FnMut(PullProgress),
    cancel: impl Future<Output = ()>,
) -> Result<Option<Installed>> {
    let start = Instant::now();
    let mut tracker = PullTracker::new();
    on_progress(tracker.progress(start.elapsed()));

    let fetch = async {
        if let Some(name) = installed_name(client, base).await? {
            return Ok((name, false));
        }
        tracing::info!(base, "ajout à GRAM Harness : téléchargement");
        client
            .pull(base, |event| {
                let now = start.elapsed();
                if let Some((digest, total, completed)) = event.layer() {
                    tracker.layer(digest, total, completed, now);
                } else if event.is_verifying() {
                    tracker.set_phase(InstallPhase::Verifying);
                }
                on_progress(tracker.progress(now));
            })
            .await?;
        match installed_name(client, base).await? {
            Some(name) => Ok((name, true)),
            None => Err(OllamaError::Invalid(format!(
                "{base} est téléchargé mais absent de la liste des modèles d'Ollama"
            ))),
        }
    };
    let (base_name, pulled) = tokio::select! {
        biased;
        () = cancel => {
            tracing::info!(base, "ajout à GRAM Harness annulé");
            return Ok(None);
        }
        fetched = fetch => fetched?,
    };

    tracker.set_phase(InstallPhase::Creating);
    on_progress(tracker.progress(start.elapsed()));
    client.create(alias, &base_name, ctx).await?;
    let show = client.show(alias).await?;
    if show.num_ctx != Some(ctx) {
        tracing::warn!(alias, ctx, num_ctx = ?show.num_ctx, "contexte inattendu dans le modèle dérivé");
    }
    tracing::info!(
        alias,
        base = base_name,
        pulled,
        ctx,
        "ajout à GRAM Harness terminé"
    );
    Ok(Some(Installed {
        base: base_name,
        pulled,
        show,
    }))
}

/// Nom exact d'un modèle installé, sans tenir compte de la casse.
async fn installed_name(client: &OllamaClient, name: &str) -> Result<Option<String>> {
    let installed = client.tags().await?;
    Ok(installed
        .into_iter()
        .find(|m| same_model(&m.name, name))
        .map(|m| m.name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Capability;
    use crate::testing::{
        CREATE, FakeOllama, PULL_ERROR_QUANT, PULL_RESUME, PULL_RESUME_TOTAL, Recorded, Reply,
        SHOW_ALIAS, tags_json,
    };
    use std::future::pending;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    const BASE: &str = "hf.co/ggml-org/SmolVLM-256M-Instruct-GGUF:Q8_0";
    const ALIAS: &str = "gram/smolvlm-256m-instruct:q8_0";

    /// Ollama sans la base ; `pull` répond `pull_reply` et, s'il réussit, la base apparaît.
    fn ollama(pull_reply: Reply, appears: bool) -> FakeOllama {
        let pulled = Arc::new(AtomicBool::new(false));
        FakeOllama::streaming(move |r: &Recorded| match r.path.as_str() {
            "/api/tags" if pulled.load(Ordering::SeqCst) => Reply::new(200, tags_json(&[BASE])),
            "/api/tags" => Reply::new(200, tags_json(&["qwen3:8b"])),
            "/api/pull" => {
                pulled.store(appears, Ordering::SeqCst);
                pull_reply.clone()
            }
            "/api/create" => Reply::new(200, CREATE),
            "/api/show" => Reply::new(200, SHOW_ALIAS),
            _ => Reply::new(404, r#"{"error":"404 page not found"}"#),
        })
    }

    fn paths(fake: &FakeOllama) -> Vec<String> {
        fake.requests().iter().map(|r| r.path.clone()).collect()
    }

    #[tokio::test]
    async fn base_deja_installee() {
        // Installée avec une autre casse : son nom exact est repris.
        let installed = "hf.co/GGML-org/smolvlm-256m-instruct-gguf:q8_0";
        let fake = FakeOllama::start(move |r| match r.path.as_str() {
            "/api/tags" => (200, tags_json(&["qwen3:8b", installed])),
            "/api/create" => (200, CREATE.into()),
            "/api/show" => (200, SHOW_ALIAS.into()),
            _ => (404, String::new()),
        });
        let client = OllamaClient::new(fake.url()).unwrap();
        let mut phases = Vec::new();
        let done = install(
            &client,
            BASE,
            ALIAS,
            16384,
            |p| phases.push(p.phase),
            pending(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(done.base, installed);
        assert!(!done.pulled);
        assert_eq!(done.show.num_ctx, Some(16384));
        assert!(done.show.has(&Capability::Completion));
        assert_eq!(phases, [InstallPhase::Preparing, InstallPhase::Creating]);

        assert_eq!(paths(&fake), ["/api/tags", "/api/create", "/api/show"]);
        let create = fake.requests()[1].json();
        assert_eq!(
            create,
            serde_json::json!({
                "model": ALIAS,
                "from": installed,
                "parameters": { "num_ctx": 16384 },
                "stream": false,
            })
        );
        assert_eq!(fake.requests()[2].json()["model"], ALIAS);
    }

    #[tokio::test]
    async fn telechargement_puis_creation() {
        let fake = ollama(Reply::ndjson(PULL_RESUME), true);
        let client = OllamaClient::new(fake.url()).unwrap();
        let mut progress = Vec::new();
        let done = install(&client, BASE, ALIAS, 4096, |p| progress.push(p), pending())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(done.base, BASE);
        assert!(done.pulled);
        assert_eq!(
            paths(&fake),
            [
                "/api/tags",
                "/api/pull",
                "/api/tags",
                "/api/create",
                "/api/show"
            ]
        );
        let pull = fake.requests()[1].json();
        assert_eq!(pull, serde_json::json!({ "model": BASE, "stream": true }));
        assert_eq!(fake.requests()[3].json()["parameters"]["num_ctx"], 4096);

        // Étapes dans l'ordre, une progression par ligne du flux, plus le début et la création.
        let phases: Vec<InstallPhase> = progress.iter().map(|p| p.phase).collect();
        assert!(phases.is_sorted(), "{phases:?}");
        assert_eq!(progress.len(), PULL_RESUME.lines().count() + 2);
        assert_eq!(phases[0], InstallPhase::Preparing);
        assert_eq!(phases[1], InstallPhase::Preparing, "pulling manifest");
        let downloading: Vec<&PullProgress> = progress
            .iter()
            .filter(|p| p.phase == InstallPhase::Downloading)
            .collect();
        // Reprise : 39,5 Mo déjà là dès la première ligne.
        assert_eq!(downloading[0].completed, 39_538_779);
        assert!(
            downloading
                .windows(2)
                .all(|w| w[0].completed <= w[1].completed)
        );
        let verifying = progress
            .iter()
            .find(|p| p.phase == InstallPhase::Verifying)
            .unwrap();
        assert_eq!(verifying.completed, PULL_RESUME_TOTAL);
        assert_eq!(verifying.total, PULL_RESUME_TOTAL);
        assert_eq!(progress.last().unwrap().phase, InstallPhase::Creating);
    }

    #[tokio::test]
    async fn annulation_pendant_le_telechargement() {
        let head: String = PULL_RESUME
            .lines()
            .take(10)
            .map(|l| format!("{l}\n"))
            .collect();
        let reply = Reply::ndjson(&head)
            .pause(Duration::from_secs(10))
            .lines(PULL_RESUME);
        let fake = ollama(reply, true);
        let client = OllamaClient::new(fake.url()).unwrap();
        let mut progress = Vec::new();
        let start = Instant::now();
        let cancel = tokio::time::sleep(Duration::from_millis(500));
        let done = install(&client, BASE, ALIAS, 4096, |p| progress.push(p), cancel)
            .await
            .unwrap();
        assert_eq!(done, None);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{:?}",
            start.elapsed()
        );
        assert_eq!(progress.len(), 11, "début, puis les 10 premières lignes");
        assert_eq!(progress[10].phase, InstallPhase::Downloading);
        assert_eq!(
            paths(&fake),
            ["/api/tags", "/api/pull"],
            "aucun modèle dérivé créé"
        );
    }

    #[tokio::test]
    async fn erreur_pendant_le_telechargement() {
        let fake = ollama(Reply::ndjson(PULL_ERROR_QUANT), false);
        let client = OllamaClient::new(fake.url()).unwrap();
        let err = install(&client, BASE, ALIAS, 4096, |_| {}, pending())
            .await
            .unwrap_err();
        assert!(
            matches!(&err, OllamaError::Interrupted(m) if m.contains("not a valid quantization")),
            "{err:?}"
        );
        assert_eq!(paths(&fake), ["/api/tags", "/api/pull"]);
    }

    #[tokio::test]
    async fn base_introuvable_apres_telechargement() {
        let fake = ollama(Reply::ndjson(PULL_RESUME), false);
        let client = OllamaClient::new(fake.url()).unwrap();
        let err = install(&client, BASE, ALIAS, 4096, |_| {}, pending())
            .await
            .unwrap_err();
        assert!(matches!(err, OllamaError::Invalid(_)), "{err:?}");
        assert_eq!(paths(&fake), ["/api/tags", "/api/pull", "/api/tags"]);
    }

    #[tokio::test]
    async fn ollama_absent() {
        let addr = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let client = OllamaClient::for_addr(addr).unwrap();
        let err = install(&client, BASE, ALIAS, 4096, |_| {}, pending())
            .await
            .unwrap_err();
        assert!(matches!(err, OllamaError::Unreachable(_)), "{err:?}");
    }
}
