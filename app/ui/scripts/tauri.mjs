// Lance la CLI Tauri depuis app/ui en lui indiquant où se trouve l'application (variable
// TAURI_APP_PATH) : GRAM (app/src-tauri) par défaut, GRAM Harness (app/harness-tauri) avec
// --harness, que `npm run harness <commande>` ajoute. La CLI cherche tauri.conf.json en
// descendant depuis le dossier courant : sans TAURI_APP_PATH, elle ne trouverait pas les
// dossiers voisins.
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const flag = process.argv.indexOf('--harness', 2);
if (flag >= 0) {
  process.argv.splice(flag, 1);
  process.env.TAURI_APP_PATH = resolve(here, '../../harness-tauri');
} else {
  process.env.TAURI_APP_PATH ??= resolve(here, '../../src-tauri');
}

createRequire(import.meta.url)('@tauri-apps/cli/tauri.js');
