//! Ce que Hugging Face sert à Ollama : `GET /v2/{dépôt}/manifests/{quant}`, la première
//! requête de `ollama pull hf.co/{dépôt}:{quant}`. Elle ne télécharge rien, et dit si la
//! quantification sera servie (avec la taille du fichier que Hugging Face a choisi pour cette
//! étiquette) ou pourquoi elle ne le sera pas : étiquette absente du dépôt ou qui n'est pas
//! une quantification, quantification découpée en plusieurs fichiers, dépôt protégé ou
//! introuvable.
//!
//! La réponse est la même quel que soit le client (constaté le 17 novembre 2026 avec
//! l'agent `GRAM/…` comme avec celui d'Ollama).
//!
//! Hugging Face peut aussi servir seul le premier fichier d'un modèle découpé dont le nom ne
//! le dit pas : `ollama create` le refuse ensuite (« invalid split GGUF »), après le
//! téléchargement. Quand le fichier servi est celui que GRAM a évalué, son en-tête GGUF le
//! dit avant ([`GgufMeta::split_part`](crate::gguf::GgufMeta::split_part)).

use crate::api;
use crate::error::Result;
use crate::gguf::{self, Parsed};
use crate::http::{Hub, Request, Transport};
use gram_core::QuantVariant;
use gram_core::harness::{Availability, PullFailure};
use serde::Deserialize;

/// Début du fichier lu pour son en-tête : les clés d'architecture et le nombre de tenseurs y
/// sont presque toujours.
const HEADER_LEN: u64 = 64 << 10;

const MODEL_LAYER: &str = "application/vnd.ollama.image.model";
const PROJECTOR_LAYER: &str = "application/vnd.ollama.image.projector";

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    layers: Vec<Layer>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Layer {
    media_type: String,
    size: u64,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

/// Lit la réponse : manifeste (2xx), sinon refus (4xx, `{"error": "…"}`).
pub fn parse_manifest(status: u16, body: &[u8]) -> Availability {
    if (200..300).contains(&status) {
        let Ok(manifest) = serde_json::from_slice::<Manifest>(body) else {
            return Availability::Unknown("manifeste illisible".into());
        };
        let size = |media_type: &str| {
            let mut layers = manifest
                .layers
                .iter()
                .filter(|l| l.media_type == media_type)
                .peekable();
            layers
                .peek()
                .is_some()
                .then(|| layers.map(|l| l.size).sum())
        };
        return match size(MODEL_LAYER) {
            Some(model) => Availability::Served {
                model,
                projector: size(PROJECTOR_LAYER),
            },
            None => Availability::Unknown("manifeste sans fichier de modèle".into()),
        };
    }
    let message = serde_json::from_slice::<ErrorBody>(body)
        .map(|e| e.error)
        .unwrap_or_else(|_| format!("HTTP {status}"));
    let failure = PullFailure::recognize(&message)
        .or_else(|| (status == 401).then_some(PullFailure::Unauthorized));
    Availability::Refused { failure, message }
}

/// Demande à Hugging Face s'il sert `variant` à Ollama, puis, si le fichier servi est celui
/// que GRAM a évalué (un seul fichier, même taille), lit son en-tête : une partie d'un modèle
/// découpé est refusée ([`PullFailure::SplitPart`]). Erreur : pas de réponse exploitable du
/// manifeste (réseau, quota, erreur du serveur) ; un en-tête illisible ne refuse rien.
pub async fn ollama_availability<T: Transport>(
    hub: &Hub<T>,
    variant: &QuantVariant,
) -> Result<Availability> {
    let repo = &variant.repo_id;
    let url = api::manifest_url(hub.base(), repo, &variant.label);
    let resp = hub.get_or_client_error(&Request::api(url)).await?;
    let availability = parse_manifest(resp.status, &resp.body);
    if let Availability::Served { model, .. } = availability
        && let [file] = variant.files.as_slice()
        && model == variant.size_bytes
        && split_part(hub, repo, file).await
    {
        return Ok(Availability::Refused {
            failure: Some(PullFailure::SplitPart),
            message: format!("{file} n'est qu'une partie d'un modèle découpé"),
        });
    }
    Ok(availability)
}

/// L'en-tête de `file` dit qu'il n'est qu'une partie d'un modèle découpé.
async fn split_part<T: Transport>(hub: &Hub<T>, repo: &str, file: &str) -> bool {
    let url = api::resolve_url(hub.base(), repo, "main", file);
    match hub.get(&Request::resolver(url, Some(0..HEADER_LEN))).await {
        Ok(resp) => match gguf::parse(&resp.body) {
            Ok(Parsed::Complete(m) | Parsed::Partial(m)) => m.split_part(),
            Ok(Parsed::NeedMore) => false,
            Err(e) => {
                tracing::debug!(repo, file, "en-tête GGUF : {e}");
                false
            }
        },
        Err(e) => {
            tracing::debug!(repo, file, "en-tête GGUF inaccessible : {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HubError;
    use crate::http::{Policy, Response};
    use crate::test_support::{FakeTransport, fixture};
    use std::time::Duration;

    const REPO: &str = "huihui-ai/Huihui-DeepSeek-V4.1-Flash-1017-abliterated-GGUF";
    const MAYA: &str = "Maya/DeepSeek-V4.1-Flash-1017-Maya-XS-IQ1_S.gguf";

    fn variant(label: &str, files: &[&str], size_bytes: u64) -> QuantVariant {
        QuantVariant {
            label: label.into(),
            repo_id: REPO.into(),
            files: files.iter().map(|&f| f.to_owned()).collect(),
            size_bytes,
            bpw: 8.5,
            exact: true,
        }
    }

    fn manifest(name: &str, status: u16) -> Availability {
        parse_manifest(status, &fixture(&format!("manifests/{name}.json")))
    }

    fn refused(failure: Option<PullFailure>, message: &str) -> Availability {
        Availability::Refused {
            failure,
            message: message.into(),
        }
    }

    #[test]
    fn manifestes_servis() {
        // bartowski : modèle, gabarit et paramètres, pas de projecteur.
        assert_eq!(
            manifest("served", 200),
            Availability::Served {
                model: 5_027_784_224,
                projector: None
            }
        );
        // huihui-ai, IQ1_S : le fichier `Maya/…` et le projecteur `mmproj-BF16.gguf`.
        assert_eq!(
            manifest("served-vision", 200),
            Availability::Served {
                model: 1_788_364_288,
                projector: Some(972_208_288)
            }
        );
        assert!(matches!(
            parse_manifest(200, br#"{"layers":[]}"#),
            Availability::Unknown(_)
        ));
        assert!(matches!(
            parse_manifest(200, b"<html>"),
            Availability::Unknown(_)
        ));
    }

    #[test]
    fn refus_reconnus() {
        let unavailable = "The specified tag is not available in the repository. Please use \
                           another tag or \"latest\"";
        assert_eq!(
            manifest("tag-unavailable", 400),
            refused(Some(PullFailure::UnknownQuant), unavailable)
        );
        assert_eq!(
            manifest("invalid-quant", 400),
            refused(
                Some(PullFailure::UnknownQuant),
                "The specified tag is not a valid quantization scheme. Please use another tag \
                 or \"latest\""
            )
        );
        assert!(matches!(
            manifest("sharded", 400),
            Availability::Refused {
                failure: Some(PullFailure::Sharded),
                ..
            }
        ));
        // Dépôt protégé, ou inexistant : même réponse.
        assert_eq!(
            manifest("unauthorized", 401),
            refused(
                Some(PullFailure::Unauthorized),
                "Invalid username or password."
            )
        );
        // Refus inconnu : le message de Hugging Face, sinon le statut.
        assert_eq!(
            parse_manifest(403, br#"{"error":"Nope"}"#),
            refused(None, "Nope")
        );
        assert_eq!(
            parse_manifest(401, b""),
            refused(Some(PullFailure::Unauthorized), "HTTP 401")
        );
    }

    fn hub(t: FakeTransport) -> Hub<FakeTransport> {
        Hub::with_policy(
            t,
            Policy {
                spacing: Duration::ZERO,
                ..Policy::default()
            },
        )
    }

    #[tokio::test(start_paused = true)]
    async fn requete_et_erreurs_du_client() {
        let url = format!("https://huggingface.co/v2/{REPO}/manifests/Q8_0");
        assert_eq!(api::manifest_url(crate::api::HF_BASE, REPO, "Q8_0"), url);
        let t = FakeTransport::new();
        t.push(
            &url,
            Response {
                status: 400,
                ..Response::ok(fixture("manifests/tag-unavailable.json"))
            },
        );
        let h = hub(t);
        let a = ollama_availability(&h, &variant("Q8_0", &[], 1))
            .await
            .unwrap();
        assert!(matches!(
            a,
            Availability::Refused {
                failure: Some(PullFailure::UnknownQuant),
                ..
            }
        ));
        // Le refus est une réponse : pas de nouvelle tentative.
        assert_eq!(h.api_calls(), 1);

        // Réseau coupé : erreur, pour un avertissement.
        let t = FakeTransport::new();
        t.offline.store(true, std::sync::atomic::Ordering::Relaxed);
        let err = ollama_availability(&hub(t), &variant("Q4_K_M", &[], 1))
            .await
            .unwrap_err();
        assert!(matches!(err, HubError::Network(_)), "{err}");
    }

    #[tokio::test(start_paused = true)]
    async fn premiere_partie_servie_seule() {
        let manifest = format!("https://huggingface.co/v2/{REPO}/manifests/IQ1_S");
        let header = api::resolve_url(crate::api::HF_BASE, REPO, "main", MAYA);
        let route = move |req: &Request| {
            if req.url == manifest {
                Some(Response::ok(fixture("manifests/served-vision.json")))
            } else if req.url == header {
                assert_eq!(req.range, Some(0..HEADER_LEN));
                Some(Response::ok(fixture(
                    "gguf/huihui-ai--Huihui-DeepSeek-V4.1-Flash-1017-abliterated-GGUF--Maya-XS-IQ1_S.bin",
                )))
            } else {
                None
            }
        };
        // Le fichier servi est celui évalué (même taille) : 40 tenseurs pour 40 couches.
        let h = hub(FakeTransport::with_route(route.clone()));
        let maya = variant("IQ1_S", &[MAYA], 1_788_364_288);
        assert_eq!(
            ollama_availability(&h, &maya).await.unwrap(),
            Availability::Refused {
                failure: Some(PullFailure::SplitPart),
                message: format!("{MAYA} n'est qu'une partie d'un modèle découpé"),
            }
        );
        assert_eq!((h.api_calls(), h.resolver_calls()), (1, 1));

        // Un autre fichier que celui évalué : l'en-tête n'est pas lu.
        let h = hub(FakeTransport::with_route(route.clone()));
        let other = variant("IQ1_S", &[MAYA], 2_000_000_000);
        assert!(matches!(
            ollama_availability(&h, &other).await.unwrap(),
            Availability::Served { .. }
        ));
        assert_eq!(h.resolver_calls(), 0);

        // En-tête inaccessible (404) : rien n'est refusé.
        let h = hub(FakeTransport::with_route(move |req: &Request| {
            req.url
                .contains("/manifests/")
                .then(|| Response::ok(fixture("manifests/served-vision.json")))
        }));
        assert!(matches!(
            ollama_availability(&h, &maya).await.unwrap(),
            Availability::Served { .. }
        ));
    }
}
