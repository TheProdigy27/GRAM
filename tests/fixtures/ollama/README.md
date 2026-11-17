# Réponses de l'API d'Ollama

Utilisées par `crates/gram-ollama` (analyse des réponses, faux serveur des tests) et par les
tests de l'application (`app/src-tauri/src/tests.rs`).

Enregistrées le 11 novembre 2026 avec Ollama 0.40.16 (Windows 11, RTX 5080 16 Go), pour
`hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M` installé par `ollama pull`, en rejouant le protocole de mesure
de GRAM sur le modèle déchargé : échauffement, mesure, puis `/api/ps`.

| Fichier | Requête |
|---|---|
| `tags.json` | `GET /api/tags`. Seule la première entrée est réelle. Les trois autres, au format documenté, couvrent des cas absents de la machine : MoE installé depuis le dépôt d'unsloth, modèle de la bibliothèque d'Ollama (`qwen3:8b`) et modèle distant (`remote_host`), écarté par GRAM |
| `generate-warmup.json` | `POST /api/generate`, échauffement (8 tokens, `num_ctx` 8192) : le chargement du modèle est compté dans `load_duration` |
| `generate.json` | `POST /api/generate`, mesure (128 tokens, mêmes options) : 135 tok/s ; 3 des 45 tokens du prompt sont repris du cache (`prompt_eval_cached_count`) |
| `ps.json` | `GET /api/ps` après la mesure |
| `error-not-found.json` | Erreur pour un modèle absent (HTTP 404) |

Toutes les durées sont en nanosecondes. Ce qu'Ollama ajoute par rapport à la documentation
(`https://docs.ollama.com/api`) : `capabilities` et `details.context_length` dans `/api/tags`,
`runner` dans `/api/ps`, `prompt_eval_cached_count` dans `/api/generate`. Qwen3 réfléchit avant de
répondre : les 128 tokens vont dans `thinking` et `response` reste vide, sans effet sur la mesure
(`eval_count` compte les deux).

## Ajout pour GRAM Harness

Enregistrées le 12 novembre 2026, même machine et même version d'Ollama, avec
`hf.co/unsloth/Qwen3-1.7B-GGUF:Q4_K_M` (déjà installé) et
`hf.co/ggml-org/SmolVLM-256M-Instruct-GGUF:Q8_0` (petit modèle de vision, téléchargé pour
l'occasion puis supprimé). Dans les `modelfile` de `show-*.json`, le dossier de l'utilisateur
devient `C:\Users\gram`.

| Fichier | Requête |
|---|---|
| `pull-installed.ndjson` | `POST /api/pull` (`stream: true`) d'un modèle déjà installé : 9 lignes en 0,45 s, une seule par couche, déjà complète |
| `pull-resume.ndjson` | `POST /api/pull` de SmolVLM, repris après une annulation : la première ligne repart de 39,5 Mo ; trois couches (modèle, projecteur `mmproj`, paramètres) ; les premières lignes du projecteur n'ont pas de `completed` ; la dernière ligne du modèle s'arrête avant `total` ; une ligne toutes les 60 ms |
| `pull-error-quant.ndjson` | Quantification qui n'existe pas dans le dépôt : statut HTTP 200, puis une ligne `{"error": …}` |
| `pull-error-gated.ndjson` | Dépôt protégé depuis `hf.co/` : `realm host "huggingface.co" does not match original host "hf.co"`. Un dépôt **inexistant** donne exactement la même erreur |
| `pull-error-gated-key.ndjson` | Même dépôt protégé depuis `huggingface.co/` : Ollama signe avec sa clé, Hugging Face répond 401 « Invalid username or password » (clé non enregistrée). Là encore, un dépôt inexistant donne la même erreur |
| `pull-error-sharded.ndjson` | Quantification découpée en plusieurs fichiers : refusée par Hugging Face dès le manifeste |
| `create.json` | `POST /api/create` (`stream: false`) du modèle dérivé `gram/qwen3-1.7b:q4_k_m`, `num_ctx` 16384 |
| `create-error-split.json` | `POST /api/create` (HTTP 500), enregistré le 17 novembre 2026, depuis `hf.co/huihui-ai/Huihui-DeepSeek-V4.1-Flash-1017-abliterated-GGUF:IQ1_S` : le seul fichier de cette quantification (`Maya/…-Maya-XS-IQ1_S.gguf`, 1,8 Go) est la première partie d'un modèle découpé en 5 (`split.count`), sous un nom qui ne le dit pas. Hugging Face le sert seul, `/api/pull` réussit, la création échoue |
| `show-base.json` | `POST /api/show` de la base : des paramètres (`stop`, `temperature`…), pas de `num_ctx` |
| `show-alias.json` | `POST /api/show` du modèle dérivé : les paramètres de la base, plus `num_ctx 16384` ; `details.parent_model` donne la base |
| `show-vision.json` | `POST /api/show` de SmolVLM : capacité `vision`, `projector_info`, aucun paramètre (`parameters` absent) |

Sans flux (`stream: false`), une erreur de `pull` arrive avec le statut HTTP 500. `DELETE
/api/delete` répond 200 sans corps, ou 404 avec `error-not-found.json` si le modèle est déjà absent.

## Discussion dans GRAM Harness

Enregistrées le 13 novembre 2026, même machine et même version d'Ollama, avec
`hf.co/unsloth/Qwen3-1.7B-GGUF:Q4_K_M` (déjà installé), en flux (`stream: true`), `seed` 42 et
`temperature` 0.

| Fichier | Requête |
|---|---|
| `chat-default.ndjson` | `POST /api/chat` sans `think`, modèle à charger : Qwen3 réfléchit quand même, et la réflexion arrive à part dans `message.thinking` (99 lignes de réflexion puis de réponse, puis le bilan) ; `load_duration` 1,47 s |
| `chat-think.ndjson` | Même requête avec `think: true` : même format |
| `chat-no-think.ndjson` | `think: false` : réponse seule, 11 lignes |
| `chat-length.ndjson` | Instructions (`system`), historique, `num_predict` 5 : `done_reason` `length` |
| `chat-not-found.json` | Modèle absent : HTTP 404, avant tout flux |

Chaque ligne porte un morceau de `message.content` ou de `message.thinking` (l'autre est vide) ;
la dernière (`done: true`) a un contenu vide et le bilan : `done_reason`, `prompt_eval_count`,
`prompt_eval_cached_count`, `eval_count` (réflexion comprise) et les durées. Fermer la connexion
arrête la génération (« srv stop: cancel task » dans le journal d'Ollama) ; la requête suivante
est servie aussitôt.

## Images et réglages de GRAM Harness

Enregistrées le 17 novembre 2026, même machine et même version d'Ollama.

| Fichier | Requête |
|---|---|
| `chat-image-unsupported.json` | `POST /api/chat` avec une image (`images`, base64) pour Qwen3 1.7B, qui n'a pas la vision : HTTP 500 avant tout flux. Même réponse quand l'image n'est que dans l'historique, et que la question en cours n'en a pas |

Avec un modèle de vision de 27 Md déjà installé, la même requête répond normalement (image PNG de
128 × 128 décrite correctement ; `prompt_eval_count` 39, image comprise). `keep_alive` : `"30m"`
et `"-1m"` (toujours, `expires_at` en 2319) sont acceptés, comme le nombre `-1` ; la chaîne `"-1"`
est refusée (HTTP 400, `time: missing unit in duration "-1"`). `/api/ps` donne `context_length`
du modèle chargé (4 096 pour une base sans `num_ctx`).
