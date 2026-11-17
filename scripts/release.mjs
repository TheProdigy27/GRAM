#!/usr/bin/env node
// Release de GRAM et de GRAM Harness (même version, même tag, même release GitHub), construite
// sur tes machines : la clé de signature des mises à jour ne quitte pas ~/.tauri. La même
// commande sur le Mac puis sur le PC Windows (ou l'inverse) :
//
//   node scripts/release.mjs 0.2.0 [--notes-file notes.md] [--dry-run]
//
// - Première machine (le tag v0.2.0 n'existe pas encore) : version dans Cargo.toml et
//   app/ui/package.json, commit « Version 0.2.0 », tag annoté dont le message sert de notes de
//   version (par défaut, les commits depuis le tag précédent), push de main et du tag, brouillon
//   de release GitHub.
// - Chaque machine : build des deux applications pour son système (macOS universel, ou Windows
//   x64 NSIS et MSI), paquets de mise à jour signés avec ~/.tauri/gram.key, envoyés dans la
//   release avec les manifestes complétés (scripts/latest-json.mjs) : latest.json pour GRAM,
//   latest-harness.json pour GRAM Harness.
// - Quand macOS et Windows y sont, la release est publiée : les applications installées la
//   proposent d'elles-mêmes.
//
// Seconde machine : `git pull` avant (main doit être sur le tag). `--dry-run` : build et paquets
// dans target/release-assets/vX.Y.Z, sans commit, tag, push ni GitHub (version actuelle
// seulement, sur n'importe quelle branche).
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

/**
 * Applications de la release. `name` : productName de tauri.conf.json, qui nomme les paquets
 * construits ; `file` : leur nom dans la release (GitHub remplacerait l'espace par un point) ;
 * `manifest` : manifeste de mise à jour, lu à …/releases/latest/download/<manifest> ;
 * `scheme` : schéma du lien profond, que les installeurs doivent enregistrer.
 */
const APPS = [
  { name: 'GRAM', file: 'GRAM', tauri: 'src-tauri', cli: [], manifest: 'latest.json' },
  {
    name: 'GRAM Harness',
    file: 'GRAM-Harness',
    tauri: 'harness-tauri',
    cli: ['--harness'],
    manifest: 'latest-harness.json',
    scheme: 'gram-harness',
  },
];

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

### GRAM Harness

L'application de discussion pour les modèles ajoutés depuis GRAM (bouton « Ajouter à GRAM Harness » de la fiche d'un modèle), avec [Ollama](https://ollama.com/download) : \`GRAM-Harness_${version}_x64-setup.exe\` ou \`GRAM-Harness_${version}_x64_fr-FR.msi\` sous Windows, \`GRAM-Harness_${version}_universal.dmg\` sous macOS, avec les mêmes remarques (sous macOS : \`xattr -dr com.apple.quarantine "/Applications/GRAM Harness.app"\`). Une fois installé, « Ouvrir dans GRAM Harness » passe de la fiche du modèle à la discussion.

Une fois installés, GRAM et GRAM Harness se mettent à jour tout seuls : ils cherchent une nouvelle version au lancement, et le bouton de mise à jour lance la recherche à la demande. Les fichiers \`*.app.tar.gz\`, \`latest.json\` et \`latest-harness.json\` servent à ces mises à jour automatiques.
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
    if (dryRun) console.log(`\x1b[33mLe dépôt n'est pas sur ${tag} (ignoré avec --dry-run : build de l'état actuel)\x1b[0m`);
    else fail(`le dépôt n'est pas sur ${tag} : lancer \`git pull\` (main doit être sur le tag), ou \`git checkout ${tag}\``);
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

const ui = join(root, 'app/ui');
if (!existsSync(join(ui, 'node_modules'))) {
  // npm est un script .cmd sous Windows : il passe par le shell.
  execFileSync('npm', ['ci'], { cwd: ui, stdio: 'inherit', shell: process.platform === 'win32' });
}
if (os === 'macos') {
  const installed = run('rustup', ['target', 'list', '--installed']).split('\n');
  const missing = ['aarch64-apple-darwin', 'x86_64-apple-darwin'].filter((t) => !installed.includes(t));
  if (missing.length) execFileSync('rustup', ['target', 'add', ...missing], { stdio: 'inherit' });
}

/** Paquets d'une application : [dossier du bundle, nom construit, nom dans la release]. */
function packagesOf({ name, file }) {
  return os === 'macos'
    ? [
        ['dmg', `${name}_${version}_universal.dmg`, `${file}_${version}_universal.dmg`],
        ['macos', `${name}.app.tar.gz`, `${file}_${version}_universal.app.tar.gz`],
        ['macos', `${name}.app.tar.gz.sig`, `${file}_${version}_universal.app.tar.gz.sig`],
      ]
    : [
        ['nsis', `${name}_${version}_x64-setup.exe`, `${file}_${version}_x64-setup.exe`],
        ['nsis', `${name}_${version}_x64-setup.exe.sig`, `${file}_${version}_x64-setup.exe.sig`],
        ['msi', `${name}_${version}_x64_fr-FR.msi`, `${file}_${version}_x64_fr-FR.msi`],
        ['msi', `${name}_${version}_x64_fr-FR.msi.sig`, `${file}_${version}_x64_fr-FR.msi.sig`],
      ];
}

// Les deux applications partagent le dossier target du workspace.
const bundle = join(root, os === 'macos' ? 'target/universal-apple-darwin/release/bundle' : 'target/release/bundle');
const out = join(root, 'target', 'release-assets', tag, os);
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });

for (const app of APPS) {
  step(`Build de ${app.name} : ${os === 'macos' ? 'macOS (universel : Apple Silicon et Intel)' : 'Windows (x64)'}`);
  const tauriArgs = [...app.cli, 'build', '--config', `../${app.tauri}/tauri.updater.conf.json`];
  if (os === 'macos') tauriArgs.push('--target', 'universal-apple-darwin');
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
    fail(`le build de ${app.name} a échoué (voir ci-dessus)`);
  }

  // Paquets sous leur nom dans la release ; ceux d'un build précédent sont refusés.
  for (const [dir, name, published] of packagesOf(app)) {
    const src = join(bundle, dir, name);
    if (!existsSync(src) || statSync(src).mtimeMs < started - 1000) fail(`paquet introuvable ou ancien : ${src}`);
    copyFileSync(src, join(out, published));
  }
  if (app.scheme) checkScheme(app);
}

/**
 * Le schéma du lien profond (`plugins.deep-link` de tauri.conf.json) est déclaré par la CLI
 * Tauri dans ce qu'elle génère : Info.plist de l'application (macOS), scripts NSIS et WiX
 * (Windows : clé `Software\Classes\<schéma>`).
 */
function checkScheme({ name, scheme }) {
  const sources =
    os === 'macos'
      ? [[join(bundle, 'macos', `${name}.app`, 'Contents', 'Info.plist'), `<string>${scheme}</string>`]]
      : [
          [join(root, 'target/release/nsis/x64/installer.nsi'), `Software\\Classes\\${scheme}`],
          [join(root, 'target/release/wix/x64/main.wxs'), `Software\\Classes\\${scheme}`],
        ];
  for (const [file, expected] of sources) {
    if (!existsSync(file) || !readFileSync(file, 'utf8').includes(expected)) {
      fail(`schéma ${scheme} non déclaré dans ${file} : vérifier plugins.deep-link dans tauri.conf.json`);
    }
  }
  console.log(`Schéma ${scheme} déclaré par les installeurs de ${name}`);
}

// ── Release GitHub ───────────────────────────────────────────────────────────────────────────

step('Release GitHub');

const existing = dryRun ? null : release();
/** Manifeste déjà envoyé par l'autre machine, complété par les paquets de celle-ci. */
function previousManifest(name) {
  if (!existing?.assets.some((a) => a.name === name)) return undefined;
  const dir = mkdtempSync(join(tmpdir(), 'gram-release-'));
  try {
    run('gh', ['release', 'download', tag, '--pattern', name, '--dir', dir, '--clobber']);
    return JSON.parse(readFileSync(join(dir, name), 'utf8'));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const missing = [];
for (const app of APPS) {
  let manifest;
  try {
    manifest = buildManifest({
      version,
      notes,
      appName: app.name,
      platforms: platformEntries(out, `https://github.com/${repo}/releases/download/${tag}`, `${app.file}_`),
      previous: previousManifest(app.manifest),
    });
  } catch (e) {
    if (e instanceof ManifestError) fail(`${app.manifest} : ${e.message}`);
    throw e;
  }
  writeFileSync(join(out, app.manifest), toJson(manifest));
  missing.push(...missingKeys(manifest));
}

const uploads = readdirSync(out)
  .filter((f) => !f.endsWith('.sig'))
  .map((f) => join(out, f));
act('gh', ['release', 'upload', tag, ...uploads, '--clobber']);
console.log(`Envoyés : ${uploads.map((f) => f.slice(out.length + 1)).join(', ')}`);

if (missing.length) {
  console.log(`\n\x1b[33mPaquets ${osName} en place. Il manque ${otherName} : sur l'autre machine, \`git pull\` puis la même commande (node scripts/release.mjs ${version}).\x1b[0m`);
} else if (dryRun || existing?.isDraft) {
  act('gh', ['release', 'edit', tag, '--draft=false', '--latest']);
  console.log(`\n\x1b[32mRelease ${tag} publiée : https://github.com/${repo}/releases/tag/${tag}\x1b[0m`);
} else {
  console.log(`\n\x1b[32mPaquets ${osName} remplacés dans la release ${tag}, déjà publiée.\x1b[0m`);
}
if (dryRun) console.log(`\n--dry-run : paquets et manifestes dans ${out}`);
