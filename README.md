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

powershell -ExecutionPolicy Bypass -File scripts/check.ps1   # CI locale complète
```

## Structure

| Dossier | Rôle |
|---|---|
| `crates/gram-core` | Types, moteur d'estimation, scoring (pur, sans I/O) |
| `crates/gram-hw` | Détection matériel et runtimes installés (Windows) |
| `crates/gram-hub` | Client Hugging Face, cache, parseur GGUF, dédoublonnage, catalogue embarqué |
| `crates/gram-cli` | Binaire de debug : `hw`, `sync`, `rank`, `snapshot` |
| `app/src-tauri` | Application Tauri : commandes IPC, état, événements |
| `app/ui` | Frontend Angular + Angular Material |
| `data` | Tables embarquées : `gpu_specs.json` (bande passante des GPU, liste du mode simulation), `uploaders.json` (priorité des uploaders GGUF), `seed_catalog.json` (catalogue de démarrage, généré) |
| `tests/fixtures` | Données réelles pour les tests (dumps matériels `gram-cli hw --raw`, réponses Hugging Face) |

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
