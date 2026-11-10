//! Mesure réelle d'un modèle installé dans Ollama (phase 7 du plan).
//!
//! 1. **Échauffement** : une courte génération charge le modèle au contexte demandé
//!    (`num_ctx`) et prépare les noyaux de calcul du GPU ; la durée de chargement est notée.
//! 2. **Mesure** : [`MEASURE_TOKENS`] tokens générés avec les mêmes options (pas de
//!    rechargement). Ollama compte lui-même `eval_count` et `eval_duration`, sans le traitement
//!    du prompt : c'est la vitesse de génération qu'estime GRAM.
//! 3. **Mémoire** : `GET /api/ps` donne la mémoire occupée et la part en VRAM.
//!
//! Le modèle reste chargé 5 minutes après la mesure, comme après un usage normal d'Ollama.

use crate::api::{GenerateOptions, GenerateRequest};
use crate::client::OllamaClient;
use crate::error::Result;
use gram_core::OllamaRun;

/// Tokens générés pour la mesure : assez pour une moyenne stable, sans attendre plus d'une
/// minute à 2 tok/s.
pub const MEASURE_TOKENS: u32 = 128;

const WARMUP_TOKENS: u32 = 8;
const KEEP_ALIVE: &str = "5m";
const SEED: i64 = 42;
const WARMUP_PROMPT: &str = "Bonjour.";
/// Demande une réponse longue : le modèle ne s'arrête pas avant `num_predict`.
const PROMPT: &str = "Raconte en détail, en plusieurs longs paragraphes, l'histoire de \
                      l'imprimerie, de la Chine ancienne à l'imprimerie numérique.";

/// Mesure `model` (nom dans Ollama) au contexte `ctx`.
pub async fn measure(client: &OllamaClient, model: &str, ctx: u32) -> Result<OllamaRun> {
    let request = |prompt, num_predict| GenerateRequest {
        model,
        prompt,
        stream: false,
        keep_alive: KEEP_ALIVE,
        options: GenerateOptions {
            num_ctx: ctx,
            num_predict,
            temperature: 0.0,
            seed: SEED,
        },
    };
    tracing::info!(model, ctx, "mesure Ollama : chargement");
    let warmup = client
        .generate(&request(WARMUP_PROMPT, WARMUP_TOKENS))
        .await?;
    let run = client.generate(&request(PROMPT, MEASURE_TOKENS)).await?;
    let loaded = client.ps().await?;
    let entry = loaded.iter().find(|e| e.is(model));
    let others = loaded.len() - usize::from(entry.is_some());
    let result = OllamaRun {
        eval_count: run.eval_count,
        eval_duration_ns: run.eval_duration,
        prompt_eval_count: run.prompt_eval_count,
        prompt_eval_duration_ns: run.prompt_eval_duration,
        load_duration_ns: warmup.load_duration.saturating_add(run.load_duration),
        loaded: entry.map(|e| e.loaded()),
        others_loaded: u32::try_from(others).unwrap_or(u32::MAX),
    };
    tracing::info!(
        model,
        tokens = result.eval_count,
        eval_ms = result.eval_duration_ns / 1_000_000,
        "mesure Ollama terminée"
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OllamaError;
    use crate::testing::{FakeOllama, NOT_FOUND, standard_response};
    use gram_core::LoadedModel;

    const MODEL: &str = "hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M";

    #[tokio::test]
    async fn protocole_de_mesure() {
        let fake = FakeOllama::standard();
        let client = OllamaClient::new(fake.url()).unwrap();
        let run = measure(&client, MODEL, 8192).await.unwrap();
        assert_eq!(run.eval_count, 128);
        assert_eq!(run.eval_duration_ns, 1_542_168_000);
        // Les deux générations renvoient la même fixture : chargement compté deux fois.
        assert_eq!(run.load_duration_ns, 2 * 2_731_000_000);
        assert_eq!(
            run.loaded,
            Some(LoadedModel {
                size: 6_925_000_000,
                size_vram: 6_925_000_000,
                context_length: Some(8192),
            })
        );
        assert_eq!(run.others_loaded, 0);

        let reqs = fake.requests();
        let paths: Vec<(&str, &str)> = reqs
            .iter()
            .map(|r| (r.method.as_str(), r.path.as_str()))
            .collect();
        assert_eq!(
            paths,
            [
                ("POST", "/api/generate"),
                ("POST", "/api/generate"),
                ("GET", "/api/ps")
            ]
        );
        let (warmup, main) = (reqs[0].json(), reqs[1].json());
        assert_eq!(warmup["model"], MODEL);
        assert_eq!(warmup["stream"], false);
        assert_eq!(warmup["options"]["num_ctx"], 8192);
        assert_eq!(warmup["options"]["num_predict"], WARMUP_TOKENS);
        // Mêmes options de chargement : pas de rechargement entre les deux.
        assert_eq!(main["options"]["num_ctx"], 8192);
        assert_eq!(main["options"]["num_predict"], MEASURE_TOKENS);
        assert_eq!(main["options"]["seed"], warmup["options"]["seed"]);
        assert_eq!(main["keep_alive"], "5m");
    }

    #[tokio::test]
    async fn modele_absent_de_api_ps() {
        let fake = FakeOllama::start(|r| match r.path.as_str() {
            "/api/ps" => (
                200,
                r#"{"models":[{"name":"llama3:latest","size":1,"size_vram":1}]}"#.into(),
            ),
            _ => standard_response(r),
        });
        let client = OllamaClient::new(fake.url()).unwrap();
        let run = measure(&client, MODEL, 4096).await.unwrap();
        assert_eq!((run.loaded, run.others_loaded), (None, 1));
    }

    #[tokio::test]
    async fn modele_introuvable() {
        let fake = FakeOllama::start(|r| match r.path.as_str() {
            "/api/generate" => (404, NOT_FOUND.into()),
            _ => standard_response(r),
        });
        let client = OllamaClient::new(fake.url()).unwrap();
        let err = measure(&client, MODEL, 4096).await.unwrap_err();
        assert!(
            matches!(&err, OllamaError::Api { status: 404, message } if message.contains("not found")),
            "{err:?}"
        );
        assert_eq!(fake.requests().len(), 1, "arrêt dès l'échec du chargement");
    }
}
