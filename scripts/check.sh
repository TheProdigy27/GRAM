#!/usr/bin/env bash
# CI locale de GRAM sous macOS (ou Linux) : équivalent de scripts/check.ps1, à lancer avant
# chaque commit de fin de phase.
#
#   1. cargo fmt (vérification)
#   2. cargo clippy -D warnings
#   3. cargo test (régénère aussi les types TypeScript via ts-rs)
#   4. vérifie que les types TypeScript générés sont à jour dans git
#   5. tests Angular (Vitest) et build de production
#
# Usage : scripts/check.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

step() {
  printf '\n\033[36m=== %s ===\033[0m\n' "$1"
}

fail() {
  printf '\033[31mÉCHEC : %s\033[0m\n' "$1"
  exit 1
}

step 'cargo fmt'
cargo fmt --all -- --check || fail 'cargo fmt'

step 'cargo clippy'
cargo clippy --workspace --all-targets -- -D warnings || fail 'cargo clippy'

step 'cargo test'
cargo test --workspace || fail 'cargo test'

step 'types TypeScript à jour'
generated='app/ui/src/app/core/generated'
# Fichiers régénérés différents de l'index git, ou nouveaux fichiers non suivis.
changes="$(git diff --name-only -- "$generated"; git ls-files --others --exclude-standard -- "$generated")"
if [ -n "$changes" ]; then
  printf '\033[33mTypes générés modifiés ou non suivis par git :\n%s\n' "$changes"
  printf 'Vérifier puis ajouter ces fichiers au commit.\033[0m\n'
  fail 'types TypeScript à jour'
fi

cd app/ui
step 'tests Angular'
npm run test:ci || fail 'tests Angular'
step 'build Angular'
npm run build || fail 'build Angular'

printf '\n\033[32mTout est vert.\033[0m\n'
