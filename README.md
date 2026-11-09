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

powershell -ExecutionPolicy Bypass -File scripts/check.ps1   # CI locale complète
```

## Structure

| Dossier | Rôle |
|---|---|
| `crates/gram-core` | Types, moteur d'estimation, scoring (pur, sans I/O) |
| `crates/gram-hw` | Détection matériel et runtimes installés (Windows) |
| `crates/gram-hub` | Client Hugging Face, cache, parseur GGUF |
| `crates/gram-cli` | Binaire de debug : `hw`, `sync`, `rank`, `snapshot` |
| `app/src-tauri` | Application Tauri : commandes IPC, état, événements |
| `app/ui` | Frontend Angular + Angular Material |
| `data` | Tables embarquées (`gpu_specs.json` : bande passante des GPU, liste du mode simulation) |
| `tests/fixtures` | Données réelles pour les tests (dumps matériels `gram-cli hw --raw`, réponses Hugging Face) |

Les types Rust exposés au frontend sont convertis en TypeScript par `ts-rs` dans `app/ui/src/app/core/generated/` à chaque `cargo test`. Ne pas modifier ces fichiers à la main.

`npm run tauri` passe par `app/ui/scripts/tauri.mjs`, qui indique à la CLI Tauri où se trouve `app/src-tauri` (variable `TAURI_APP_PATH`).

## Détection du matériel

`gram-hw` sépare la collecte (DXGI + DXCore, NVML optionnelle, WMI, sysinfo : propre à Windows) de la construction du profil (`build_profile`, pure et testée). Les corrections manuelles et la simulation (`Overrides`) sont des entrées de `build_profile` : on repart toujours des données brutes.

Pour ajouter un cas de test matériel (portable, GPU AMD, machine sans GPU…) : lancer `gram-cli hw --raw` sur la machine concernée, enregistrer la sortie dans `tests/fixtures/hardware/` et ajouter un test dans `crates/gram-hw/src/profile/tests.rs`.
