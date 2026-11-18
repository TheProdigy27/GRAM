//! État partagé de l'application (`tauri::State<AppState>`) : matériel, runtimes, catalogue et
//! paramètres.
//!
//! - Le matériel est détecté une fois (au démarrage), dans `spawn_blocking` : WMI (Windows) a
//!   besoin d'un thread initialisé COM, et `system_profiler` (macOS) prend ~0,3 s. Les corrections et la simulation repartent des données brutes ; les
//!   corrections sont enregistrées d'un lancement à l'autre, pas la simulation.
//! - Le catalogue est chargé hors ligne au démarrage (cache, sinon catalogue embarqué), puis
//!   remplacé par `sync_catalog`. Les fiches enrichies à la demande y sont reportées.
//! - Les modèles installés dans Ollama sont relus avec les runtimes ; les mesures réelles
//!   (phase 7) sont enregistrées d'un lancement à l'autre, et reportées dans le manifeste de
//!   GRAM Harness (`models.json` du dossier partagé).
//! - Un seul ajout à GRAM Harness à la fois : sa progression est gardée ici, pour qu'une
//!   interface rechargée la retrouve, avec de quoi l'annuler.
//! - Les dépendances au système (détection, trousseau, réseau, Ollama) sont injectables pour
//!   les tests.

use crate::error::{CmdResult, CommandError};
use crate::events::{self, HARDWARE_READY};
use crate::store::{self, CORRECTIONS_FILE, MEASUREMENTS_FILE, SETTINGS_FILE};
use crate::token::{KeyringStore, TokenStore};
use gram_core::harness::Availability;
use gram_core::{
    DataSource, HardwareProfile, InstallProgress, Measurement, ModelEntry, OllamaModel, Overrides,
    RankRequest, RuntimesStatus, Settings,
};
use gram_hub::{Cache, HfToken};
use gram_hw::RawHardware;
use gram_ollama::OllamaClient;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;
use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::oneshot;

type Collect = Arc<dyn Fn() -> RawHardware + Send + Sync>;
type DetectRuntimes = Arc<dyn Fn() -> RuntimesStatus + Send + Sync>;

/// Matériel détecté : données brutes, corrections, et profil qui en résulte.
struct Hardware {
    raw: RawHardware,
    profile: HardwareProfile,
}

impl Hardware {
    fn new(raw: RawHardware, overrides: &Overrides) -> Self {
        let profile = gram_hw::build_profile(&raw, overrides);
        Self { raw, profile }
    }
}

/// Catalogue en mémoire et son origine.
#[derive(Clone)]
pub struct Loaded {
    pub models: Vec<ModelEntry>,
    pub source: DataSource,
}

/// Où l'application range ses fichiers, et si elle a le droit d'aller sur le réseau.
pub struct Config {
    /// Dossier de l'application (`%LOCALAPPDATA%\fr.gram.desktop` sous Windows,
    /// `~/Library/Application Support/fr.gram.desktop` sous macOS) : `cache`, `settings.json`,
    /// `corrections.json`.
    pub dir: PathBuf,
    /// `false` dans les tests : synchronisation hors ligne, fiches non enrichies.
    pub network: bool,
    /// API locale d'Ollama (`OLLAMA_HOST`) ; `None` si l'adresse n'est pas utilisable, ou dans
    /// les tests qui n'en ont pas besoin.
    pub ollama: Option<OllamaClient>,
    /// Dossier partagé avec GRAM Harness (`fr.gram.shared`) : `models.json`, `harness.json`.
    pub shared_dir: PathBuf,
    /// Dossier des modèles d'Ollama (place sur le disque avant un ajout) ; `None` si inconnu.
    pub models_dir: Option<PathBuf>,
}

pub struct AppState {
    config: Config,
    cache: Cache,
    collect: Collect,
    detect_runtimes: DetectRuntimes,
    tokens: Box<dyn TokenStore>,
    hardware: tokio::sync::Mutex<Option<Hardware>>,
    /// Corrections (relues au lancement) et simulation en cours.
    overrides: Mutex<Overrides>,
    runtimes: tokio::sync::Mutex<Option<RuntimesStatus>>,
    catalog: tokio::sync::Mutex<Option<Arc<Loaded>>>,
    settings: RwLock<Settings>,
    /// Dernier classement demandé (repris par `export_ranking`).
    last_rank: Mutex<RankRequest>,
    /// Modèles déjà enrichis pendant cette session (pas de nouvel appel réseau).
    enriched: Mutex<HashSet<String>>,
    /// Réponses de Hugging Face avant un ajout à GRAM Harness, par (dépôt, quantification) :
    /// la vérification est refaite à chaque changement de la fiche (contexte, cas d'usage).
    availability: Mutex<HashMap<(String, String), Availability>>,
    /// Une seule synchronisation à la fois.
    pub(crate) sync_lock: tokio::sync::Mutex<()>,
    /// Une seule mesure Ollama à la fois (elle occupe la mémoire du GPU).
    pub(crate) measure_lock: tokio::sync::Mutex<()>,
    /// Mesures réelles, la plus récente d'abord (`measurements.json`).
    measurements: Mutex<Vec<Measurement>>,
    /// Un seul ajout à GRAM Harness à la fois (`harness_add`).
    pub(crate) install_lock: tokio::sync::Mutex<()>,
    /// Ajout en cours : dernière progression, et de quoi l'annuler.
    install: Mutex<Option<InstallProgress>>,
    install_cancel: Mutex<Option<oneshot::Sender<()>>>,
    /// Création de l'état (≈ lancement de l'application) et premier classement servi.
    started: Instant,
    first_rank_served: AtomicBool,
}

impl AppState {
    pub fn new(
        config: Config,
        collect: Collect,
        detect_runtimes: DetectRuntimes,
        tokens: Box<dyn TokenStore>,
    ) -> Self {
        let settings = store::load::<Settings>(&config.dir.join(SETTINGS_FILE), "paramètres");
        let corrections =
            store::load::<Overrides>(&config.dir.join(CORRECTIONS_FILE), "corrections");
        let measurements =
            store::load::<Vec<Measurement>>(&config.dir.join(MEASUREMENTS_FILE), "mesures");
        Self {
            cache: Cache::new(config.dir.join("cache")),
            config,
            collect,
            detect_runtimes,
            tokens,
            hardware: tokio::sync::Mutex::new(None),
            overrides: Mutex::new(corrections.corrections()),
            runtimes: tokio::sync::Mutex::new(None),
            catalog: tokio::sync::Mutex::new(None),
            settings: RwLock::new(settings.sanitized()),
            last_rank: Mutex::new(RankRequest::default()),
            enriched: Mutex::new(HashSet::new()),
            availability: Mutex::new(HashMap::new()),
            sync_lock: tokio::sync::Mutex::new(()),
            measure_lock: tokio::sync::Mutex::new(()),
            measurements: Mutex::new(measurements),
            install_lock: tokio::sync::Mutex::new(()),
            install: Mutex::new(None),
            install_cancel: Mutex::new(None),
            started: Instant::now(),
            first_rank_served: AtomicBool::new(false),
        }
    }

    /// Dossiers de l'application et partagé (dossier temporaire à défaut), vraie détection,
    /// trousseau du système.
    pub fn production() -> Self {
        let dir = gram_hub::cache::app_dir()
            .unwrap_or_else(|| std::env::temp_dir().join(gram_hub::cache::APP_ID));
        let shared_dir = gram_link::shared_dir()
            .unwrap_or_else(|| std::env::temp_dir().join(gram_link::SHARED_DIR_NAME));
        Self::new(
            Config {
                dir,
                network: true,
                ollama: OllamaClient::from_env(),
                shared_dir,
                models_dir: gram_ollama::models_dir(),
            },
            Arc::new(gram_hw::collect),
            Arc::new(gram_hw::detect_runtimes),
            Box::new(KeyringStore),
        )
    }

    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    pub fn network(&self) -> bool {
        self.config.network
    }

    pub fn ollama(&self) -> Option<&OllamaClient> {
        self.config.ollama.as_ref()
    }

    /// Client d'Ollama, ou l'erreur à afficher si son adresse n'est pas utilisable.
    pub fn require_ollama(&self) -> CmdResult<OllamaClient> {
        self.ollama().cloned().ok_or_else(|| {
            CommandError::msg(
                "API d'Ollama inaccessible : la variable OLLAMA_HOST désigne une adresse non \
                 prise en charge (HTTPS ou hôte inconnu).",
            )
        })
    }

    pub fn shared_dir(&self) -> &Path {
        &self.config.shared_dir
    }

    pub fn models_dir(&self) -> Option<&Path> {
        self.config.models_dir.as_deref()
    }

    // ── Matériel ────────────────────────────────────────────────────────────────────────────

    async fn collect(&self) -> CmdResult<RawHardware> {
        let collect = Arc::clone(&self.collect);
        Ok(tauri::async_runtime::spawn_blocking(move || collect()).await?)
    }

    fn overrides(&self) -> Overrides {
        self.overrides.lock().expect("verrou").clone()
    }

    /// Profil matériel (détecté au premier appel, puis gardé en mémoire), avec les corrections
    /// enregistrées. La première détection émet `hardware-ready`.
    pub async fn hardware<R: Runtime>(&self, app: &AppHandle<R>) -> CmdResult<HardwareProfile> {
        let mut slot = self.hardware.lock().await;
        if let Some(h) = slot.as_ref() {
            return Ok(h.profile.clone());
        }
        let h = Hardware::new(self.collect().await?, &self.overrides());
        let profile = h.profile.clone();
        *slot = Some(h);
        drop(slot);
        events::emit(app, HARDWARE_READY, &profile);
        Ok(profile)
    }

    /// Nouvelle détection, en gardant les corrections ; émet `hardware-ready`.
    pub async fn rescan<R: Runtime>(&self, app: &AppHandle<R>) -> CmdResult<HardwareProfile> {
        let mut slot = self.hardware.lock().await;
        let h = Hardware::new(self.collect().await?, &self.overrides());
        let profile = h.profile.clone();
        *slot = Some(h);
        drop(slot);
        events::emit(app, HARDWARE_READY, &profile);
        Ok(profile)
    }

    /// Applique les corrections et la simulation (sans nouvelle détection), et enregistre les
    /// corrections. Un échec de l'enregistrement est signalé dans les avertissements du profil :
    /// les corrections restent appliquées jusqu'à la fermeture.
    pub async fn set_overrides<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        overrides: Overrides,
    ) -> CmdResult<HardwareProfile> {
        self.hardware(app).await?;
        let mut slot = self.hardware.lock().await;
        let h = slot
            .as_mut()
            .ok_or_else(|| CommandError::msg("matériel non détecté"))?;
        let previous = std::mem::replace(
            &mut *self.overrides.lock().expect("verrou"),
            overrides.clone(),
        );
        h.profile = gram_hw::build_profile(&h.raw, &overrides);
        let corrections = overrides.corrections();
        if corrections != previous.corrections() {
            let path = self.config.dir.join(CORRECTIONS_FILE);
            if let Err(e) = store::save(&path, &corrections) {
                tracing::warn!(path = %path.display(), "corrections non enregistrées : {e}");
                h.profile.warnings.push(format!(
                    "Corrections non enregistrées ({e}) : elles seront perdues à la fermeture."
                ));
            }
        }
        Ok(h.profile.clone())
    }

    /// Profil tel que détecté, sans corrections ni simulation (dialogue « Corriger… »).
    pub async fn detected<R: Runtime>(&self, app: &AppHandle<R>) -> CmdResult<HardwareProfile> {
        self.hardware(app).await?;
        let slot = self.hardware.lock().await;
        let h = slot
            .as_ref()
            .ok_or_else(|| CommandError::msg("matériel non détecté"))?;
        Ok(gram_hw::build_profile(&h.raw, &Overrides::default()))
    }

    /// La machine réelle : corrections comprises, simulation ignorée (mesures Ollama).
    pub async fn real_profile<R: Runtime>(&self, app: &AppHandle<R>) -> CmdResult<HardwareProfile> {
        self.hardware(app).await?;
        let slot = self.hardware.lock().await;
        let h = slot
            .as_ref()
            .ok_or_else(|| CommandError::msg("matériel non détecté"))?;
        Ok(gram_hw::build_profile(
            &h.raw,
            &self.overrides().corrections(),
        ))
    }

    /// Une simulation de matériel est en cours.
    pub fn simulation(&self) -> bool {
        self.overrides.lock().expect("verrou").is_simulation()
    }

    // ── Runtimes ────────────────────────────────────────────────────────────────────────────

    /// Nouvelle détection des runtimes installés (≈ 100 ms, jusqu'à 1 s si Ollama ne répond pas),
    /// avec la liste des modèles installés dans Ollama s'il tourne.
    pub async fn detect_runtimes(&self) -> CmdResult<RuntimesStatus> {
        let detect = Arc::clone(&self.detect_runtimes);
        let mut status = tauri::async_runtime::spawn_blocking(move || detect()).await?;
        if status.ollama.running
            && let Some(client) = self.ollama()
        {
            match client.tags().await {
                Ok(models) => status.ollama_models = models,
                Err(e) => tracing::warn!("modèles installés dans Ollama : {e}"),
            }
        }
        *self.runtimes.lock().await = Some(status.clone());
        Ok(status)
    }

    /// Remplace la liste des modèles installés dans Ollama (relue avant une mesure : son API
    /// répond, il tourne donc).
    pub async fn set_ollama_models(&self, models: Vec<OllamaModel>) {
        let mut slot = self.runtimes.lock().await;
        let status = slot.get_or_insert_with(RuntimesStatus::default);
        status.ollama.installed = true;
        status.ollama.running = true;
        status.ollama_models = models;
    }

    /// Dernière détection des runtimes, sans en relancer une.
    pub async fn known_runtimes(&self) -> Option<RuntimesStatus> {
        self.runtimes.lock().await.clone()
    }

    // ── Catalogue ───────────────────────────────────────────────────────────────────────────

    /// Catalogue en mémoire ; au premier appel, celui du cache, sinon le catalogue embarqué.
    pub async fn catalog(&self) -> CmdResult<Arc<Loaded>> {
        let mut slot = self.catalog.lock().await;
        if let Some(c) = slot.as_ref() {
            return Ok(Arc::clone(c));
        }
        let cache = self.cache.clone();
        let (catalog, source) =
            tauri::async_runtime::spawn_blocking(move || gram_hub::load_offline(&cache))
                .await?
                .ok_or(gram_hub::HubError::NoCatalog)?;
        let loaded = Arc::new(Loaded {
            models: catalog.models,
            source,
        });
        *slot = Some(Arc::clone(&loaded));
        Ok(loaded)
    }

    /// Remplace le catalogue (après une synchronisation).
    pub async fn replace_catalog(&self, models: Vec<ModelEntry>, source: DataSource) {
        *self.catalog.lock().await = Some(Arc::new(Loaded { models, source }));
        self.enriched.lock().expect("verrou").clear();
        self.availability.lock().expect("verrou").clear();
    }

    /// Reporte dans le catalogue une fiche enrichie (tailles réelles, architecture).
    pub async fn update_model(&self, entry: ModelEntry) {
        let mut slot = self.catalog.lock().await;
        if let Some(c) = slot.as_mut() {
            // Copie le catalogue seulement s'il est partagé (un classement en cours le lit).
            let c = Arc::make_mut(c);
            if let Some(m) = c.models.iter_mut().find(|m| m.repo_id == entry.repo_id) {
                self.enriched
                    .lock()
                    .expect("verrou")
                    .insert(entry.repo_id.clone());
                *m = entry;
            }
        }
    }

    pub fn is_enriched(&self, repo_id: &str) -> bool {
        self.enriched.lock().expect("verrou").contains(repo_id)
    }

    /// Réponse de Hugging Face déjà reçue pour `label` de `repo_id` (oubliée à la
    /// synchronisation du catalogue).
    pub fn availability(&self, repo_id: &str, label: &str) -> Option<Availability> {
        self.availability
            .lock()
            .expect("verrou")
            .get(&(repo_id.to_owned(), label.to_owned()))
            .cloned()
    }

    pub fn set_availability(&self, repo_id: &str, label: &str, a: Availability) {
        self.availability
            .lock()
            .expect("verrou")
            .insert((repo_id.to_owned(), label.to_owned()), a);
    }

    // ── Paramètres, token, dernier classement ───────────────────────────────────────────────

    pub fn settings(&self) -> Settings {
        self.settings.read().expect("verrou").clone()
    }

    /// Enregistre les paramètres (assainis) et les renvoie.
    pub fn save_settings(&self, settings: Settings) -> CmdResult<Settings> {
        let settings = settings.sanitized();
        let path = self.config.dir.join(SETTINGS_FILE);
        store::save(&path, &settings).map_err(|e| {
            CommandError::msg(format!(
                "Paramètres non enregistrés dans {} : {e}",
                path.display()
            ))
        })?;
        *self.settings.write().expect("verrou") = settings.clone();
        Ok(settings)
    }

    pub fn tokens(&self) -> &dyn TokenStore {
        self.tokens.as_ref()
    }

    /// Token pour les appels Hugging Face ; un trousseau illisible n'empêche pas de continuer
    /// sans token.
    pub fn hf_token(&self) -> Option<HfToken> {
        match self.tokens.get() {
            Ok(t) => t.and_then(HfToken::new),
            Err(e) => {
                tracing::warn!("{e} : appels Hugging Face sans token");
                None
            }
        }
    }

    // ── Mesures Ollama ──────────────────────────────────────────────────────────────────────

    pub fn measurements(&self) -> Vec<Measurement> {
        self.measurements.lock().expect("verrou").clone()
    }

    /// Ajoute une mesure (elle remplace celle du même modèle au même contexte), puis enregistre
    /// la liste. Un échec de l'enregistrement est journalisé : la mesure reste en mémoire.
    ///
    /// Une mesure fiable est reportée dans le manifeste de GRAM Harness, sur les modèles
    /// dérivés dont la base est le modèle mesuré.
    pub fn record_measurement(&self, m: Measurement) {
        let (model, tok_s, reliable) = (m.ollama_model.clone(), m.tok_s, m.reliable);
        let list = {
            let mut list = self.measurements.lock().expect("verrou");
            gram_core::calibration::record(&mut list, m);
            list.clone()
        };
        self.save_measurements(&list);
        if reliable
            && let Err(e) = gram_link::update_manifest(self.shared_dir(), |manifest| {
                manifest.record_measurement(&model, tok_s)
            })
        {
            tracing::warn!("mesure non reportée dans le manifeste de GRAM Harness : {e}");
        }
    }

    pub fn clear_measurements(&self) {
        self.measurements.lock().expect("verrou").clear();
        self.save_measurements(&[]);
    }

    fn save_measurements(&self, list: &[Measurement]) {
        let path = self.config.dir.join(MEASUREMENTS_FILE);
        if let Err(e) = store::save(&path, &list) {
            tracing::warn!(path = %path.display(), "mesures non enregistrées : {e}");
        }
    }

    // ── Ajout à GRAM Harness ────────────────────────────────────────────────────────────────

    /// Progression de l'ajout en cours.
    pub fn install_progress(&self) -> Option<InstallProgress> {
        self.install.lock().expect("verrou").clone()
    }

    pub fn set_install_progress(&self, progress: InstallProgress) {
        *self.install.lock().expect("verrou") = Some(progress);
    }

    /// Note le début d'un ajout ; la fin (le garde rendu détruit) efface sa progression.
    /// `cancel` reçoit l'annulation demandée par `harness_cancel`.
    pub fn begin_install(
        &self,
        progress: InstallProgress,
        cancel: oneshot::Sender<()>,
    ) -> InstallGuard<'_> {
        self.set_install_progress(progress);
        *self.install_cancel.lock().expect("verrou") = Some(cancel);
        InstallGuard(self)
    }

    /// Demande l'annulation de l'ajout en cours ; `false` s'il n'y en a pas.
    pub fn cancel_install(&self) -> bool {
        let sender = self.install_cancel.lock().expect("verrou").take();
        sender.is_some_and(|s| s.send(()).is_ok())
    }

    pub fn remember_rank(&self, request: &RankRequest) {
        *self.last_rank.lock().expect("verrou") = request.clone();
    }

    pub fn last_rank(&self) -> RankRequest {
        self.last_rank.lock().expect("verrou").clone()
    }

    /// Journalise, une seule fois, le délai entre le lancement et le premier classement servi
    /// (objectif : moins de 5 s avec le cache).
    pub fn note_rank_served(&self) {
        if !self.first_rank_served.swap(true, Ordering::Relaxed) {
            let ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
            tracing::info!(ms, "premier classement servi depuis le lancement");
        }
    }
}

/// Ajout à GRAM Harness en cours ; sa destruction efface la progression et l'annulation.
pub struct InstallGuard<'a>(&'a AppState);

impl Drop for InstallGuard<'_> {
    fn drop(&mut self) {
        *self.0.install.lock().expect("verrou") = None;
        *self.0.install_cancel.lock().expect("verrou") = None;
    }
}

/// Démarrage : matériel, runtimes et catalogue en parallèle, pour que le premier classement
/// soit immédiat.
pub async fn startup<R: Runtime>(app: AppHandle<R>) {
    let state = app.state::<AppState>();
    let (hw, rt, catalog) = tokio::join!(
        state.hardware(&app),
        state.detect_runtimes(),
        state.catalog()
    );
    if let Err(e) = hw {
        tracing::error!("détection du matériel : {e}");
    }
    if let Err(e) = rt {
        tracing::warn!("détection des runtimes : {e}");
    }
    match catalog {
        Ok(c) => {
            tracing::info!(models = c.models.len(), source = ?c.source.kind, "catalogue chargé")
        }
        Err(e) => tracing::error!("catalogue : {e}"),
    }
}
