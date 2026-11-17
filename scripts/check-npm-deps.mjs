#!/usr/bin/env node
// Vérifie que app/ui/node_modules correspond à app/ui/package-lock.json, en le comparant au
// lockfile caché que npm écrit à chaque installation (node_modules/.package-lock.json). Appelé
// par scripts/check.sh et scripts/check.ps1 : après un pull qui change les dépendances, on
// obtient « lancer npm ci » au lieu d'une avalanche d'erreurs TS2307 dans les tests Angular.
//
//   node scripts/check-npm-deps.mjs   (code de sortie 1 si node_modules est périmé)

import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ui = join(dirname(fileURLToPath(import.meta.url)), '..', 'app', 'ui');
const lockfile = join(ui, 'package-lock.json');
const installedLockfile = join(ui, 'node_modules', '.package-lock.json');

function fail(lines) {
  console.error([...lines, 'Lancer « npm ci » dans app/ui.'].join('\n'));
  process.exit(1);
}

if (!existsSync(installedLockfile)) {
  fail(['node_modules absent ou jamais installé (pas de node_modules/.package-lock.json).']);
}

const expected = JSON.parse(readFileSync(lockfile, 'utf8')).packages;
const installed = JSON.parse(readFileSync(installedLockfile, 'utf8')).packages;
const name = (path) => path.replace(/^node_modules\//, '');

const problems = [];
for (const [path, pkg] of Object.entries(expected)) {
  if (path === '') continue; // le projet lui-même
  const got = installed[path];
  if (!got) {
    // Les paquets optionnels d'une autre plateforme (@esbuild/win32-x64 sur le Mac…) ne sont
    // jamais installés : leur absence est normale.
    if (!pkg.optional && !pkg.devOptional) problems.push(`manquant : ${name(path)} ${pkg.version}`);
  } else if (got.version !== pkg.version) {
    problems.push(`version : ${name(path)} ${got.version} installé, ${pkg.version} attendu`);
  }
}
for (const [path, pkg] of Object.entries(installed)) {
  if (!(path in expected)) problems.push(`en trop : ${name(path)} ${pkg.version}`);
}

if (problems.length > 0) {
  const shown = 10;
  const more = problems.length > shown ? [`… et ${problems.length - shown} autres`] : [];
  fail(['node_modules ne correspond pas à package-lock.json :', ...problems.slice(0, shown), ...more]);
}
console.log('node_modules à jour.');
