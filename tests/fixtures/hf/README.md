# Réponses Hugging Face enregistrées

Réponses réelles de l'API Hugging Face, enregistrées le 9 novembre 2026, utilisées par les tests de `gram-hub` (aucun appel réseau dans les tests).

Seule modification : le champ `gguf.chat_template` est retiré (plusieurs Ko par modèle, inutile pour GRAM).

| Dossier | Contenu | Requête |
|---|---|---|
| `list/` | Listes de modèles GGUF (tranche 5–10B) | `GET /api/models?filter=gguf&pipeline_tag=…&num_parameters=min:5B,max:10B&sort=downloads\|trendingScore&limit=100\|50&expand[]=gguf&expand[]=downloads&expand[]=likes&expand[]=trendingScore&expand[]=lastModified&expand[]=createdAt&expand[]=pipeline_tag&expand[]=cardData&expand[]=sha&expand[]=gated&expand[]=tags` |
| `details/` | Détail d'un dépôt, avec la taille de chaque fichier (`{org}--{nom}.json`) | `GET /api/models/{org}/{nom}?blobs=true` |
| `gguf/` | 4 premiers Kio d'un fichier GGUF (`{org}--{nom}--{fichier}.bin`). Tronqués dans les tableaux du tokenizer, après les clés d'architecture. | `GET /{org}/{nom}/resolve/main/{fichier}` avec `Range: bytes=0-4095` |
| `config/` | `config.json` d'un modèle de base (repli quand l'en-tête GGUF est illisible) | `GET /{org}/{nom}/resolve/main/config.json` |

Cas couverts :

- `details/` : bartowski (24 quantifications), unsloth (`UD-…`, `mmproj`, sous-dossiers et fichiers découpés), Qwen officiel (noms en minuscules, découpés), mradermacher (`.gguf.part1of2`), ggml-org (`mmproj`, fichiers auxiliaires `dflash-…` et `mtp-…`), granite hybride.
- `gguf/` : dense (qwen3), hybride à attention linéaire (qwen35, `full_attention_interval`), MoE hybride (qwen35moe, `recurrent_layers`), fenêtre glissante (gemma3), têtes KV par couche (granitehybrid).
- `config/` : configuration simple (Qwen2.5) et multimodale avec `text_config` (Qwen3-VL).

Pour ajouter un cas : refaire la requête correspondante (avec `curl`), retirer `gguf.chat_template`, enregistrer le fichier ici et ajouter un test dans le module concerné de `crates/gram-hub/src/`.
