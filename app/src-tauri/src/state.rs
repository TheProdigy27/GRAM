//! État partagé de l'application (`tauri::State<AppState>`) : matériel, runtimes, catalogue et
//! paramètres.
//!
//! - Le matériel est détecté une fois (au démarrage), dans `spawn_blocking` : WMI a besoin d'un
//!   thread initialisé COM. Les corrections et la simulation repartent des données brutes.
//! - Le catalogue est chargé hors ligne au démarrage (cache, sinon catalogue embarqué), puis
//!   remplacé par `sync_catalog`. Les fiches enrichies à la demande y sont reportées.
//! - Les dépendances au système (détection, trousseau, réseau) sont injectables pour les tests.

use crate::error::{CmdResult, CommandError};
use crate::events::{self, HARDWARE_READY};
use crate::settings_store;
use crate::token::{KeyringStore, TokenStore};
use gram_core::{
    DataSource, HardwareProfile, ModelEntry, Overrides, RankRequest, RuntimesStatus, Settings,
};
use gram_hub::{Cache, HfToken};
use gram_hw::RawHardware;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;
use tauri::{AppHandle, Manager, Runtime};

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
    pub cache: Cache,
    pub settings_path: PathBuf,
    /// `false` dans les tests : synchronisation hors ligne, fiches non enrichies.
    pub network: bool,
}

pub struct AppState {
    config: Config,
    collect: Collect,
    detect_runtimes: DetectRuntimes,
    tokens: Box<dyn TokenStore>,
    hardware: tokio::sync::Mutex<Option<Hardware>>,
    runtimes: tokio::sync::Mutex<Option<RuntimesStatus>>,
    catalog: tokio::sync::Mutex<Option<Arc<Loaded>>>,
    settings: RwLock<Settings>,
    /// Dernier classement demandé (repris par `export_ranking`).
    last_rank: Mutex<RankRequest>,
    /// Modèles déjà enrichis pendant cette session (pas de nouvel appel réseau).
    enriched: Mutex<HashSet<String>>,
    /// Une seule synchronisation à la fois.
    pub(crate) sync_lock: tokio::sync::Mutex<()>,
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
        let settings = settings_store::load(&config.settings_path);
        Self {
            config,
            collect,
            detect_runtimes,
            tokens,
            hardware: tokio::sync::Mutex::new(None),
            runtimes: tokio::sync::Mutex::new(None),
            catalog: tokio::sync::Mutex::new(None),
            settings: RwLock::new(settings),
            last_rank: Mutex::new(RankRequest::default()),
            enriched: Mutex::new(HashSet::new()),
            sync_lock: tokio::sync::Mutex::new(()),
            started: Instant::now(),
            first_rank_served: AtomicBool::new(false),
        }
    }

    /// `%LOCALAPPDATA%\GRAM\` (dossier temporaire à défaut), vraie détection, trousseau Windows.
    pub fn production() -> Self {
        let dir = gram_hub::cache::app_dir().unwrap_or_else(|| std::env::temp_dir().join("GRAM"));
        Self::new(
            Config {
                cache: Cache::new(dir.join("cache")),
                settings_path: dir.join("settings.json"),
                network: true,
            },
            Arc::new(gram_hw::collect),
            Arc::new(gram_hw::detect_runtimes),
            Box::new(KeyringStore),
        )
    }

    pub fn cache(&self) -> &Cache {
        &self.config.cache
    }

    pub fn network(&self) -> bool {
        self.config.network
    }

    // ── Matériel ────────────────────────────────────────────────────────────────────────────

    async fn collect(&self) -> CmdResult<RawHardware> {
        let collect = Arc::clone(&self.collect);
        Ok(tauri::async_runtime::spawn_blocking(move || collect()).await?)
    }

    /// Profil matériel (détecté au premier appel, puis gardé en mémoire). La première détection
    /// émet `hardware-ready`.
    pub async fn hardware<R: Runtime>(&self, app: &AppHandle<R>) -> CmdResult<HardwareProfile> {
        let mut slot = self.hardware.lock().await;
        if let Some(h) = slot.as_ref() {
            return Ok(h.profile.clone());
        }
        let h = Hardware::new(self.collect().await?, &Overrides::default());
        let profile = h.profile.clone();
        *slot = Some(h);
        drop(slot);
        events::emit(app, HARDWARE_READY, &profile);
        Ok(profile)
    }

    /// Nouvelle détection, en gardant les corrections ; émet `hardware-ready`.
    pub async fn rescan<R: Runtime>(&self, app: &AppHandle<R>) -> CmdResult<HardwareProfile> {
        let mut slot = self.hardware.lock().await;
        let overrides = slot
            .as_ref()
            .map(|h| h.profile.overrides.clone())
            .unwrap_or_default();
        let h = Hardware::new(self.collect().await?, &overrides);
        let profile = h.profile.clone();
        *slot = Some(h);
        drop(slot);
        events::emit(app, HARDWARE_READY, &profile);
        Ok(profile)
    }

    /// Applique les corrections et la simulation (sans nouvelle détection).
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
        h.profile = gram_hw::build_profile(&h.raw, &overrides);
        Ok(h.profile.clone())
    }

    // ── Runtimes ────────────────────────────────────────────────────────────────────────────

    /// Nouvelle détection des runtimes installés (≈ 100 ms, jusqu'à 1 s si Ollama ne répond pas).
    pub async fn detect_runtimes(&self) -> CmdResult<RuntimesStatus> {
        let detect = Arc::clone(&self.detect_runtimes);
        let status = tauri::async_runtime::spawn_blocking(move || detect()).await?;
        *self.runtimes.lock().await = Some(status.clone());
        Ok(status)
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
        let cache = self.config.cache.clone();
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

    // ── Paramètres, token, dernier classement ───────────────────────────────────────────────

    pub fn settings(&self) -> Settings {
        self.settings.read().expect("verrou").clone()
    }

    /// Enregistre les paramètres (assainis) et les renvoie.
    pub fn save_settings(&self, settings: Settings) -> CmdResult<Settings> {
        let settings = settings.sanitized();
        settings_store::save(&self.config.settings_path, &settings).map_err(|e| {
            CommandError::msg(format!(
                "paramètres non enregistrés dans {} : {e}",
                self.config.settings_path.display()
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
