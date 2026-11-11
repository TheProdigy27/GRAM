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
