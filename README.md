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

Les types Rust exposés au frontend sont convertis en TypeScript par `ts-rs` dans `app/ui/src/app/core/generated/` à chaque `cargo test`. Ne pas modifier ces fichiers à la main.

`npm run tauri` passe par `app/ui/scripts/tauri.mjs`, qui indique à la CLI Tauri où se trouve `app/src-tauri` (variable `TAURI_APP_PATH`).
