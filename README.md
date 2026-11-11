# GRAM

*GPU & RAM Aware Matcher* : une application Windows et macOS (Tauri 2 + Rust + Angular) qui analyse le matériel de la machine et classe les modèles IA exécutables en local, à partir des données Hugging Face.

## Prérequis

Windows :

- Rust stable (MSVC) et les Build Tools Visual Studio
- Node.js 24+ et npm
- WebView2 (déjà présent sous Windows 11)

macOS (10.15 ou plus récent ; Apple Silicon ou Intel) :

- Outils en ligne de commande de Xcode (`xcode-select --install`)
- Rust stable (`rustup`, cible `aarch64-apple-darwin` ou `x86_64-apple-darwin`)
- Node.js 24+ et npm

## Commandes

Les mêmes sous Windows (PowerShell) et macOS (Terminal), sauf les scripts indiqués.

```powershell
cd app/ui
npm install
npm run tauri dev       # lance l'application (Angular sur :1420 + fenêtre Tauri)
npm run test:ci         # tests Angular (Vitest)
npm run tauri build     # installeurs NSIS et MSI sous Windows, GRAM.app et .dmg sous macOS (voir « Installeur »)
npm run tauri icon public/logo.svg   # régénère les icônes depuis le logo

cd ../..
cargo test --workspace  # tests Rust ; régénère aussi les types TypeScript
cargo run -p gram-cli -- --help
cargo run -p gram-cli -- hw          # matériel détecté (à comparer au Gestionnaire des tâches, ou à « Informations système » sur un Mac)
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
cargo run -p gram-cli -- estimate Qwen3-8B --profile tests/fixtures/profiles/p5-m1pro-16go-macos.json   # Mac Apple Silicon
cargo run -p gram-cli -- ollama                        # modèles installés dans Ollama, rapprochés du catalogue
cargo run -p gram-cli -- ollama --measure Qwen3-8B --quant Q4_K_M --ctx 8192   # mesure réelle face à l'estimation

powershell -ExecutionPolicy Bypass -File scripts/check.ps1   # CI locale complète (Windows)
powershell -ExecutionPolicy Bypass -File scripts/test-installer.ps1   # installeur testé dans Windows Sandbox
scripts/check.sh                                             # CI locale complète (macOS, Linux)
node scripts/release.mjs 0.2.0                               # release : sur le Mac puis sur le PC Windows (voir « Releases et mises à jour »)
```

## Structure

| Dossier | Rôle |
|---|---|
| `crates/gram-core` | Types, moteur d'estimation, scoring (pur, sans I/O) |
| `crates/gram-hw` | Détection matériel et runtimes installés (Windows, macOS ; CPU et RAM seulement ailleurs) |
| `crates/gram-hub` | Client Hugging Face, cache, parseur GGUF, dédoublonnage, catalogue embarqué |
| `crates/gram-ollama` | API locale d'Ollama : modèles installés, mesure de la vitesse et de la mémoire réelles |
| `crates/gram-cli` | Binaire de debug : `hw`, `sync`, `estimate`, `rank`, `ollama`, `snapshot` |
| `app/src-tauri` | Application Tauri : commandes IPC, état, événements, icônes (`icons/`, générées) |
| `app/ui` | Frontend Angular + Angular Material : `core/` (service Tauri typé, types générés), `state/` (stores à base de signals), `features/` (bandeau matériel et dialogue « Corriger… », filtres, classement, fiche détail, paramètres, barre d'état), `shared/` (mise en forme française, libellés, badge de mode), `src/testing/` (fixtures typées et faux backend pour les tests), `public/logo.svg` (logo, source des icônes) |
| `data` | Tables embarquées : `gpu_specs.json` (bande passante des GPU, liste du mode simulation), `apple_chips.json` (type, débit et largeur du bus de la mémoire unifiée des puces Apple Silicon), `uploaders.json` (priorité des uploaders GGUF), `quant_quality.json` (bits par poids typiques pour les modèles pas encore détaillés, et facteur de qualité selon les bits par poids), `quality_overrides.json` (bonus de qualité manuels par famille, vide au départ), `seed_catalog.json` (catalogue de démarrage, généré) |
| `scripts` | `check.ps1` (CI locale sous Windows), `check.sh` (CI locale sous macOS et Linux), `test-installer.ps1` et `sandbox/` (installeur testé sur une Windows vierge), `release.mjs` (release construite sur le Mac et le PC Windows), `latest-json.mjs` (manifeste de mise à jour) |
| `tests/fixtures` | Données pour les tests : dumps matériels `gram-cli hw --raw`, réponses Hugging Face, profils matériels types (`profiles/`), modèles réels du catalogue (`models/`), échantillon figé de 35 modèles pour le classement (`catalog/`), réponses de l'API d'Ollama (`ollama/`) et release fictive 99.0.0 signée avec une clé de test (`update/`) |

Les types Rust exposés au frontend sont convertis en TypeScript par `ts-rs` dans `app/ui/src/app/core/generated/` à chaque `cargo test`. Ne pas modifier ces fichiers à la main.

`npm run tauri` passe par `app/ui/scripts/tauri.mjs`, qui indique à la CLI Tauri où se trouve `app/src-tauri` (variable `TAURI_APP_PATH`).

## Détection du matériel

`gram-hw` sépare la collecte (propre à chaque système) de la construction du profil (`build_profile`, pure et testée). Les corrections manuelles et la simulation (`Overrides`) sont des entrées de `build_profile` : on repart toujours des données brutes.

- **Windows** : DXGI + DXCore (GPU de tous constructeurs, iGPU), NVML optionnelle (NVIDIA), WMI (barrettes, CPU), sysinfo.
- **macOS** : Metal (`MTLCopyAllDevices`, qui ne fait pas basculer un Mac Intel sur son GPU dédié), `system_profiler` (type de mémoire, barrettes d'un Mac Intel, cœurs du GPU ; lancé sur un thread à part, abandonné au bout de 10 s), sysinfo (CPU, RAM, disques sans les volumes système d'APFS). Sur **Apple Silicon**, le GPU est intégré mais à **mémoire unifiée** (`GpuInfo::unified_memory`) : il reçoit les couches du modèle (Metal) jusqu'à la limite `recommendedMaxWorkingSetSize` de Metal, la « VRAM » que comptent llama.cpp et Ollama (11,8 Gio sur un M1 Pro de 16 Go ; `sudo sysctl iogpu.wired_limit_mb=…` la relève). macOS ne donne ni la vitesse ni la largeur du bus de cette mémoire : elles viennent de `data/apple_chips.json` d'après le nom de la puce (« Apple M1 Pro » : LPDDR5-6400 sur 256 bits, 4 canaux de 64 bits, 204,8 Go/s), et la version d'une puce se choisit d'après les cœurs de son GPU (M3 Max 30 ou 40 cœurs : 307 ou 410 Go/s). Une puce plus récente que la table prend la dernière puce connue de la même gamme, avec un avertissement. Sur un **Mac Intel**, l'iGPU Intel compte comme du CPU et le GPU AMD dédié comme sous Windows (bande passante par défaut s'il est absent de la table).
- **Ailleurs** (Linux) : CPU, RAM et disques seulement.

Les runtimes sont cherchés dans le `PATH` et aux emplacements des installateurs : dossiers des programmes sous Windows ; `/Applications` et `~/Applications` (`Ollama.app`, `LM Studio.app`), `~/.lmstudio/bin` et les dossiers de Homebrew et MacPorts sous macOS (une application lancée depuis le Finder n'hérite pas du `PATH` du terminal).

Pour ajouter un cas de test matériel (portable, GPU AMD, machine sans GPU, autre Mac…) : lancer `gram-cli hw --raw` sur la machine concernée, enregistrer la sortie dans `tests/fixtures/hardware/` et ajouter un test dans `crates/gram-hw/src/profile/tests.rs`. Deux dumps réels y sont : un PC Windows (Ryzen 7 9800X3D, RTX 5080) et un MacBook Pro M1 Pro 16 Go.

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

1. **Mémoire utilisable** (`MemoryBudget::from_profile`) : `min(VRAM × 90 %, VRAM libre + 256 Mio)` sur le GPU retenu, et `min(RAM disponible, RAM × 75 %)`. Le réglage « je ferme tout » passe à `RAM − 4 Go`. Un iGPU compte comme du CPU, sauf en **mémoire unifiée** (Mac Apple Silicon) : la « VRAM » est alors `limite de Metal × 90 %`, au plus la RAM utilisable, et elle est prise sur la RAM (`MemoryBudget::unified`) : en hybride, la part GPU et la part CPU doivent tenir ensemble dans la RAM utilisable, au lieu de s'additionner. Les deux bandes passantes sont celles de la mémoire unifiée ; seule l'efficacité diffère (Metal ou CPU).
2. **Besoin** : poids (taille réelle des fichiers) + cache KV (`2 × couches d'attention × têtes KV × head_dim × octets`, f16 / q8_0 / q4_0) + 0,5 Go de surcoût (+ le projecteur vision si l'usage vision est demandé).
3. **Mode** : GPU si tout tient en VRAM ; sinon hybride (KV et surcoût en VRAM, puis des couches entières : le `-ngl` de llama.cpp) ; sinon CPU. Si rien ne tient, le contexte est divisé par deux jusqu'à 2 048 tokens, puis le modèle est déclaré trop gros.
4. **Vitesse** : la génération est limitée par la bande passante, plus un temps fixe par token. `temps = octets GPU / (BW GPU × 0,81) + octets RAM / (BW RAM × 0,62) + 0,8 ms` et `tok/s = 1 / temps`. Le GPU et le CPU n'exploitent pas la même part de leur bande passante, d'où deux efficacités. Le temps fixe (lancement des calculs, synchronisations) compte surtout quand un token lit peu d'octets : un modèle de 1,7B en Q4_K_M lit ses poids en 1,2 ms sur une RTX 5080, et le temps fixe ajoute 0,8 ms. Les valeurs 0,81, 0,62 et 0,8 ms viennent de mesures réelles de deux modèles denses, sur GPU et sur CPU (voir « Calibration avec Ollama ») ; une mesure avec Ollama les recalibre pour la machine. Un MoE ne lit que ses paramètres actifs, avec trois termes propres, ajustés sur un MoE mesuré : `paramètres actifs × (1,5 + 0,7 × bpw) / 8` octets par token (hors experts, l'attention, la tête de sortie et l'expert partagé restent quantifiés plus finement : en Q2_K, un MoE lit ~1,25 fois la part de ses paramètres actifs dans le fichier, en Q8_0 ~0,9 fois), lus à 75 % de l'efficacité d'un modèle dense, et 1,3 ms de temps fixe en plus par token (routeur, noyaux des experts).
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
| P5 : MacBook Pro M1 Pro, 16 Go unifiés (204,8 Go/s) | GPU (Metal), ~32 tok/s, parfait | trop gros |

Sur Mac, les efficacités par défaut (mesurées sur une RTX 5080 et de la DDR5) donnent des vitesses un peu optimistes : ~+15 % sur un M1 Pro d'après les mesures publiées de llama.cpp, davantage sur les puces Max et Ultra, dont le GPU exploite une plus petite part de leur bande passante. Une mesure avec Ollama (« Calibration avec Ollama ») recalibre l'efficacité GPU pour la machine.

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

Chaque ligne indique aussi les quantifications du modèle déjà installées dans Ollama (`installed`, voir « Calibration avec Ollama »).

La fiche détail (`model_detail`) évalue de la même façon toutes les quantifications d'un modèle et génère les commandes du runtime choisi (§10.3) : `ollama run hf.co/{dépôt}:{QUANT}` puis `/set parameter num_ctx`, `llama-server -hf … -c … -ngl …` (999 si tout tient en VRAM), ou la page Hugging Face pour LM Studio. Les variables d'environnement d'Ollama dépendent du système du profil (`HardwareProfile::os`) : `setx` sous Windows, `launchctl setenv` sous macOS (ce que lit l'application Ollama, jusqu'au redémarrage), `export` sous Linux. Sur un Mac, des notes signalent la limite de Metal ; sur un Mac Intel, qu'Ollama n'y utilise pas le GPU et que LM Studio n'y fonctionne pas. Les règles propres aux runtimes (contexte par défaut d'Ollama, GPU intégré, cache KV quantifié, dépôt protégé, fichier découpé) sont toutes dans `crates/gram-core/src/launch.rs`.

Le snapshot du classement (`crates/gram-core/tests/snapshots/`) porte sur un échantillon figé de 35 modèles réels et les 4 profils types. Il a été relu à la main. Après un changement volontaire du scoring, relire les différences avec `cargo insta review`, ou `INSTA_UPDATE=always cargo test -p gram-core` puis `git diff`.

## Application : commandes IPC

Toute la logique est en Rust (`app/src-tauri`). Au démarrage, le matériel, les runtimes et le catalogue (cache, sinon catalogue embarqué) sont chargés en parallèle. Le classement est recalculé à chaque appel de `rank`.

| Commande | Arguments | Résultat |
|---|---|---|
| `get_hardware` | – | `HardwareProfile` (détecté une fois, puis gardé en mémoire) |
| `rescan_hardware` | – | `HardwareProfile` (corrections conservées) |
| `set_hardware_overrides` | `{ overrides: Overrides }` | `HardwareProfile`. Corrections et simulation (`simulatedGpu`, `simulatedRamTotal`), utilisées par les classements suivants ; `{}` revient aux valeurs détectées. Les corrections sont enregistrées dans `corrections.json` et reprises au lancement, pas la simulation |
| `hardware_options` | – | `HardwareOptions` : le matériel tel que détecté (sans corrections) et les GPU simulables de `data/gpu_specs.json` (dialogue « Corriger… ») |
| `detect_runtimes` | – | `RuntimesStatus`, avec les modèles installés dans Ollama (`ollamaModels`) s'il tourne |
| `sync_catalog` | `{ force: boolean }` | `SyncSummary`, plus les événements `sync-progress`, `rate-limit` et `sync-done` |
| `rank` | `{ request: RankRequest }` | `RankResponse` (`useCase`, `ctx`, `runtime`, `maxSize` en octets, `showTooBig`, `limit` ; tout est optionnel) |
| `get_model_detail` | `{ request: DetailRequest }` | `ModelDetail`. À la première ouverture, les quantifications de tous les dépôts du modèle sont chargées (cache, sinon réseau) |
| `get_settings` / `save_settings` | – / `{ settings: Settings }` | `Settings`, enregistrés dans `%LOCALAPPDATA%\fr.gram.desktop\settings.json` et ramenés dans leurs bornes |
| `get_default_settings` | – | `Settings` du §12 (bouton « Valeurs par défaut », rappels « défaut : … ») |
| `set_hf_token` / `clear_hf_token` / `has_hf_token` | `{ token }` / – / – | `null` / `null` / `boolean`. Le token est rangé dans le trousseau du système (Gestionnaire d'identifiants Windows, Trousseau d'accès macOS) et n'est jamais renvoyé |
| `get_cache_info` / `clear_cache` | – | `CacheInfo { path, files, bytes }`. Vider le cache est refusé pendant une synchronisation ; le catalogue en mémoire reste utilisé |
| `ollama_measure` | `{ request: MeasureRequest }` | `Measurement`. Mesure réelle d'une quantification déjà installée dans Ollama (`repoId`, `label`, `useCase`, `ctx` comme la fiche), enregistrée dans `measurements.json`. Une seule à la fois ; refusée si la quantification ne tient pas d'après l'estimation |
| `get_calibration` / `clear_measurements` | – | `Calibration` : mesures enregistrées (la plus récente d'abord) et réglages de vitesse proposés |
| `apply_calibration` | – | `Settings`, avec les efficacités GPU et RAM (et le temps fixe par token, s'il a pu être ajusté) proposées par les mesures |
| `check_update` | – | `UpdateCheck { currentVersion, update }` : `update` (`UpdateInfo { version, notes, date }`) est la mise à jour proposée par la dernière release GitHub, `null` si GRAM est à jour |
| `install_update` | – | Ne revient pas : télécharge la mise à jour trouvée par `check_update` (événements `update-progress`), vérifie sa signature, l'installe et relance l'application. Refusée en mode développement |
| `export_ranking` | `{ format: 'md' \| 'csv', path? }` | Chemin écrit, ou `null` si l'utilisateur annule. Exporte le dernier classement en entier. Sans `path`, le backend ouvre le dialogue « Enregistrer sous » (plugin dialog appelé depuis Rust : aucune permission côté WebView) ; avec `path`, chemin absolu avec l'extension du format. CSV : `;`, virgule décimale, UTF-8 avec BOM (Excel en français) |

Événements : `hardware-ready` (`HardwareProfile`), `sync-progress` (`SyncProgress`), `sync-done` (`SyncSummary`), `rate-limit` (`RateLimitInfo`), `update-progress` (`UpdateProgress { downloaded, total }`). Les erreurs rejettent la promesse avec un message en français.

Différences avec l'esquisse du §5.3 : la simulation passe par `set_hardware_overrides` (déjà gérée par `Overrides`) plutôt que par un drapeau de `RankRequest`, ainsi ce qui est affiché est toujours ce qui est classé. `get_model_detail` reçoit le cas d'usage et le contexte, dont dépendent les estimations. `export_ranking` ouvre lui-même le dialogue d'enregistrement : le WebView n'a pas à choisir un chemin d'écriture. `ollama_measure` reçoit la quantification du catalogue plutôt qu'un nom Ollama et un contexte : le backend retrouve le nom sous lequel elle est installée et mesure au contexte retenu par l'estimation, ainsi mesure et estimation portent sur la même chose.

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

L'interface Angular (`app/ui`) affiche les données et envoie des filtres ; elle ne calcule rien et ne fait aucun appel réseau. `TauriService` est le seul point d'accès à `invoke`, `listen` et aux deux plugins utilisés : presse-papiers (bouton « Copier ») et ouverture d'URL, limitée à `https://huggingface.co/*` et aux releases GitHub de GRAM dans `capabilities/default.json`.

Disposition (fenêtre 1280×800) :

- **Barre d'outils** : « Ré-analyser » (`rescan_hardware` + `detect_runtimes`), « Actualiser les modèles » (`sync_catalog` forcé), « Exporter » (Markdown ou CSV, le classement affiché avec ses filtres et toutes ses lignes), mise à jour (une icône qui lance la recherche ; « Mise à jour X.Y.Z » quand une version plus récente est publiée) et « Paramètres ».
- **Dialogue « Mise à jour »** : recherche à l'ouverture (sauf si une mise à jour est déjà connue), version proposée, date et notes de version, « Installer et redémarrer » avec la progression du téléchargement (le dialogue ne se ferme pas pendant l'installation), message d'erreur avec « Réessayer », lien vers la page des releases pour une installation à la main.
- **Bandeau matériel** : CPU, RAM, GPU, disques, runtimes installés, mémoire réellement utilisable. L'origine d'une valeur est affichée quand ce n'est pas une mesure directe (« estimée », « table des GPU », « corrigée »). Les GPU non retenus sont signalés sur la ligne GPU. Bouton « Corriger… » ; pendant une simulation, étiquette « Simulation » (le détail en infobulle) et bouton « Arrêter », qui garde les corrections.
- **Dialogue « Corriger… »** (§6.6) : corrections (canaux mémoire, GPU retenu s'il y en a plusieurs, GPU intégré oui/non, bande passante du GPU), chacune avec la valeur détectée en rappel, et simulation (un GPU de la table, regroupée par constructeur ; une quantité de RAM de 8 à 256 Go). « Valeurs détectées » efface tout.
- **Dialogue « Paramètres »** (§11.2, §12), en quatre onglets : estimation (marge VRAM, part de RAM, « je ferme tout », surcoût, cache KV, efficacités GPU et RAM, temps fixe par token, bits par poids et contexte minimum), cas d'usage (contexte par défaut, seuil « lent » et poids du score de chacun), catalogue (téléchargements minimum, durée de vie du cache, modèles détaillés à la synchronisation, lignes affichées, taille et vidage du cache), Hugging Face (token). Les réglages sont modifiés sur une copie et enregistrés avec « Enregistrer », qui recalcule le classement et la fiche ; « Valeurs par défaut » reprend celles du §12. Le champ du token est masqué, vidé après l'enregistrement, et seul l'état « configuré / non configuré » est affiché.
- **Filtres** : cas d'usage (il reprend aussi son contexte par défaut), curseur de contexte de 2k à 128k (une puissance de deux par cran), runtime (enregistré dans les paramètres), taille maximale en Go, affichage des modèles trop gros.
- **Classement** : remarques du runtime, tableau `MatTable` triable (le rang « # » reste celui du score), badge de mode coloré, coche « déjà installé dans Ollama » (les quantifications en infobulle), détail du score en infobulle, « Voir plus » (25 lignes par page, paramètre `pageSize`), sections repliables « Exécutable mais lent » et « Trop gros ».
- **Fiche détail** (panneau à droite, sans fond bloquant : un clic sur une autre ligne change de modèle) : barre mémoire empilée [poids | KV | vision | surcoût] face à la mémoire du mode, avec un repère à la fin de la VRAM en hybride ; toutes les quantifications (vert : tient bien, orange : limite, rouge : trop juste), la meilleure marquée d'une étoile, celles installées dans Ollama marquées d'une coche ; commandes à copier pour la quantification choisie ; mesure réelle avec Ollama (voir plus bas) ; dépôts alternatifs et page Hugging Face.
- **Mesure réelle** (fiche détail, si Ollama est installé) : bouton « Mesurer » pour une quantification installée, sinon la commande `ollama pull` à copier et « Actualiser » ; tableau mesuré / estimé / écart (vitesse, mémoire, dont VRAM), remarques (contexte chargé, partie en RAM, autres modèles chargés…) ; proposition de recalibrage, « Recalibrer l'estimation » et « Effacer les mesures ».
- **Barre d'état** : source du catalogue et son âge, nombre de modèles, progression de la synchronisation (`sync-progress`), quota Hugging Face (`rate-limit`), rappel « Estimations ±30 % ».

Au lancement, le matériel, les paramètres et le premier classement partent en parallèle (catalogue du cache, sinon embarqué), puis `sync_catalog({ force: false })` tourne en arrière-plan : rien n'est téléchargé si le cache a moins de 24 h. Le backend journalise le délai jusqu'au premier classement servi (`premier classement servi depuis le lancement ms=…`) : environ 550 ms en build debug sur la machine de développement, avec le cache.

Les stores (`state/`) reposent sur `resource` d'Angular : un filtre modifié relance `rank` après un debounce de 150 ms (`FILTER_DEBOUNCE_MS`), sauf au premier calcul ; une requête remplacée est annulée, et le classement précédent reste affiché pendant le calcul et après une erreur. Le classement et la fiche sont aussi recalculés quand le matériel change (nouvelle analyse, corrections, simulation) ou quand les paramètres sont enregistrés (compteurs `version` des stores). La fiche détail suit les mêmes filtres. À sa première ouverture, le backend charge les tailles réelles du modèle ; si sa ligne du classement était estimée, le classement est recalculé. Les deux dialogues sont chargés à leur première ouverture (bundle initial : environ 880 Ko).

Tests (`npm run test:ci`) : fonctions de mise en forme, barre mémoire, tri, stores (requête, debounce, erreur, recalcul après les paramètres), composants avec `mockIPC` (tableau trié et badges, fiche détail, copie et ouverture de pages via les plugins, dialogue des paramètres avec le masquage du token, dialogue « Corriger… », mesure Ollama et recalibrage), et l'application entière avec les harnesses Angular Material (filtres, ouverture de la fiche, progression de la synchronisation, « Réessayer », export, arrêt d'une simulation). Les fixtures de `src/testing/fixtures.ts` utilisent les types générés : si un type Rust change, elles ne compilent plus.

Le budget du bundle initial est relevé à 1 Mo (avertissement) et 1,5 Mo (erreur) dans `angular.json` : l'application est servie depuis le disque local, pas par le réseau, et Angular + Angular Material pèsent à eux seuls environ 780 Ko bruts (165 Ko compressés).

## Calibration avec Ollama

La phase 7 compare les estimations à la réalité, sur la machine, avec l'API locale d'Ollama (`http://localhost:11434`, ou `OLLAMA_HOST`). Tout passe par Rust (`crates/gram-ollama` pour l'API, `gram-core::calibration` pour les calculs, purs et testés) ; le WebView n'a toujours aucun accès réseau.

**Modèles déjà installés** (`GET /api/tags`, relu avec les runtimes au lancement, à « Ré-analyser » et à « Actualiser » dans la fiche). Un modèle installé depuis Hugging Face s'appelle `hf.co/{dépôt}:{quantification}` dans Ollama, c'est ce qu'installent les commandes de GRAM : il est rapproché du catalogue par son dépôt (canonique, alternatif ou celui d'une quantification) et sa quantification, sans tenir compte de la casse. Sans étiquette, ou avec `latest`, la quantification annoncée par Ollama est retenue. Les modèles de la bibliothèque d'Ollama (`qwen3:8b`) ne sont pas rapprochés : rien ne dit de quel dépôt ils viennent. Les modèles distants (`remote_host`) sont écartés.

**Mesure** (bouton « Mesurer », `ollama_measure`) :

1. une courte génération (8 tokens) charge le modèle avec le contexte retenu par l'estimation (`num_ctx`) ;
2. une génération de 128 tokens, mêmes options (pas de rechargement), température 0 : Ollama renvoie `eval_count` et `eval_duration`, d'où la vitesse de génération, sans le traitement du prompt (celui-ci est affiché pour information, sur les seuls tokens recalculés : remesurer dans les 5 minutes reprend presque tout le prompt du cache d'Ollama, et sa vitesse n'est alors pas affichée) ;
3. `GET /api/ps` donne la mémoire occupée et la part en VRAM.

Le modèle reste chargé 5 minutes (`keep_alive`), comme après un usage normal. La mesure est comparée à l'estimation de la machine réelle (corrections comprises, simulation ignorée), avec des remarques quand elles divergent pour une raison connue : contexte chargé différent, partie du modèle en RAM alors que l'estimation tenait tout en VRAM, mémoire bien plus élevée (`OLLAMA_NUM_PARALLEL` > 1), autres modèles chargés, trop peu de tokens générés (moins de 32 : mesure non utilisée pour la calibration). Les 50 dernières mesures sont gardées dans `measurements.json` (une par modèle Ollama et contexte).

**Recalibrage** (`get_calibration`, `apply_calibration`). La vitesse estimée suit `temps par token = transfert GPU / efficacité GPU + transfert RAM / efficacité RAM + temps fixe`, où les transferts sont les temps de lecture des poids actifs à pleine bande passante. Les termes propres aux MoE ne sont pas recalibrés : leurs octets sont ramenés à l'efficacité d'un modèle dense, et leur temps fixe propre est retiré du temps mesuré. Pour chaque mesure, ils sont calculés avec la répartition VRAM / RAM **mesurée** (`size_vram / size`) : une erreur de placement de l'estimation ne fausse pas la calibration. Chaque mesure compte pour le côté dont la lecture domine son temps : la VRAM pour un modèle qui y tient, la RAM pour un modèle hybride ou sur CPU. Un côté sans mesure garde son efficacité.

- Des modèles de tailles proches : l'efficacité de chaque côté mesuré, médiane de `transfert du côté / (temps mesuré − temps fixe actuel − temps de l'autre côté)`.
- Au moins deux mesures d'un même côté dont les octets lus par token diffèrent d'un facteur 2 (un petit et un gros modèle, un dense et un MoE) : moindres carrés sur l'erreur relative, d'où les efficacités des côtés mesurés et le temps fixe, retenus s'ils restent plausibles (efficacité de 0,05 à 2, temps fixe de 0 à 20 ms).

Les efficacités sont arrondies au centième et le temps fixe au dixième de milliseconde. « Recalibrer l'estimation » les enregistre dans les paramètres : classement et fiche sont recalculés. Les mesures restent, et la proposition est relue quand les paramètres ou le matériel changent.

Mesures réelles avec Ollama 0.40.16 au contexte 8K, sur une RTX 5080 (960 Go/s) avec de la DDR5-6000 en double canal (96 Go/s), médianes de 3 à 5 essais :

| Modèle (Q4_K_M) | Octets lus par token | GPU | CPU seul (`num_gpu: 0`) |
|---|---|---|---|
| Qwen3-1.7B | 1,1 Go | 450 tok/s | 50 tok/s |
| Qwen3-8B | 5,0 Go | 137 tok/s | 12 tok/s |

Une seule efficacité ne convient pas : seul, le GPU donne 0,80 et 0,8 ms, le CPU 0,64 et 2,1 ms. Les valeurs par défaut (0,81 sur GPU, 0,62 sur CPU, 0,8 ms) sont l'ajustement des quatre mesures, à moins de 4 % de chacune. Les anciennes (0,55 partout et 1,5 ms) sous-estimaient le GPU de 34 à 38 % et le CPU de 11 à 14 %. La mémoire est estimée du côté prudent : +6 à +11 % pour Qwen3-8B (6,3 Go pour 5,7 Go annoncés par Ollama), +19 % pour Qwen3-1.7B.

**MoE.** Ornith-1.5-35B-A3B en Q2_K (13,1 Go, 3,5B paramètres actifs sur 35,5B, 8 experts sur 256), mêmes conditions, médianes de 3 ou 4 essais ; en hybride, les couches en VRAM sont imposées (`num_gpu`) :

| Placement | Mesuré | Formule des modèles denses | Avec les termes MoE |
|---|---|---|---|
| GPU | 208 tok/s | ~405 (+95 %) | 209 (+0 %) |
| CPU seul | 26,7 tok/s | ~44 (+65 %) | 26,9 (+1 %) |
| Hybride 30/41 couches | 79,7 tok/s | – | 74 (−7 %) |
| Hybride 20/41 couches | 50,4 tok/s | – | 47 (−7 %) |

L'écart ne vient pas d'un seul facteur : il est de +95 % sur GPU et de +65 % sur CPU, ce qu'aucune correction des octets seule, ni aucun temps fixe seul, n'explique. Les octets réellement lus par token se lisent dans l'en-tête du GGUF (taille de chaque tenseur, experts routés comptés pour 8/256) : 1,66 Go en Q2_K, et non les 1,30 Go de la part des paramètres actifs dans le fichier, parce que l'attention, la tête de sortie (Q6_K) et l'expert partagé sont quantifiés plus finement que les experts. Le rapport dépend de la quantification ; d'après les en-têtes de 6 quantifications d'Ornith-1.5-35B-A3B et 3 de Qwen3-30B-A3B, les bits lus par paramètre actif valent `1,5 + 0,7 × bpw` à ±6 % près (1,46 fois la part du fichier en IQ2_XXS, 0,86 fois en Q8_0, où les embeddings non lus l'emportent). Restent l'efficacité de lecture (75 % de celle d'un modèle dense) et le temps fixe propre (1,3 ms), déterminés par les mesures GPU et CPU. Vérification indépendante : pour Qwen3-30B-A3B en Q4_K_M sur RTX 4090, l'estimation passe de ~330 à ~195 tok/s, pour ~190 publiés. La mémoire d'Ornith est estimée à +6 % (12,8 Go pour 12,1 Go).

**Limites.** Ces valeurs viennent d'une seule machine (GPU NVIDIA récent, CPU de bureau à grand cache) ; les réponses d'Ollama sont dans `tests/fixtures/ollama/`. Un portable ou un GPU plus ancien exploite sans doute une moindre part de sa bande passante. Un seul temps fixe sert au GPU et au CPU. Les termes MoE viennent d'un seul MoE mesuré, d'architecture hybride (30 couches à attention linéaire sur 40) : une part de son temps fixe propre tient peut-être à cette architecture plutôt qu'aux experts. Leur efficacité de lecture et leur temps fixe propre ne sont pas recalibrés par les mesures.

`gram-cli ollama` liste les modèles installés et leur correspondance dans le catalogue ; `gram-cli ollama --measure <modèle> [--quant Q4_K_M] [--ctx 8192]` fait la même mesure que l'application, avec les paramètres par défaut, et affiche l'écart et la calibration qui en découle.

## Installeur

### Windows

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

Données de l'application : `%LOCALAPPDATA%\fr.gram.desktop\` (`cache\`, `settings.json`, `corrections.json`, `measurements.json`, et `EBWebView\` pour WebView2). Le token Hugging Face reste dans le Gestionnaire d'identifiants Windows. Le plan prévoyait `%LOCALAPPDATA%\GRAM\`, mais c'est le dossier d'installation par défaut de l'installeur NSIS. Le dossier qui porte l'identifiant de l'application est aussi celui que la désinstallation NSIS supprime quand on coche « Supprimer les données de l'application ». `gram-cli` utilise le même dossier.

Icônes : `npm run tauri icon public/logo.svg` (depuis `app/ui`) régénère `app/src-tauri/icons/` depuis le logo (`icon.ico` pour Windows, `icon.icns` pour macOS). Supprimer ensuite les dossiers `android/` et `ios/` créés par la commande : seuls Windows et macOS sont visés. `public/favicon.ico` est une copie de `icons/icon.ico`.

**Test sur une Windows vierge** : `scripts/test-installer.ps1` lance Windows Sandbox (Windows Pro ou Entreprise), une Windows jetable sans outils de développement ni redistribuable Visual C++. Dans la sandbox, `scripts/sandbox/install-test.ps1` installe GRAM en silencieux (`/S`), vérifie le raccourci et WebView2, lance l'application, contrôle qu'elle tourne encore après 25 s et prend une capture d'écran. Résultat : `target/sandbox-results/resultat.txt` et `capture.png`. Windows Sandbox s'active une fois, dans un PowerShell administrateur, puis on redémarre : `Enable-WindowsOptionalFeature -Online -FeatureName Containers-DisposableClientVM`.

### macOS

Sous macOS, `npm run tauri build` (depuis `app/ui`) produit :

| Fichier | Contenu |
|---|---|
| `target/release/bundle/macos/GRAM.app` | L'application, à glisser dans `/Applications` |
| `target/release/bundle/dmg/GRAM_0.1.0_aarch64.dmg` | Image disque à distribuer (`x64` pour un Mac Intel) |

Pour un binaire universel (Apple Silicon et Intel) : `rustup target add x86_64-apple-darwin aarch64-apple-darwin`, puis `npm run tauri build -- --target universal-apple-darwin`. Avec `CI=true`, le script de création du `.dmg` ne pilote pas le Finder (pas de mise en page de la fenêtre, mais pas de demande d'autorisation non plus).

- **Système minimal** : macOS 10.15 (`bundle.macOS.minimumSystemVersion`), pour les propriétés de Metal lues par la détection (`hasUnifiedMemory`, macOS 10.15+).
- **Aucune dépendance à installer** : WebKit et Metal font partie de macOS ; tables et catalogue de démarrage sont compilés dans le binaire.
- **Signature** : l'application est signée *ad hoc* (`bundle.macOS.signingIdentity: "-"`), sans compte Apple Developer : macOS accepte de la lancer sur Apple Silicon, mais Gatekeeper bloque une copie téléchargée au premier lancement. Réglages Système → Confidentialité et sécurité → Ouvrir quand même, ou `xattr -dr com.apple.quarantine /Applications/GRAM.app`. Les mises à jour automatiques ne sont pas concernées (le paquet téléchargé par GRAM n'est pas mis en quarantaine). Pour une distribution sans avertissement : un certificat Developer ID dans `signingIdentity` et la notarisation (variables `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` de la CLI Tauri). Après une mise à jour, macOS peut redemander l'accès au Trousseau pour le token Hugging Face : la signature *ad hoc* change à chaque version.

Données de l'application : `~/Library/Application Support/fr.gram.desktop/` (`cache/`, `settings.json`, `corrections.json`, `measurements.json`), partagé avec `gram-cli`. Le token Hugging Face est rangé dans le Trousseau d'accès (entrée « GRAM »).

## Releases et mises à jour

Chaque release GitHub contient les installeurs, prêts à télécharger :

| Fichier | Système |
|---|---|
| `GRAM_X.Y.Z_x64-setup.exe` | Windows : installeur NSIS, pour l'utilisateur courant, sans droits administrateur |
| `GRAM_X.Y.Z_x64_fr-FR.msi` | Windows : installeur MSI, pour toute la machine |
| `GRAM_X.Y.Z_universal.dmg` | macOS 10.15+, Apple Silicon et Intel (binaire universel) |
| `GRAM_X.Y.Z_universal.app.tar.gz`, `latest.json` | Mises à jour automatiques (paquet macOS et manifeste) |

Ni l'installeur Windows ni l'application macOS ne sont signés par une autorité reconnue : SmartScreen (« Informations complémentaires » → « Exécuter quand même ») et Gatekeeper (voir « Installeur » → « macOS ») préviennent au premier lancement. La page de chaque release le rappelle.

### Publier une version

Les releases sont construites sur tes machines, pas par GitHub : la clé de signature des mises à jour ne quitte pas `~/.tauri`. Le `.msi` ne se construit que sous Windows, d'où une étape par système. La même commande sur les deux machines, dans n'importe quel ordre :

```sh
# Première machine (ici le Mac)
scripts/check.sh                          # ou scripts/check.ps1 sous Windows
node scripts/release.mjs 0.2.0            # --notes-file notes.md pour écrire les notes soi-même

# Seconde machine (ici le PC Windows)
git pull
node scripts/release.mjs 0.2.0
```

`scripts/release.mjs` :

1. **Première machine** (le tag `v0.2.0` n'existe pas encore), depuis `main` propre : écrit la version dans `Cargo.toml` (`[workspace.package]`, donc toutes les crates et l'application), `app/ui/package.json` et les fichiers de verrouillage, commite « Version 0.2.0 », crée le tag annoté `v0.2.0` (son message est le texte des notes de version : par défaut, la liste des commits depuis le tag précédent), pousse `main` et le tag et crée un **brouillon** de release GitHub, invisible du public. Sur la seconde machine, le dépôt doit être exactement sur le tag (`git pull`).
2. **Chaque machine** : `tauri build` avec `tauri.updater.conf.json`, qui ajoute `createUpdaterArtifacts`. La CLI Tauri signe alors chaque paquet de mise à jour (`.app.tar.gz` sur le Mac, en binaire universel ; `-setup.exe` et `.msi` sous Windows) avec la clé locale. Les paquets sont copiés sous leur nom de release dans `target/release-assets/v0.2.0/`, puis envoyés dans la release avec `latest.json`, complété des plateformes de cette machine (`scripts/latest-json.mjs`).
3. **Quand macOS et Windows y sont**, le brouillon est publié : à partir de là, les applications installées proposent la mise à jour.

Un build qui échoue se relance avec la même commande (le tag existe déjà : seul le build de cette machine est refait). `--dry-run` construit et signe les paquets de la version actuelle dans `target/release-assets/`, sans commit, tag, push ni GitHub. Les builds ordinaires (`npm run tauri build`) ne produisent pas de paquets de mise à jour et n'ont pas besoin de la clé.

Prérequis sur chaque machine, en plus de ceux du développement : `gh` (GitHub CLI : `brew install gh`, `winget install GitHub.cli`), connecté avec `gh auth login`, et la clé (voir ci-dessous). Sur le Mac, le script ajoute la cible Rust `x86_64-apple-darwin` si elle manque (binaire universel). Compter quelques minutes par build (LTO ; deux compilations pour le binaire universel).

### Clé de signature des mises à jour

L'application n'installe une mise à jour que si sa signature correspond à la clé publique de `tauri.conf.json` (`plugins.updater.pubkey`). La clé privée et son mot de passe sont dans `~/.tauri/gram.key` et `~/.tauri/gram.key.password`, sur le Mac qui les a créés ; les copier dans `%USERPROFILE%\.tauri\` sur le PC Windows (clé USB, gestionnaire de mots de passe : pas par le dépôt). Le script lit aussi `TAURI_SIGNING_PRIVATE_KEY` et `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` s'ils sont définis.

**À sauvegarder** (gestionnaire de mots de passe) : sans cette clé, aucune mise à jour ne peut plus être proposée aux applications installées, qu'il faudrait réinstaller à la main avec une nouvelle clé publique.

Pour signer à la main : `node scripts/tauri.mjs signer sign -f ~/.tauri/gram.key -p "$(cat ~/.tauri/gram.key.password)" --app-version X.Y.Z <fichier>` (depuis `app/ui`).

### Mise à jour dans l'application

Quelques secondes après le lancement, GRAM lit `https://github.com/TheProdigy27/GRAM/releases/latest/download/latest.json` (`check_update`). Si une version plus récente est publiée, un message le signale et la barre d'outils affiche « Mise à jour X.Y.Z » ; sinon, rien n'est affiché, même hors ligne. Le bouton de mise à jour lance aussi la recherche à la demande. « Installer et redémarrer » (`install_update`) télécharge le paquet et vérifie sa signature (un paquet modifié est refusé avant toute installation), puis :

- **Windows** : l'installeur de la nouvelle version se lance en mode passif (une barre de progression, aucune question), GRAM se ferme et l'installeur le relance. Une installation faite avec le MSI se met à jour avec le MSI (entrée `windows-x86_64-msi` du manifeste ; droits administrateur), les autres avec l'installeur NSIS ;
- **macOS** : `GRAM.app` est remplacée sur place (macOS demande un mot de passe administrateur si le dossier n'est pas modifiable), puis GRAM redémarre.

Les paramètres, corrections, mesures et le cache restent dans le dossier de l'application. Le plugin updater de Tauri n'est appelé que depuis Rust (`commands/update.rs`) : le WebView n'a aucune permission sur lui. La signature porte aussi la version (`requireSignedVersion`) : un manifeste modifié ne peut pas faire réinstaller une ancienne release sous un numéro plus récent. En développement (`npm run tauri dev`), la recherche fonctionne mais l'installation est refusée : le plugin remplacerait le dossier `target/debug`.

Tests (`app/src-tauri/src/tests.rs`) : un faux GitHub sert la release fictive 99.0.0 de `tests/fixtures/update/` (manifeste produit par `scripts/latest-json.mjs`, paquets signés avec une clé de test, `test.key.pub`) : mise à jour proposée ou non, aucune release, paquet modifié refusé après téléchargement (progression comprise), installation refusée en développement, signature liée à la version. Pour régénérer cette release : `signer generate` d'une clé de test, `signer sign --app-version 99.0.0` de chaque paquet, puis `scripts/latest-json.mjs` avec `--pub-date 2026-11-11T12:00:00Z`.
