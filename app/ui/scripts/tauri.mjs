// Lance la CLI Tauri depuis app/ui en lui indiquant où se trouve app/src-tauri.
// La CLI cherche tauri.conf.json en descendant depuis le dossier courant : sans
// TAURI_APP_PATH, elle ne trouverait pas le dossier voisin ../src-tauri.
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
process.env.TAURI_APP_PATH ??= resolve(here, '../../src-tauri');

createRequire(import.meta.url)('@tauri-apps/cli/tauri.js');
