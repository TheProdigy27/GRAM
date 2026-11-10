# Réponses de l'API d'Ollama

Utilisées par `crates/gram-ollama` (analyse des réponses, faux serveur des tests) et par les
tests de l'application (`app/src-tauri/src/tests.rs`).

| Fichier | Requête |
|---|---|
| `tags.json` | `GET /api/tags` : deux modèles installés depuis Hugging Face (`hf.co/…`), un modèle de la bibliothèque d'Ollama et un modèle distant (`remote_host`), écarté par GRAM |
| `ps.json` | `GET /api/ps` : le modèle chargé après une mesure |
| `generate.json` | `POST /api/generate` avec `stream: false` : statistiques de fin de génération (durées en nanosecondes) |
| `error-not-found.json` | Erreur renvoyée pour un modèle absent (HTTP 404) |

Ces fichiers suivent le format documenté sur `https://docs.ollama.com/api` (consulté le
10 novembre 2026), avec des valeurs plausibles pour Qwen3-8B en Q4_K_M sur une RTX 5080 au
contexte 8k. Ollama n'était pas installé sur la machine de développement : les remplacer par de
vraies réponses dès que possible (`curl http://localhost:11434/api/tags`, etc.).
