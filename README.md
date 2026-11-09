# GRAM

*GPU & RAM Aware Matcher* : une application Windows (Tauri 2 + Rust + Angular) qui analyse le matériel de la machine et classe les modèles IA exécutables en local, à partir des données Hugging Face.

## Prérequis

- Rust stable (MSVC) et les Build Tools Visual Studio
- Node.js 24+ et npm
- WebView2 (déjà présent sous Windows 11)

## Commandes

```powershell
cd app/ui
npm install
npm run tauri dev       # lance l'application (Angular sur :1420 + fenêtre Tauri)
npm run test:ci         # tests Angular (Vitest)

cd ../..
cargo test --workspace  # tests Rust ; régénère aussi les types TypeScript
cargo run -p gram-cli -- --help
cargo run -p gram-cli -- hw          # matériel détecté (à comparer au Gestionnaire des tâches)
cargo run -p gram-cli -- hw --json   # { hardware, runtimes }
cargo run -p gram-cli -- hw --raw    # données brutes, à enregistrer dans tests/fixtures/hardware/
cargo run -p gram-cli -- sync        # catalogue Hugging Face (cache de 24 h ; --force pour resynchroniser)
cargo run -p gram-cli -- sync --offline   # catalogue du cache, sans réseau
cargo run -p gram-cli -- snapshot    # régénère data/seed_catalog.json (catalogue embarqué)
cargo run -p gram-cli -- estimate Qwen3-8B             # chaque quantification estimée pour cette machine
cargo run -p gram-cli -- estimate Qwen3-8B --ctx 32768 --kv q8_0 --vision
cargo run -p gram-cli -- estimate Llama-3.3-70B --profile tests/fixtures/profiles/p3-rtx4090-24go-64go-ddr5.json

powershell -ExecutionPolicy Bypass -File scripts/check.ps1   # CI locale complète
```

## Structure

| Dossier | Rôle |
|---|---|
| `crates/gram-core` | Types, moteur d'estimation, scoring (pur, sans I/O) |
| `crates/gram-hw` | Détection matériel et runtimes installés (Windows) |
| `crates/gram-hub` | Client Hugging Face, cache, parseur GGUF, dédoublonnage, catalogue embarqué |
| `crates/gram-cli` | Binaire de debug : `hw`, `sync`, `estimate`, `rank`, `snapshot` |
| `app/src-tauri` | Application Tauri : commandes IPC, état, événements |
| `app/ui` | Frontend Angular + Angular Material |
| `data` | Tables embarquées : `gpu_specs.json` (bande passante des GPU, liste du mode simulation), `uploaders.json` (priorité des uploaders GGUF), `quant_quality.json` (bits par poids typiques, pour les modèles pas encore détaillés), `seed_catalog.json` (catalogue de démarrage, généré) |
| `tests/fixtures` | Données pour les tests : dumps matériels `gram-cli hw --raw`, réponses Hugging Face, profils matériels types (`profiles/`) et modèles réels du catalogue (`models/`) |

Les types Rust exposés au frontend sont convertis en TypeScript par `ts-rs` dans `app/ui/src/app/core/generated/` à chaque `cargo test`. Ne pas modifier ces fichiers à la main.

`npm run tauri` passe par `app/ui/scripts/tauri.mjs`, qui indique à la CLI Tauri où se trouve `app/src-tauri` (variable `TAURI_APP_PATH`).

## Détection du matériel

`gram-hw` sépare la collecte (DXGI + DXCore, NVML optionnelle, WMI, sysinfo : propre à Windows) de la construction du profil (`build_profile`, pure et testée). Les corrections manuelles et la simulation (`Overrides`) sont des entrées de `build_profile` : on repart toujours des données brutes.

Pour ajouter un cas de test matériel (portable, GPU AMD, machine sans GPU…) : lancer `gram-cli hw --raw` sur la machine concernée, enregistrer la sortie dans `tests/fixtures/hardware/` et ajouter un test dans `crates/gram-hw/src/profile/tests.rs`.

## Données Hugging Face

`gram-hub` ne parcourt jamais tout le Hub. Une synchronisation liste les modèles GGUF par tranche de taille (jusqu'au plus gros modèle plausible sur la machine) et par pipeline (texte, vision), puis :

1. elle les regroupe par modèle de base, d'après les tags `base_model:quantized:…` : un fine-tune reste à part ;
2. elle choisit le dépôt canonique (organisation officielle, puis `data/uploaders.json`, puis le plus téléchargé) ;
3. elle charge les tailles réelles des fichiers pour les 150 premiers groupes, et réunit les quantifications de tous les dépôts du groupe. Si le dépôt canonique en propose peu (un dépôt officiel limité à Q4_K_M et au-dessus, par exemple), elle charge jusqu'à 2 dépôts alternatifs. Pour un même libellé, le dépôt prioritaire l'emporte ;
4. elle lit l'architecture dans les 64 premiers Kio de l'en-tête GGUF, et à défaut dans le `config.json` du modèle de base.

Une première synchronisation coûte environ 230 appels API et 150 appels resolver, ce qui reste sous le quota anonyme (500 par 5 minutes). Les suivantes réutilisent le cache. La fiche d'un modèle (`enrich`) réunit les quantifications de tous ses dépôts, dans la limite de 10 alternatives.

- Cache : `%LOCALAPPDATA%\GRAM\cache\` (`catalog.json`, `details/` et `arch/` indexés par commit, donc gardés indéfiniment).
- Hors ligne, ou si Hugging Face est injoignable : cache, puis catalogue embarqué (`data/seed_catalog.json`).
- Token optionnel (quotas plus larges) : variable `HF_TOKEN` pour `gram-cli`. Il n'est jamais affiché ni écrit.
- Tests : réponses réelles enregistrées dans `tests/fixtures/hf/` (voir son README), aucun appel réseau.

## Estimation

`gram-core::estimate` calcule, pour un modèle, une quantification et une machine, la mémoire nécessaire, le mode d'exécution, le contexte maximal et la vitesse de génération (§8 du plan). C'est du calcul pur, sans I/O.

1. **Mémoire utilisable** (`MemoryBudget::from_profile`) : `min(VRAM × 90 %, VRAM libre + 256 Mio)` sur le GPU retenu, et `min(RAM disponible, RAM × 75 %)`. Le réglage « je ferme tout » passe à `RAM − 4 Go`. Un iGPU compte comme du CPU.
2. **Besoin** : poids (taille réelle des fichiers) + cache KV (`2 × couches d'attention × têtes KV × head_dim × octets`, f16 / q8_0 / q4_0) + 0,5 Go de surcoût (+ le projecteur vision si l'usage vision est demandé).
3. **Mode** : GPU si tout tient en VRAM ; sinon hybride (KV et surcoût en VRAM, puis des couches entières : le `-ngl` de llama.cpp) ; sinon CPU. Si rien ne tient, le contexte est divisé par deux jusqu'à 2 048 tokens, puis le modèle est déclaré trop gros.
4. **Vitesse** : la génération est limitée par la bande passante, plus un temps fixe par token. `temps = (octets GPU / BW GPU + octets RAM / BW RAM) / 0,55 + 1,5 ms` et `tok/s = 1 / temps`. Un MoE ne lit que ses paramètres actifs. Le temps fixe (lancement des calculs, routage des experts) compte surtout quand un token lit peu d'octets : sans lui, un MoE à 3B actifs ou un modèle de 0,6B serait surestimé d'un facteur 1,5 à 2 sur GPU. Les valeurs 0,55 et 1,5 ms sont prudentes, tirées de vitesses publiées ; la phase 7 les calibrera sur la machine.
5. **Adéquation** : parfait (GPU, ≤ 80 % de la VRAM utilisable), bon (GPU ≤ 95 %, ou au moins 75 % des couches sur le GPU), limite (autre hybride, CPU), trop juste.

Les quantifications sous 2,5 bits par poids ne sont pas proposées. Sans tailles réelles (modèle pas encore détaillé), l'estimation part de l'échelle Q8_0 → Q2_K de `data/quant_quality.json`. Sans architecture, elle prend un cache KV prudent. Ces deux cas sont marqués « estimation approximative ».

Limites connues (prévues pour plus tard) : l'attention à fenêtre glissante (Gemma, gpt-oss) n'est pas modélisée, donc le KV est surestimé (c'est signalé). Le traitement du prompt n'est pas estimé. La précision visée est de ±30 %.

Profils types du §15 (`tests/fixtures/profiles/`), testés dans `crates/gram-core/tests/profiles.rs` avec des modèles réels du catalogue, contexte 8k :

| Profil | Qwen3-8B Q4_K_M (4,7 Go) | Llama-3.3-70B Q4_K_M (39,6 Go) |
|---|---|---|
| P1 : portable iGPU, 16 Go DDR4-3200 | CPU, ~5,6 tok/s | trop gros |
| P2 : RTX 3060 12 Go + 32 Go DDR4-3600 | GPU, ~37 tok/s, parfait | trop gros |
| P3 : RTX 4090 24 Go + 64 Go DDR5-6000 | GPU, ~95 tok/s, parfait | hybride 37/80 couches, ~2,1 tok/s |
| P4 : RX 7800 XT 16 Go + 32 Go DDR5-6000 | GPU, ~62 tok/s, parfait | trop gros |
