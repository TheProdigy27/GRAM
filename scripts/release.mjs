#!/usr/bin/env node
// Release de GRAM construite sur tes machines : la clé de signature des mises à jour ne quitte
// pas ~/.tauri. La même commande sur le Mac puis sur le PC Windows (ou l'inverse) :
//
//   node scripts/release.mjs 0.2.0 [--notes-file notes.md] [--dry-run]
//
// - Première machine (le tag v0.2.0 n'existe pas encore) : version dans Cargo.toml et
//   app/ui/package.json, commit « Version 0.2.0 », tag annoté dont le message sert de notes de
//   version (par défaut, les commits depuis le tag précédent), push de main et du tag, brouillon
//   de release GitHub.
// - Chaque machine : build de son système (macOS universel, ou Windows x64 NSIS et MSI), paquets de
//   mise à jour signés avec ~/.tauri/gram.key, envoyés dans la release avec le manifeste
//   latest.json complété (scripts/latest-json.mjs).
// - Quand macOS et Windows y sont, la release est publiée : les applications installées la
//   proposent d'elles-mêmes.
//
// Seconde machine : `git pull` avant (main doit être sur le tag). `--dry-run` : build et paquets
// dans target/release-assets/vX.Y.Z, sans commit, tag, push ni GitHub (version actuelle seulement).
//
// Prérequis sur chaque machine : Node et npm, Rust, gh (GitHub CLI, `gh auth login`), et la clé :
// ~/.tauri/gram.key et ~/.tauri/gram.key.password (sous Windows, %USERPROFILE%\.tauri\), copiées
// depuis la machine qui les a créées. Lancer avant scripts/check.sh (ou scripts/check.ps1).
import { execFileSync } from 'node:child_process';
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { ManifestError, buildManifest, missingKeys, platformEntries, toJson } from './latest-json.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const { values: opts, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    'notes-file': { type: 'string' },
    'dry-run': { type: 'boolean', default: false },
  },
});
const dryRun = opts['dry-run'];

function fail(message) {
  console.error(`\n\x1b[31mrelease : ${message}\x1b[0m`);
  process.exit(1);
}

function step(title) {
  console.log(`\n\x1b[36m=== ${title} ===\x1b[0m`);
}

/** Sortie de la commande ; en cas d'échec, arrêt avec son message d'erreur. */
function run(cmd, args, options = {}) {
  try {
    return execFileSync(cmd, args, {
      cwd: root,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
      ...options,
    }).trim();
  } catch (e) {
    const detail = e.stderr?.toString().trim() || e.message;
    fail(`${cmd} ${args.join(' ')} a échoué :\n${detail}`);
  }
}

/** Sortie de la commande, ou `null` si elle échoue (tag ou release absents…). */
function tryRun(cmd, args) {
  try {
    return execFileSync(cmd, args, { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
  } catch {
    return null;
  }
}

const git = (...args) => run('git', args);

/** Commande qui modifie le dépôt ou GitHub : seulement affichée avec `--dry-run`. */
function act(cmd, args) {
  if (dryRun) {
    const shown = args.map((a) => (/[\s"]/.test(a) ? JSON.stringify(a) : a)).join(' ');
    console.log(`[dry-run] ${cmd} ${shown}`);
    return '';
  }
  return run(cmd, args);
}

/** Fichier temporaire, supprimé après `use`. */
function withTempFile(name, content, use) {
  const dir = mkdtempSync(join(tmpdir(), 'gram-release-'));
  try {
    const file = join(dir, name);
    writeFileSync(file, content);
    return use(file);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/** [X, Y, Z] d'une version X.Y.Z. */
function parse(version) {
  const m = /^(\d+)\.(\d+)\.(\d+)$/.exec(version ?? '');
  return m ? m.slice(1).map(Number) : null;
}

function compare(a, b) {
  for (let i = 0; i < 3; i++) if (a[i] !== b[i]) return a[i] - b[i];
  return 0;
}

// ── Vérifications ────────────────────────────────────────────────────────────────────────────

const os = { darwin: 'macos', win32: 'windows' }[process.platform];
if (!os) fail('les releases se construisent sous macOS ou Windows');
const osName = os === 'macos' ? 'macOS' : 'Windows';
const otherName = os === 'macos' ? 'Windows' : 'macOS';

const version = positionals[0];
if (!parse(version)) fail('usage : node scripts/release.mjs X.Y.Z [--notes-file notes.md] [--dry-run]');
const tag = `v${version}`;

step('Vérifications');

const cargoPath = join(root, 'Cargo.toml');
const cargo = readFileSync(cargoPath, 'utf8');
const versionLine = /(\[workspace\.package\][^[]*?\nversion = ")([^"]+)(")/;
const current = versionLine.exec(cargo)?.[2];
if (!parse(current)) fail('version introuvable dans [workspace.package] de Cargo.toml');

// La clé d'abord : rien n'est poussé si cette machine ne peut pas signer.
const keyDir = join(homedir(), '.tauri');
function readKeyFile(name) {
  const path = join(keyDir, name);
  return existsSync(path) ? readFileSync(path, 'utf8').trim() : undefined;
}
const signingKey = process.env.TAURI_SIGNING_PRIVATE_KEY ?? readKeyFile('gram.key');
const signingPassword = process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? readKeyFile('gram.key.password');
if (!signingKey || signingPassword === undefined) {
  fail(`clé de signature introuvable : copier gram.key et gram.key.password dans ${keyDir} depuis la machine qui les a créées`);
}
console.log(`Clé de signature : ${keyDir}`);

const remote = git('remote', 'get-url', 'origin');
const repo = /github\.com[:/](.+?)(?:\.git)?$/.exec(remote)?.[1];
if (!repo) fail(`dépôt GitHub introuvable dans l'adresse de origin : ${remote}`);

if (!dryRun) {
  if (tryRun('gh', ['auth', 'status']) === null) fail('gh (GitHub CLI) absent ou non connecté : installer gh, puis `gh auth login`');
  git('fetch', '--tags', 'origin');
}

const dirty = git('status', '--porcelain');
function requireClean() {
  if (!dirty) return;
  if (dryRun) console.log('\x1b[33mModifications non commitées (ignoré avec --dry-run)\x1b[0m');
  else fail('des modifications ne sont pas commitées');
}

/** Release GitHub du tag : brouillon ou non, fichiers déjà envoyés. `null` si absente. */
function release() {
  const json = tryRun('gh', ['release', 'view', tag, '--json', 'isDraft,assets,url']);
  return json ? JSON.parse(json) : null;
}

/** Texte de la page de la release : notes de version, puis comment installer. */
function releaseBody(notes) {
  return `${notes.trim()}

## Installation

- **Windows** : \`GRAM_${version}_x64-setup.exe\` (installation pour l'utilisateur courant, sans droits administrateur). \`GRAM_${version}_x64_fr-FR.msi\` installe pour toute la machine (droits administrateur, déploiement par GPO ou Intune). Au premier lancement, SmartScreen peut afficher « Windows a protégé votre ordinateur » : *Informations complémentaires* → *Exécuter quand même*.
- **macOS** (10.15 ou plus récent, Apple Silicon et Intel) : \`GRAM_${version}_universal.dmg\`, puis glisser GRAM dans Applications. L'application n'est pas notarisée par Apple : au premier lancement, *Réglages Système* → *Confidentialité et sécurité* → *Ouvrir quand même* (ou, dans le Terminal : \`xattr -dr com.apple.quarantine /Applications/GRAM.app\`).

Une fois installé, GRAM se met à jour tout seul : il cherche une nouvelle version au lancement, et le bouton de mise à jour de la barre d'outils lance la recherche à la demande. \`GRAM_${version}_universal.app.tar.gz\` et \`latest.json\` servent à ces mises à jour automatiques.
`;
}

function defaultNotes() {
  const previous = tryRun('git', ['describe', '--tags', '--abbrev=0', '--match', 'v[0-9]*']);
  const subjects = git('log', '--no-merges', '--pretty=format:%s', previous ? `${previous}..HEAD` : 'HEAD')
    .split('\n')
    .filter((s) => s && !/^Version \d+\.\d+\.\d+$/.test(s));
  const items = subjects.length ? subjects.map((s) => `- ${s}`) : ['- Corrections et améliorations.'];
  return `## Nouveautés\n\n${items.join('\n')}\n`;
}

// ── Version, tag et brouillon de release (première machine) ──────────────────────────────────

let notes;
if (!git('tag', '-l', tag)) {
  const branch = git('rev-parse', '--abbrev-ref', 'HEAD');
  if (branch !== 'main') fail(`les releases partent de main (branche actuelle : ${branch})`);
  requireClean();
  if (compare(parse(version), parse(current)) < 0) fail(`${version} est plus ancienne que la version actuelle (${current})`);
  if (dryRun && version !== current) fail(`--dry-run construit la version actuelle (${current})`);
  notes = opts['notes-file'] ? readFileSync(resolve(opts['notes-file']), 'utf8') : defaultNotes();
  if (!notes.trim()) fail('notes de version vides');

  step(`Version ${version}`);
  if (version !== current) {
    writeFileSync(cargoPath, cargo.replace(versionLine, `$1${version}$3`));
    const pkgPath = join(root, 'app/ui/package.json');
    writeFileSync(pkgPath, readFileSync(pkgPath, 'utf8').replace(/("version":\s*")[^"]+(")/, `$1${version}$2`));
    const lockPath = join(root, 'app/ui/package-lock.json');
    const lock = JSON.parse(readFileSync(lockPath, 'utf8'));
    lock.version = version;
    if (lock.packages?.['']) lock.packages[''].version = version;
    writeFileSync(lockPath, `${JSON.stringify(lock, null, 2)}\n`);
    // Cargo.lock : versions des crates du workspace seulement, sans réseau.
    run('cargo', ['update', '--workspace', '--offline']);
    git('add', 'Cargo.toml', 'Cargo.lock', 'app/ui/package.json', 'app/ui/package-lock.json');
    git('commit', '-m', `Version ${version}`);
    console.log(`Version ${current} → ${version}, commit « Version ${version} »`);
  } else {
    console.log(`Version ${version} déjà en place`);
  }
  // `verbatim` : les titres Markdown (« ## … ») ne sont pas pris pour des commentaires.
  withTempFile('notes.md', notes, (file) => act('git', ['tag', '-a', tag, '-F', file, '--cleanup=verbatim']));
  act('git', ['push', 'origin', 'main']);
  act('git', ['push', 'origin', tag]);
  withTempFile('release.md', releaseBody(notes), (file) =>
    act('gh', ['release', 'create', tag, '--draft', '--verify-tag', '--title', `GRAM ${version}`, '--notes-file', file]),
  );
  console.log(`Tag ${tag} et brouillon de release créés. Notes de version :\n\n${notes.trim()}`);
} else {
  // Seconde machine, ou nouvel essai : on construit exactement le tag.
  if (opts['notes-file']) console.log(`\x1b[33m--notes-file ignoré : les notes sont celles du tag ${tag}\x1b[0m`);
  if (git('rev-parse', 'HEAD') !== git('rev-list', '-n', '1', tag)) {
    fail(`le dépôt n'est pas sur ${tag} : lancer \`git pull\` (main doit être sur le tag), ou \`git checkout ${tag}\``);
  }
  requireClean();
  notes = git('tag', '-l', '--format=%(contents)', tag).replace(/-----BEGIN PGP SIGNATURE-----[\s\S]*$/, '');
  if (!dryRun && !release()) {
    withTempFile('release.md', releaseBody(notes), (file) =>
      act('gh', ['release', 'create', tag, '--draft', '--verify-tag', '--title', `GRAM ${version}`, '--notes-file', file]),
    );
  }
}

// ── Build ────────────────────────────────────────────────────────────────────────────────────

step(`Build ${os === 'macos' ? 'macOS (universel : Apple Silicon et Intel)' : 'Windows (x64)'}`);

const ui = join(root, 'app/ui');
if (!existsSync(join(ui, 'node_modules'))) {
  // npm est un script .cmd sous Windows : il passe par le shell.
  execFileSync('npm', ['ci'], { cwd: ui, stdio: 'inherit', shell: process.platform === 'win32' });
}
const tauriArgs = ['build', '--config', '../src-tauri/tauri.updater.conf.json'];
if (os === 'macos') {
  const installed = run('rustup', ['target', 'list', '--installed']).split('\n');
  const missing = ['aarch64-apple-darwin', 'x86_64-apple-darwin'].filter((t) => !installed.includes(t));
  if (missing.length) execFileSync('rustup', ['target', 'add', ...missing], { stdio: 'inherit' });
  tauriArgs.push('--target', 'universal-apple-darwin');
}
const started = Date.now();
try {
  // node directement (pas npm run) : les options passent telles quelles à la CLI Tauri.
  execFileSync(process.execPath, ['scripts/tauri.mjs', ...tauriArgs], {
    cwd: ui,
    stdio: 'inherit',
    env: {
      ...process.env,
      // macOS : le .dmg est créé sans piloter le Finder.
      CI: 'true',
      TAURI_SIGNING_PRIVATE_KEY: signingKey,
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD: signingPassword,
    },
  });
} catch {
  fail('le build a échoué (voir ci-dessus)');
}

// Paquets sous leur nom dans la release ; ceux d'un build précédent sont refusés.
const bundle = join(root, os === 'macos' ? 'target/universal-apple-darwin/release/bundle' : 'target/release/bundle');
const packages =
  os === 'macos'
    ? [
        ['dmg', `GRAM_${version}_universal.dmg`, `GRAM_${version}_universal.dmg`],
        ['macos', 'GRAM.app.tar.gz', `GRAM_${version}_universal.app.tar.gz`],
        ['macos', 'GRAM.app.tar.gz.sig', `GRAM_${version}_universal.app.tar.gz.sig`],
      ]
    : [
        ['nsis', `GRAM_${version}_x64-setup.exe`],
        ['nsis', `GRAM_${version}_x64-setup.exe.sig`],
        ['msi', `GRAM_${version}_x64_fr-FR.msi`],
        ['msi', `GRAM_${version}_x64_fr-FR.msi.sig`],
      ];
const out = join(root, 'target', 'release-assets', tag, os);
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
for (const [dir, name, published = name] of packages) {
  const src = join(bundle, dir, name);
  if (!existsSync(src) || statSync(src).mtimeMs < started - 1000) fail(`paquet introuvable ou ancien : ${src}`);
  copyFileSync(src, join(out, published));
}

// ── Release GitHub ───────────────────────────────────────────────────────────────────────────

step('Release GitHub');

const existing = dryRun ? null : release();
let previous;
if (existing?.assets.some((a) => a.name === 'latest.json')) {
  const dir = mkdtempSync(join(tmpdir(), 'gram-release-'));
  try {
    run('gh', ['release', 'download', tag, '--pattern', 'latest.json', '--dir', dir, '--clobber']);
    previous = JSON.parse(readFileSync(join(dir, 'latest.json'), 'utf8'));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

let manifest;
try {
  manifest = buildManifest({
    version,
    notes,
    platforms: platformEntries(out, `https://github.com/${repo}/releases/download/${tag}`),
    previous,
  });
} catch (e) {
  if (e instanceof ManifestError) fail(e.message);
  throw e;
}
writeFileSync(join(out, 'latest.json'), toJson(manifest));

const uploads = readdirSync(out)
  .filter((f) => !f.endsWith('.sig'))
  .map((f) => join(out, f));
act('gh', ['release', 'upload', tag, ...uploads, '--clobber']);
console.log(`Envoyés : ${uploads.map((f) => f.slice(out.length + 1)).join(', ')}`);

const missing = missingKeys(manifest);
if (missing.length) {
  console.log(`\n\x1b[33mPaquets ${osName} en place. Il manque ${otherName} : sur l'autre machine, \`git pull\` puis la même commande (node scripts/release.mjs ${version}).\x1b[0m`);
} else if (dryRun || existing?.isDraft) {
  act('gh', ['release', 'edit', tag, '--draft=false', '--latest']);
  console.log(`\n\x1b[32mRelease ${tag} publiée : https://github.com/${repo}/releases/tag/${tag}\x1b[0m`);
} else {
  console.log(`\n\x1b[32mPaquets ${osName} remplacés dans la release ${tag}, déjà publiée.\x1b[0m`);
}
if (dryRun) console.log(`\n--dry-run : paquets et latest.json dans ${out}`);
