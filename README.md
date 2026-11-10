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
npm run tauri build     # installeurs NSIS et MSI (voir « Installeur »)
npm run tauri icon public/logo.svg   # régénère les icônes depuis le logo

cd ../..
cargo test --workspace  # tests Rust ; régénère aussi les types TypeScript
cargo run -p gram-cli -- --help
cargo run -p gram-cli -- hw          # matériel détecté (à comparer au Gestionnaire des tâches)
cargo run -p gram-cli -- hw --json   # { hardware, runtimes }
cargo run -p gram-cli -- hw --raw    # données brutes, à enregistrer dans tests/fixtures/hardware/
cargo run -p gram-cli -- sync        # catalogue Hugging Face (cache de 24 h ; --force pour resynchroniser)
cargo run -p gram-cli -- sync --offline   # catalogue du cache, sans réseau
cargo run -p gram-cli -- snapshot    # régénère data/seed_catalog.json (catalogue embarqué)
cargo run -p gram-cli -- estimate Qwen3-8B             # chaque quantification estimée pour cette machine
cargo run -p gram-cli -- estimate Qwen3-8B --ctx 32768 --kv q8_0 --vision
cargo run -p gram-cli -- estimate Llama-3.3-70B --profile tests/fixtures/profiles/p3-rtx4090-24go-64go-ddr5.json
cargo run -p gram-cli -- rank                          # classement polyvalent (contexte 8k) pour cette machine
cargo run -p gram-cli -- rank --usage code --ctx 16384 --json
cargo run -p gram-cli -- rank --usage vision --runtime llama-cpp --max-size-gb 12 --show-too-big --limit 0
cargo run -p gram-cli -- rank --profile tests/fixtures/profiles/p1-portable-igpu-16go.json --usage fast-chat

powershell -ExecutionPolicy Bypass -File scripts/check.ps1   # CI locale complète
powershell -ExecutionPolicy Bypass -File scripts/test-installer.ps1   # installeur testé dans Windows Sandbox
```

## Structure

| Dossier | Rôle |
|---|---|
| `crates/gram-core` | Types, moteur d'estimation, scoring (pur, sans I/O) |
| `crates/gram-hw` | Détection matériel et runtimes installés (Windows) |
| `crates/gram-hub` | Client Hugging Face, cache, parseur GGUF, dédoublonnage, catalogue embarqué |
| `crates/gram-cli` | Binaire de debug : `hw`, `sync`, `estimate`, `rank`, `snapshot` |
| `app/src-tauri` | Application Tauri : commandes IPC, état, événements, icônes (`icons/`, générées) |
| `app/ui` | Frontend Angular + Angular Material : `core/` (service Tauri typé, types générés), `state/` (stores à base de signals), `features/` (bandeau matériel et dialogue « Corriger… », filtres, classement, fiche détail, paramètres, barre d'état), `shared/` (mise en forme française, libellés, badge de mode), `src/testing/` (fixtures typées et faux backend pour les tests), `public/logo.svg` (logo, source des icônes) |
| `data` | Tables embarquées : `gpu_specs.json` (bande passante des GPU, liste du mode simulation), `uploaders.json` (priorité des uploaders GGUF), `quant_quality.json` (bits par poids typiques pour les modèles pas encore détaillés, et facteur de qualité selon les bits par poids), `quality_overrides.json` (bonus de qualité manuels par famille, vide au départ), `seed_catalog.json` (catalogue de démarrage, généré) |
| `scripts` | `check.ps1` (CI locale), `test-installer.ps1` et `sandbox/` (installeur testé sur une Windows vierge) |
| `tests/fixtures` | Données pour les tests : dumps matériels `gram-cli hw --raw`, réponses Hugging Face, profils matériels types (`profiles/`), modèles réels du catalogue (`models/`) et échantillon figé de 35 modèles pour le classement (`catalog/`) |

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

- Cache : `%LOCALAPPDATA%\fr.gram.desktop\cache\` (`catalog.json`, `details/` et `arch/` indexés par commit, donc gardés indéfiniment).
- Hors ligne, ou si Hugging Face est injoignable : cache, puis catalogue embarqué (`data/seed_catalog.json`).
- Token optionnel (quotas plus larges) : variable `HF_TOKEN` pour `gram-cli`. Il n'est jamais affiché ni écrit.
- Tests : réponses réelles enregistrées dans `tests/fixtures/hf/` (voir son README), aucun appel réseau.

## Estimation

`gram-core::estimate` calcule, pour un modèle, une quantification et une machine, la mémoire nécessaire, le mode d'exécution, le contexte maximal et la vitesse de génération (§8 du plan). C'est du calcul pur, sans I/O.

1. **Mémoire utilisable** (`MemoryBudget::from_profile`) : `min(VRAM × 90 %, VRAM libre + 256 Mio)` sur le GPU retenu, et `min(RAM disponible, RAM × 75 %)`. Le réglage « je ferme tout » passe à `RAM − 4 Go`. Un iGPU compte comme du CPU.
2. **Besoin** : poids (taille réelle des fichiers) + cache KV (`2 × couches d'attention × têtes KV × head_dim × octets`, f16 / q8_0 / q4_0) + 0,5 Go de surcoût (+ le projecteur vision si l'usage vision est demandé).
3. **Mode** : GPU si tout tient en VRAM ; sinon hybride (KV et surcoût en VRAM, puis des couches entières : le `-ngl` de llama.cpp) ; sinon CPU. Si rien ne tient, le contexte est divisé par deux jusqu'à 2 048 tokens, puis le modèle est déclaré trop gros.
4. **Vitesse** : la génération est limitée par la bande passante, plus un temps fixe par token. `temps = (octets GPU / BW GPU + octets RAM / BW RAM) / 0,55 + 1,5 ms` et `tok/s = 1 / temps`. Un MoE ne lit que ses paramètres actifs. Le temps fixe (lancement des calculs, routage des experts) compte surtout quand un token lit peu d'octets : sans lui, un MoE à 3B actifs ou un modèle de 0,6B serait surestimé d'un facteur 1,5 à 2 sur GPU. Les valeurs 0,55 et 1,5 ms sont prudentes, tirées de vitesses publiées ; la phase 7 les calibrera sur la machine.
5. **Adéquation** : parfait (GPU, ≤ 80 % de la VRAM utilisable), bon (GPU ≤ 95 %, ou au moins 75 % des couches sur le GPU), limite (autre hybride, CPU), trop juste.

Les quantifications sous 2,5 bits par poids ne sont pas proposées. Sans tailles réelles (modèle pas encore détaillé), l'estimation part de l'échelle Q8_0 → Q2_K de `data/quant_quality.json`. Sans architecture, elle prend un cache KV prudent. Ces deux cas sont marqués « estimation approximative ».

Limites connues (prévues pour plus tard) : l'attention à fenêtre glissante (Gemma, gpt-oss) n'est pas modélisée, donc le KV est surestimé (c'est signalé). Le traitement du prompt n'est pas estimé. La précision visée est de ±30 %.

Profils types du §15 (`tests/fixtures/profiles/`), testés dans `crates/gram-core/tests/profiles.rs` avec des modèles réels du catalogue, contexte 8k :

| Profil | Qwen3-8B Q4_K_M (4,7 Go) | Llama-3.3-70B Q4_K_M (39,6 Go) |
|---|---|---|
| P1 : portable iGPU, 16 Go DDR4-3200 | CPU, ~5,6 tok/s | trop gros |
| P2 : RTX 3060 12 Go + 32 Go DDR4-3600 | GPU, ~37 tok/s, parfait | trop gros |
| P3 : RTX 4090 24 Go + 64 Go DDR5-6000 | GPU, ~95 tok/s, parfait | hybride 37/80 couches, ~2,1 tok/s |
| P4 : RX 7800 XT 16 Go + 32 Go DDR5-6000 | GPU, ~62 tok/s, parfait | trop gros |

## Classement

`gram-core::rank` classe les modèles du catalogue pour la machine (§9 du plan). C'est du calcul pur : environ 15 ms pour 1 165 modèles en build debug.

1. **Quatre axes de 0 à 100** pour chaque quantification candidate :
   - qualité : `f_taille × f_quant + récence + curation`, avec `f_taille = 40 + 12 × log2(milliards de paramètres)` (`√(total × actifs)` pour un MoE) et `f_quant` interpolé sur les bits par poids réels (`data/quant_quality.json`) ; +5 sous 6 mois, −10 au-delà de 18 mois ;
   - vitesse : `100 × ln(tok/s ÷ 2) / ln(20)` (2 tok/s → 0, 40 tok/s → 100) ;
   - marge : 100 jusqu'à 80 % de la mémoire du mode, puis 60 à 95 %, puis 40 ;
   - contexte : part du contexte souhaité atteignable.
2. **Score** : `(moyenne pondérée × 0,95 + popularité) × facteur d'usage × confiance`, toujours entre 0 et 100. Les poids dépendent du cas d'usage (polyvalent 0,45 / 0,30 / 0,15 / 0,10, etc., modifiables dans les paramètres). La popularité vaut de 0 à 5 points selon `log10(téléchargements)`. Le facteur d'usage vaut × 0,92 pour un modèle qui ne correspond pas au cas d'usage Code ou Raisonnement. La confiance vaut × 0,95 pour une estimation approximative (tailles ou architecture inconnues).
3. **Meilleure quantification** : le score maximal parmi celles qui tiennent (et sous la taille maximale demandée), en préférant celles qui atteignent le seuil de lenteur du cas d'usage. À score égal, la plus petite l'emporte.
4. **Sections** : exécutables, « Exécutable mais lent » (aucune quantification au-dessus du seuil : 5 tok/s, 8 pour le raisonnement, 10 pour le chat rapide), trop gros (du plus proche de tenir au plus éloigné). Le filtre Vision ne garde que les modèles vision. Un modèle dont toutes les quantifications sont sous 2,5 bits par poids n'est pas proposé.
5. **Explication** générée pour chaque ligne, par exemple « Tient entièrement en VRAM (9,9 / 14,3 Go) en Q8_0 · ~51 tok/s estimés · contexte 158k possible ».

Pourquoi ce calcul plutôt qu'une somme simple : avec des bonus ajoutés puis un écrêtage à 100, des dizaines de modèles finissaient ex æquo à 100 et les téléchargements décidaient seuls. Sans le facteur de confiance, des fine-tunes jamais détaillés (taille estimée, « Q2_K* ») occupaient la moitié du haut du classement.

La fiche détail (`model_detail`) évalue de la même façon toutes les quantifications d'un modèle et génère les commandes du runtime choisi (§10.3) : `ollama run hf.co/{dépôt}:{QUANT}` puis `/set parameter num_ctx`, `llama-server -hf … -c … -ngl …` (999 si tout tient en VRAM), ou la page Hugging Face pour LM Studio. Les règles propres aux runtimes (contexte par défaut d'Ollama, GPU intégré, cache KV quantifié, dépôt protégé, fichier découpé) sont toutes dans `crates/gram-core/src/launch.rs`.

Le snapshot du classement (`crates/gram-core/tests/snapshots/`) porte sur un échantillon figé de 35 modèles réels et les 4 profils types. Il a été relu à la main. Après un changement volontaire du scoring, relire les différences avec `cargo insta review`, ou `INSTA_UPDATE=always cargo test -p gram-core` puis `git diff`.

## Application : commandes IPC

Toute la logique est en Rust (`app/src-tauri`). Au démarrage, le matériel, les runtimes et le catalogue (cache, sinon catalogue embarqué) sont chargés en parallèle. Le classement est recalculé à chaque appel de `rank`.

| Commande | Arguments | Résultat |
|---|---|---|
| `get_hardware` | – | `HardwareProfile` (détecté une fois, puis gardé en mémoire) |
| `rescan_hardware` | – | `HardwareProfile` (corrections conservées) |
| `set_hardware_overrides` | `{ overrides: Overrides }` | `HardwareProfile`. Corrections et simulation (`simulatedGpu`, `simulatedRamTotal`), utilisées par les classements suivants ; `{}` revient aux valeurs détectées. Les corrections sont enregistrées dans `corrections.json` et reprises au lancement, pas la simulation |
| `hardware_options` | – | `HardwareOptions` : le matériel tel que détecté (sans corrections) et les GPU simulables de `data/gpu_specs.json` (dialogue « Corriger… ») |
| `detect_runtimes` | – | `RuntimesStatus` |
| `sync_catalog` | `{ force: boolean }` | `SyncSummary`, plus les événements `sync-progress`, `rate-limit` et `sync-done` |
| `rank` | `{ request: RankRequest }` | `RankResponse` (`useCase`, `ctx`, `runtime`, `maxSize` en octets, `showTooBig`, `limit` ; tout est optionnel) |
| `get_model_detail` | `{ request: DetailRequest }` | `ModelDetail`. À la première ouverture, les quantifications de tous les dépôts du modèle sont chargées (cache, sinon réseau) |
| `get_settings` / `save_settings` | – / `{ settings: Settings }` | `Settings`, enregistrés dans `%LOCALAPPDATA%\fr.gram.desktop\settings.json` et ramenés dans leurs bornes |
| `get_default_settings` | – | `Settings` du §12 (bouton « Valeurs par défaut », rappels « défaut : … ») |
| `set_hf_token` / `clear_hf_token` / `has_hf_token` | `{ token }` / – / – | `null` / `null` / `boolean`. Le token est rangé dans le Gestionnaire d'identifiants Windows et n'est jamais renvoyé |
| `get_cache_info` / `clear_cache` | – | `CacheInfo { path, files, bytes }`. Vider le cache est refusé pendant une synchronisation ; le catalogue en mémoire reste utilisé |
| `export_ranking` | `{ format: 'md' \| 'csv', path? }` | Chemin écrit, ou `null` si l'utilisateur annule. Exporte le dernier classement en entier. Sans `path`, le backend ouvre le dialogue « Enregistrer sous » (plugin dialog appelé depuis Rust : aucune permission côté WebView) ; avec `path`, chemin absolu avec l'extension du format. CSV : `;`, virgule décimale, UTF-8 avec BOM (Excel en français) |

Événements : `hardware-ready` (`HardwareProfile`), `sync-progress` (`SyncProgress`), `sync-done` (`SyncSummary`), `rate-limit` (`RateLimitInfo`). Les erreurs rejettent la promesse avec un message en français.

Différences avec l'esquisse du §5.3 : la simulation passe par `set_hardware_overrides` (déjà gérée par `Overrides`) plutôt que par un drapeau de `RankRequest`, ainsi ce qui est affiché est toujours ce qui est classé. `get_model_detail` reçoit le cas d'usage et le contexte, dont dépendent les estimations. `export_ranking` ouvre lui-même le dialogue d'enregistrement : le WebView n'a pas à choisir un chemin d'écriture.

Pour essayer depuis la console du WebView (`npm run tauri dev`, clic droit → Inspecter) :

```js
const invoke = window.__TAURI_INTERNALS__.invoke;
await invoke('get_hardware');
await invoke('rank', { request: { useCase: 'code', ctx: 16384, limit: 5 } });
await invoke('get_model_detail', { request: { repoId: 'Qwen/Qwen3-8B-GGUF', useCase: 'general' } });
await invoke('set_hardware_overrides', { overrides: { simulatedGpu: 'NVIDIA GeForce RTX 4090 24 Go' } });
```

Les tests de `app/src-tauri/src/tests.rs` appellent ces commandes de bout en bout avec le runtime simulé de Tauri (sans réseau, sans détection réelle, sans toucher au trousseau). Sous Windows, le manifeste Common Controls v6 est embarqué par l'éditeur de liens dans tous les exécutables (`build.rs`) : sans lui, un binaire de test qui construit une `App` Tauri ne démarre pas (`STATUS_ENTRYPOINT_NOT_FOUND`).

## Interface

L'interface Angular (`app/ui`) affiche les données et envoie des filtres ; elle ne calcule rien et ne fait aucun appel réseau. `TauriService` est le seul point d'accès à `invoke`, `listen` et aux deux plugins utilisés : presse-papiers (bouton « Copier ») et ouverture d'URL, limitée à `https://huggingface.co/*` dans `capabilities/default.json`.

Disposition (fenêtre 1280×800) :

- **Barre d'outils** : « Ré-analyser » (`rescan_hardware` + `detect_runtimes`), « Actualiser les modèles » (`sync_catalog` forcé), « Exporter » (Markdown ou CSV, le classement affiché avec ses filtres et toutes ses lignes) et « Paramètres ».
- **Bandeau matériel** : CPU, RAM, GPU, disques, runtimes installés, mémoire réellement utilisable. L'origine d'une valeur est affichée quand ce n'est pas une mesure directe (« estimée », « table des GPU », « corrigée »). Les GPU non retenus sont signalés sur la ligne GPU. Bouton « Corriger… » ; pendant une simulation, étiquette « Simulation » (le détail en infobulle) et bouton « Arrêter », qui garde les corrections.
- **Dialogue « Corriger… »** (§6.6) : corrections (canaux mémoire, GPU retenu s'il y en a plusieurs, GPU intégré oui/non, bande passante du GPU), chacune avec la valeur détectée en rappel, et simulation (un GPU de la table, regroupée par constructeur ; une quantité de RAM de 8 à 256 Go). « Valeurs détectées » efface tout.
- **Dialogue « Paramètres »** (§11.2, §12), en quatre onglets : estimation (marge VRAM, part de RAM, « je ferme tout », surcoût, cache KV, efficacité, temps fixe par token, bits par poids et contexte minimum), cas d'usage (contexte par défaut, seuil « lent » et poids du score de chacun), catalogue (téléchargements minimum, durée de vie du cache, modèles détaillés à la synchronisation, lignes affichées, taille et vidage du cache), Hugging Face (token). Les réglages sont modifiés sur une copie et enregistrés avec « Enregistrer », qui recalcule le classement et la fiche ; « Valeurs par défaut » reprend celles du §12. Le champ du token est masqué, vidé après l'enregistrement, et seul l'état « configuré / non configuré » est affiché.
- **Filtres** : cas d'usage (il reprend aussi son contexte par défaut), curseur de contexte de 2k à 128k (une puissance de deux par cran), runtime (enregistré dans les paramètres), taille maximale en Go, affichage des modèles trop gros.
- **Classement** : remarques du runtime, tableau `MatTable` triable (le rang « # » reste celui du score), badge de mode coloré, détail du score en infobulle, « Voir plus » (25 lignes par page, paramètre `pageSize`), sections repliables « Exécutable mais lent » et « Trop gros ».
- **Fiche détail** (panneau à droite, sans fond bloquant : un clic sur une autre ligne change de modèle) : barre mémoire empilée [poids | KV | vision | surcoût] face à la mémoire du mode, avec un repère à la fin de la VRAM en hybride ; toutes les quantifications (vert : tient bien, orange : limite, rouge : trop juste), la meilleure marquée d'une étoile ; commandes à copier pour la quantification choisie ; dépôts alternatifs et page Hugging Face.
- **Barre d'état** : source du catalogue et son âge, nombre de modèles, progression de la synchronisation (`sync-progress`), quota Hugging Face (`rate-limit`), rappel « Estimations ±30 % ».

Au lancement, le matériel, les paramètres et le premier classement partent en parallèle (catalogue du cache, sinon embarqué), puis `sync_catalog({ force: false })` tourne en arrière-plan : rien n'est téléchargé si le cache a moins de 24 h. Le backend journalise le délai jusqu'au premier classement servi (`premier classement servi depuis le lancement ms=…`) : environ 550 ms en build debug sur la machine de développement, avec le cache.

Les stores (`state/`) reposent sur `resource` d'Angular : un filtre modifié relance `rank` après un debounce de 150 ms (`FILTER_DEBOUNCE_MS`), sauf au premier calcul ; une requête remplacée est annulée, et le classement précédent reste affiché pendant le calcul et après une erreur. Le classement et la fiche sont aussi recalculés quand le matériel change (nouvelle analyse, corrections, simulation) ou quand les paramètres sont enregistrés (compteurs `version` des stores). La fiche détail suit les mêmes filtres. À sa première ouverture, le backend charge les tailles réelles du modèle ; si sa ligne du classement était estimée, le classement est recalculé. Les deux dialogues sont chargés à leur première ouverture (bundle initial : environ 870 Ko).

Tests (`npm run test:ci`) : fonctions de mise en forme, barre mémoire, tri, stores (requête, debounce, erreur, recalcul après les paramètres), composants avec `mockIPC` (tableau trié et badges, fiche détail, copie et ouverture de pages via les plugins, dialogue des paramètres avec le masquage du token, dialogue « Corriger… »), et l'application entière avec les harnesses Angular Material (filtres, ouverture de la fiche, progression de la synchronisation, « Réessayer », export, arrêt d'une simulation). Les fixtures de `src/testing/fixtures.ts` utilisent les types générés : si un type Rust change, elles ne compilent plus.

Le budget du bundle initial est relevé à 1 Mo (avertissement) et 1,5 Mo (erreur) dans `angular.json` : l'application est servie depuis le disque local, pas par le réseau, et Angular + Angular Material pèsent à eux seuls environ 780 Ko bruts (165 Ko compressés).

## Installeur

`npm run tauri build` (depuis `app/ui`) compile l'application en release (LTO, binaire de 15 Mo) puis produit deux installeurs en français :

| Fichier | Taille | Installation |
|---|---|---|
| `target/release/bundle/nsis/GRAM_0.1.0_x64-setup.exe` | 7,7 Mo | Pour l'utilisateur courant, sans droits administrateur, dans `%LOCALAPPDATA%\GRAM` ; raccourci dans le menu Démarrer ; désinstallation depuis les Paramètres Windows |
| `target/release/bundle/msi/GRAM_0.1.0_x64_fr-FR.msi` | 9,0 Mo | Pour toute la machine, dans `C:\Program Files\GRAM` (droits administrateur) ; déploiement par GPO ou Intune |

NSIS et WiX sont téléchargés par la CLI Tauri au premier build (une connexion est nécessaire).

Ce qui rend l'installeur autonome sur une machine Windows propre :

- **WebView2** : présent d'office sous Windows 11. S'il manque (anciens Windows 10), l'installeur télécharge et installe en silence le *bootstrapper* de Microsoft (`webviewInstallMode: downloadBootstrapper`).
- **Pas de redistribuable Visual C++** : `tauri-build` lie le runtime C statiquement. `dumpbin /dependents gram.exe` ne liste que des DLL de Windows et l'UCRT (fournie avec Windows 10 et 11).
- **Données embarquées** : tables (`data/*.json`) et catalogue de démarrage sont compilés dans le binaire. Sans connexion, le premier lancement classe le catalogue embarqué.
- **DLL optionnelles chargées à l'exécution** : NVML (`nvml.dll`, pilote NVIDIA) et DXCore. Sans elles, la détection continue avec DXGI.

Données de l'application : `%LOCALAPPDATA%\fr.gram.desktop\` (`cache\`, `settings.json`, `corrections.json`, et `EBWebView\` pour WebView2). Le token Hugging Face reste dans le Gestionnaire d'identifiants Windows. Le plan prévoyait `%LOCALAPPDATA%\GRAM\`, mais c'est le dossier d'installation par défaut de l'installeur NSIS. Le dossier qui porte l'identifiant de l'application est aussi celui que la désinstallation NSIS supprime quand on coche « Supprimer les données de l'application ». `gram-cli` utilise le même dossier.

Icônes : `npm run tauri icon public/logo.svg` (depuis `app/ui`) régénère `app/src-tauri/icons/` depuis le logo. Supprimer ensuite les dossiers `android/` et `ios/` créés par la commande : seul Windows est visé. `public/favicon.ico` est une copie de `icons/icon.ico`.

**Test sur une Windows vierge** : `scripts/test-installer.ps1` lance Windows Sandbox (Windows Pro ou Entreprise), une Windows jetable sans outils de développement ni redistribuable Visual C++. Dans la sandbox, `scripts/sandbox/install-test.ps1` installe GRAM en silencieux (`/S`), vérifie le raccourci et WebView2, lance l'application, contrôle qu'elle tourne encore après 25 s et prend une capture d'écran. Résultat : `target/sandbox-results/resultat.txt` et `capture.png`. Windows Sandbox s'active une fois, dans un PowerShell administrateur, puis on redémarre : `Enable-WindowsOptionalFeature -Online -FeatureName Containers-DisposableClientVM`.
