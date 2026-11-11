#!/usr/bin/env node
// Manifeste de mise à jour `latest.json` d'une release GitHub, lu par le plugin updater de Tauri
// (app/src-tauri/src/commands/update.rs). Utilisé par scripts/release.mjs, et en ligne de commande
// pour les fixtures des tests :
//
//   node scripts/latest-json.mjs --dir <paquets> --version 0.2.0 \
//     --base-url https://github.com/<owner>/<repo>/releases/download/v0.2.0 \
//     [--merge latest.json] [--notes-file notes.md] [--pub-date 2026-11-11T12:00:00Z] [--out latest.json]
//
// `--dir` contient des paquets de mise à jour, sous le nom qu'ils ont dans la release, et leur
// signature (`.sig`, produite par `tauri build` avec `createUpdaterArtifacts`) :
//
//   *.app.tar.gz   macOS, binaire universel (Apple Silicon et Intel)
//   *-setup.exe    Windows, installeur NSIS
//   *.msi          Windows, installeur MSI
//
// Chaque machine ne construit que les paquets de son système : `--merge` reprend les plateformes
// d'un manifeste existant (même version) et y ajoute ou remplace celles des paquets présents.
//
// Le plugin cherche d'abord `{os}-{arch}-{type d'installeur}`, puis `{os}-{arch}` : une
// installation par le MSI se met à jour avec le MSI, les autres avec l'installeur NSIS.
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

/** Paquets de mise à jour de chaque système, et entrées du manifeste qu'ils servent. */
export const PACKAGES = {
  macos: [{ suffix: '.app.tar.gz', keys: ['darwin-aarch64', 'darwin-x86_64'] }],
  windows: [
    { suffix: '-setup.exe', keys: ['windows-x86_64', 'windows-x86_64-nsis'] },
    { suffix: '.msi', keys: ['windows-x86_64-msi'] },
  ],
};

/** Toutes les entrées d'une release complète. */
export const ALL_KEYS = Object.values(PACKAGES).flatMap((list) => list.flatMap((p) => p.keys));

export class ManifestError extends Error {}

/**
 * Entrées du manifeste pour les paquets présents dans `dir` (un seul par type, avec sa
 * signature). Erreur si aucun paquet n'y est.
 */
export function platformEntries(dir, baseUrl) {
  const files = readdirSync(dir);
  const base = baseUrl.replace(/\/+$/, '');
  const platforms = {};
  for (const pkg of Object.values(PACKAGES).flat()) {
    const found = files.filter((f) => f.endsWith(pkg.suffix));
    if (found.length === 0) continue;
    if (found.length > 1) throw new ManifestError(`plusieurs paquets *${pkg.suffix} dans ${dir} : ${found.join(', ')}`);
    const [file] = found;
    if (!files.includes(`${file}.sig`)) throw new ManifestError(`signature ${file}.sig manquante`);
    const entry = {
      signature: readFileSync(join(dir, `${file}.sig`), 'utf8').trim(),
      url: `${base}/${encodeURIComponent(file)}`,
    };
    for (const key of pkg.keys) platforms[key] = entry;
  }
  if (Object.keys(platforms).length === 0) throw new ManifestError(`aucun paquet de mise à jour dans ${dir}`);
  return platforms;
}

/**
 * Manifeste de la version `version` : plateformes de `previous` (s'il porte la même version),
 * complétées ou remplacées par `platforms`.
 */
export function buildManifest({ version, notes, pubDate, platforms, previous }) {
  if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
    throw new ManifestError(`version invalide : ${version} (attendu : X.Y.Z)`);
  }
  if (previous && previous.version !== version) {
    throw new ManifestError(`le manifeste existant est celui de la version ${previous.version}, pas ${version}`);
  }
  const merged = { ...previous?.platforms, ...platforms };
  return {
    version,
    notes: notes?.trim() || `GRAM ${version}`,
    pub_date: pubDate ?? new Date().toISOString().replace(/\.\d{3}Z$/, 'Z'),
    // Ordre fixe : le fichier reste lisible et stable d'une fusion à l'autre.
    platforms: Object.fromEntries(ALL_KEYS.filter((k) => merged[k]).map((k) => [k, merged[k]])),
  };
}

/** Entrées qui manquent encore pour que la release serve tous les systèmes. */
export function missingKeys(manifest) {
  return ALL_KEYS.filter((k) => !manifest.platforms[k]);
}

export function toJson(manifest) {
  return `${JSON.stringify(manifest, null, 2)}\n`;
}

// ── Ligne de commande ────────────────────────────────────────────────────────────────────────

function main() {
  const { values: args } = parseArgs({
    options: {
      dir: { type: 'string' },
      version: { type: 'string' },
      'base-url': { type: 'string' },
      merge: { type: 'string' },
      'notes-file': { type: 'string' },
      'pub-date': { type: 'string' },
      out: { type: 'string' },
    },
  });
  for (const name of ['dir', 'version', 'base-url']) {
    if (!args[name]) throw new ManifestError(`--${name} manquant`);
  }
  const manifest = buildManifest({
    version: args.version,
    notes: args['notes-file'] ? readFileSync(args['notes-file'], 'utf8') : undefined,
    pubDate: args['pub-date'],
    platforms: platformEntries(args.dir, args['base-url']),
    previous: args.merge ? JSON.parse(readFileSync(args.merge, 'utf8')) : undefined,
  });
  if (args.out) writeFileSync(args.out, toJson(manifest));
  else process.stdout.write(toJson(manifest));
}

if (resolve(process.argv[1] ?? '') === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (e) {
    console.error(`latest-json : ${e instanceof Error ? e.message : e}`);
    process.exit(1);
  }
}
