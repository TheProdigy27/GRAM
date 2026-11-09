# Échantillon figé du catalogue

`sample.json` : 35 entrées (`ModelEntry`, une par ligne) extraites du catalogue embarqué `data/seed_catalog.json` du 9 novembre 2026. Les tests de classement (`crates/gram-core/tests/ranking.rs`) et leurs snapshots en dépendent. Ce fichier est figé exprès : régénérer le catalogue embarqué (`gram-cli snapshot`) ne change pas les snapshots.

Il couvre :

- 28 modèles détaillés (tailles réelles), de 0,6B à 197B : denses (Qwen3-0.6B → Llama-3.3-70B), MoE (Ornith-1.5-35B-A3B, Qwen3-Next-80B-A3B, Qwen3-Coder-Next, Ornith-1.5-122B), un 197B sans architecture connue (DeepSeek-V4.1-Flash-GSQ-RCO), vision (Qwen3-VL, gemma-4), code (Qwen2.5-Coder-14B, TwIL-8B), raisonnement (Mellum2.1-Thinking) et fine-tunes ;
- 7 modèles non détaillés (échelle de repli, architecture parfois inconnue) : un MoE de code, un modèle de raisonnement, un dépôt protégé (gemma-7b), un petit modèle, un 70B, un modèle vision sans architecture, un MoE à moins de 2B actifs ;
- un modèle dont l'unique quantification est sous le minimum de 2,5 bits par poids (Nemotron-3.5-Bolt-74B-A9B-24GB), donc jamais proposé.

Pour ajouter un cas : copier l'entrée voulue depuis `data/seed_catalog.json` (ou le cache `%LOCALAPPDATA%\GRAM\cache\catalog.json`) sur une nouvelle ligne du tableau, puis relire les différences de snapshot avec `cargo insta review`.
