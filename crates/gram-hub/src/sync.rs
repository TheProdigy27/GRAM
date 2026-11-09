//! Synchronisation du catalogue (§7.3 du plan).
//!
//! 1. Listes : pour chaque tranche de taille utile et chaque pipeline, top 100 par
//!    téléchargements et top 50 par tendance.
//! 2. Filtrage : `gguf.total` connu, sous la borne haute, assez téléchargé (ou en tendance).
//! 3. Dédoublonnage par modèle de base ([`crate::dedup`]).
//! 4. Pré-classement, puis détail (`?blobs=true`, tailles réelles) des N premiers groupes ;
//!    le cache par `repo@sha` sert pour tous les groupes, sans appel réseau.
//! 5. Architecture (en-tête GGUF, sinon `config.json`) des modèles détaillés.
//!
//! Hors ligne, ou si Hugging Face est injoignable : cache, puis catalogue embarqué.

use crate::api::{self, ApiModel, ListQuery, SizeBucket, Sort};
use crate::cache::{Cache, valid_repo_id};
use crate::catalog::{
    ArchRecord, CATALOG_FORMAT, Catalog, RepoDetail, apply_arch, apply_detail, entry_from_group,
};
use crate::config::parse_config;
use crate::dedup::{Group, ListedRepo, Uploaders, group_repos};
use crate::error::{HubError, Result};
use crate::gguf::{self, Parsed};
use crate::http::{Hub, HubEvent, Request, Transport};
use crate::snapshot;
use chrono::Utc;
use futures_util::future::join_all;
use gram_core::{
    DataSource, DataSourceKind, HardwareProfile, ModelEntry, Pipeline, SyncPhase, SyncProgress,
    SyncSummary,
};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};

/// Lectures successives de l'en-tête GGUF : 64 Kio suffisent presque toujours (les clés
/// d'architecture précèdent le tokenizer), 32 Mio au maximum (§7.4).
const HEADER_READS: [u64; 3] = [64 << 10, 8 << 20, 32 << 20];

#[derive(Debug, Clone)]
pub struct SyncOptions {
    /// Ignore le catalogue en cache, même récent.
    pub force: bool,
    /// Aucun appel réseau : cache, puis catalogue embarqué.
    pub offline: bool,
    /// Borne haute en paramètres ([`max_params_for`]) ; `None` : toutes les tranches.
    pub max_params: Option<u64>,
    /// Groupes détaillés (tailles réelles) à la synchronisation (§12 : 150).
    pub detail_groups: usize,
    /// Téléchargements minimum, sauf dépôt en tendance (§12 : 500).
    pub min_downloads: u64,
    pub top_downloads: u32,
    pub top_trending: u32,
    /// Durée de vie du catalogue en cache (§12 : 24 h).
    pub ttl: chrono::Duration,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            force: false,
            offline: false,
            max_params: None,
            detail_groups: 150,
            min_downloads: 500,
            top_downloads: 100,
            top_trending: 50,
            ttl: chrono::Duration::hours(24),
        }
    }
}

/// Octets par paramètre de la quantification la plus agressive retenue (Q2_K).
const Q2K_BYTES_PER_PARAM: f64 = 0.37;

/// Borne haute du catalogue (§7.3) : plus gros modèle plausible en Q2_K dans la VRAM du GPU
/// discret et la RAM. Calculée sur les totaux (90 % de la VRAM, 75 % de la RAM), pour ne pas
/// dépendre des applications ouvertes au moment de la synchronisation.
pub fn max_params_for(hw: &HardwareProfile) -> u64 {
    let vram = hw
        .gpu
        .as_ref()
        .filter(|g| !g.integrated)
        .map_or(0.0, |g| g.vram_total as f64 * 0.9);
    let ram = hw.ram.total as f64 * 0.75;
    ((vram + ram) / Q2K_BYTES_PER_PARAM) as u64
}

#[derive(Debug, Clone)]
pub struct SyncOutcome {
    pub catalog: Catalog,
    pub summary: SyncSummary,
}

/// Catalogue disponible sans réseau : cache, sinon catalogue embarqué.
pub fn load_offline(cache: &Cache) -> Option<(Catalog, DataSource)> {
    offline_catalog(cache, snapshot::seed_catalog())
}

fn offline_catalog(cache: &Cache, seed: Option<&Catalog>) -> Option<(Catalog, DataSource)> {
    let (catalog, kind) = match cache.load_catalog() {
        Some(c) => (c, DataSourceKind::Cache),
        None => (seed?.clone(), DataSourceKind::Snapshot),
    };
    let source = DataSource {
        kind,
        fetched_at: catalog.fetched_at,
    };
    Some((catalog, source))
}

pub async fn sync<T: Transport>(
    hub: &Hub<T>,
    cache: &Cache,
    opts: &SyncOptions,
) -> Result<SyncOutcome> {
    sync_with_seed(hub, cache, opts, snapshot::seed_catalog()).await
}

async fn sync_with_seed<T: Transport>(
    hub: &Hub<T>,
    cache: &Cache,
    opts: &SyncOptions,
    seed: Option<&Catalog>,
) -> Result<SyncOutcome> {
    let start = tokio::time::Instant::now();
    let (api0, res0) = (hub.api_calls(), hub.resolver_calls());
    let finish = |catalog: Catalog, kind: DataSourceKind, warnings: Vec<String>| {
        let summary = SyncSummary {
            source: DataSource {
                kind,
                fetched_at: catalog.fetched_at,
            },
            repos: catalog.repo_count() as u32,
            models: catalog.models.len() as u32,
            detailed: catalog
                .models
                .iter()
                .filter(|m| !m.variants.is_empty())
                .count() as u32,
            with_arch: catalog.models.iter().filter(|m| m.arch.is_some()).count() as u32,
            api_calls: hub.api_calls() - api0,
            resolver_calls: hub.resolver_calls() - res0,
            rate_limit: hub.rate_limit(),
            duration_ms: start.elapsed().as_millis() as u64,
            warnings,
        };
        SyncOutcome { catalog, summary }
    };
    let offline = |warnings: Vec<String>| match offline_catalog(cache, seed) {
        Some((c, source)) => Ok(finish(c, source.kind, warnings)),
        None => Err(HubError::NoCatalog),
    };

    if opts.offline {
        return offline(vec![]);
    }
    if !opts.force
        && let Some(c) = cache.load_catalog()
        && c.covers(opts.max_params)
        && Utc::now() - c.fetched_at < opts.ttl
    {
        tracing::info!("catalogue en cache récent : pas de synchronisation");
        return Ok(finish(c, DataSourceKind::Cache, vec![]));
    }

    match sync_online(hub, cache, opts).await {
        Ok((catalog, mut warnings)) => {
            if let Err(e) = cache.save_catalog(&catalog) {
                warnings.push(format!("Catalogue non enregistré dans le cache : {e}"));
            }
            Ok(finish(catalog, DataSourceKind::Online, warnings))
        }
        Err(e) if e.is_global() => {
            tracing::warn!("synchronisation impossible, repli hors ligne : {e}");
            offline(vec![format!("{e}. Catalogue hors ligne utilisé.")])
        }
        Err(e) => Err(e),
    }
}

fn list_queries(opts: &SyncOptions) -> Vec<ListQuery> {
    let mut out = Vec::new();
    for bucket in SizeBucket::up_to(opts.max_params) {
        for pipeline in [Pipeline::TextGeneration, Pipeline::ImageTextToText] {
            for (sort, limit) in [
                (Sort::Downloads, opts.top_downloads),
                (Sort::Trending, opts.top_trending),
            ] {
                if limit > 0 {
                    out.push(ListQuery {
                        pipeline,
                        bucket,
                        sort,
                        limit,
                    });
                }
            }
        }
    }
    out
}

/// Fusionne une liste dans les dépôts déjà vus (tendance, vision).
fn merge_list(listed: &mut HashMap<String, ListedRepo>, q: &ListQuery, models: Vec<ApiModel>) {
    for model in models {
        let trending = q.sort == Sort::Trending && model.trending_score.unwrap_or(0.0) > 0.0;
        let vision = q.pipeline == Pipeline::ImageTextToText;
        listed
            .entry(model.id.clone())
            .and_modify(|r| {
                r.trending |= trending;
                r.vision |= vision;
            })
            .or_insert(ListedRepo {
                model,
                trending,
                vision,
            });
    }
}

/// Architectures de modèles de brouillon (décodage spéculatif), inutilisables seuls. Hugging
/// Face décrit parfois tout un dépôt à partir d'un tel fichier : paramètres faux.
const DRAFT_ARCHITECTURES: &[&str] = &["dflash", "eagle", "eagle3", "medusa"];

fn keep(r: &ListedRepo, opts: &SyncOptions) -> bool {
    let Some(params) = r.model.params_total() else {
        return false;
    };
    valid_repo_id(&r.model.id)
        && r.model.sha.is_some()
        && !r
            .model
            .architecture()
            .is_some_and(|a| DRAFT_ARCHITECTURES.contains(&a))
        && opts.max_params.is_none_or(|max| params <= max)
        && (r.downloads() >= opts.min_downloads || r.trending)
}

/// Ordre de détail des groupes, en attendant le classement par estimation (phases 3 et 4) :
/// chacune des tranches de taille à tour de rôle, des plus gros modèles aux plus petits (les
/// grosses tranches, moins téléchargées, ne sont donc pas écrasées par les petits modèles),
/// et dans chaque tranche [`bucket_order`].
fn prerank(groups: &[Group]) -> Vec<usize> {
    let mut buckets: BTreeMap<std::cmp::Reverse<u32>, Vec<usize>> = BTreeMap::new();
    for (i, g) in groups.iter().enumerate() {
        let params = g.canonical().model.params_total().unwrap_or(0);
        buckets
            .entry(std::cmp::Reverse(SizeBucket::of(params).min_b))
            .or_default()
            .push(i);
    }
    let mut lists: Vec<VecDeque<usize>> = buckets
        .into_values()
        .map(|members| bucket_order(groups, members))
        .collect();
    let mut order = Vec::with_capacity(groups.len());
    while lists.iter().any(|l| !l.is_empty()) {
        for l in &mut lists {
            if let Some(i) = l.pop_front() {
                order.push(i);
            }
        }
    }
    order
}

/// Dans une tranche : deux groupes pris par téléchargements cumulés (modèles établis), puis
/// un par score de tendance (sorties récentes), et ainsi de suite.
fn bucket_order(groups: &[Group], members: Vec<usize>) -> VecDeque<usize> {
    let mut by_downloads = members.clone();
    by_downloads.sort_by(|&a, &b| {
        let (ga, gb) = (&groups[a], &groups[b]);
        gb.downloads()
            .cmp(&ga.downloads())
            .then(ga.key.cmp(&gb.key))
    });
    let mut by_trend: Vec<usize> = members
        .into_iter()
        .filter(|&i| groups[i].trending_score() > 0.0)
        .collect();
    by_trend.sort_by(|&a, &b| {
        let (ga, gb) = (&groups[a], &groups[b]);
        gb.trending_score()
            .total_cmp(&ga.trending_score())
            .then(gb.downloads().cmp(&ga.downloads()))
            .then(ga.key.cmp(&gb.key))
    });
    let mut used = vec![false; groups.len()];
    let mut next = |list: &mut std::vec::IntoIter<usize>| {
        let i = list.find(|&i| !used[i])?;
        used[i] = true;
        Some(i)
    };
    let (mut downloads, mut trend) = (by_downloads.into_iter(), by_trend.into_iter());
    let mut out = VecDeque::new();
    for turn in 0.. {
        let pick = if turn % 3 == 2 {
            next(&mut trend).or_else(|| next(&mut downloads))
        } else {
            next(&mut downloads).or_else(|| next(&mut trend))
        };
        match pick {
            Some(i) => out.push_back(i),
            None => break,
        }
    }
    out
}

struct Progress<'a, T> {
    hub: &'a Hub<T>,
    phase: SyncPhase,
    done: AtomicU32,
    total: u32,
}

impl<'a, T: Transport> Progress<'a, T> {
    fn start(hub: &'a Hub<T>, phase: SyncPhase, total: usize) -> Self {
        let p = Self {
            hub,
            phase,
            done: AtomicU32::new(0),
            total: total as u32,
        };
        p.emit(0);
        p
    }

    fn step(&self) {
        let done = self.done.fetch_add(1, Ordering::Relaxed) + 1;
        self.emit(done);
    }

    fn emit(&self, done: u32) {
        self.hub.emit(HubEvent::Progress(SyncProgress {
            phase: self.phase,
            done,
            total: self.total,
        }));
    }
}

async fn sync_online<T: Transport>(
    hub: &Hub<T>,
    cache: &Cache,
    opts: &SyncOptions,
) -> Result<(Catalog, Vec<String>)> {
    let mut warnings = Vec::new();

    // 1. Listes. La première requête part seule : si le réseau est coupé, on le sait tout
    //    de suite, sans attendre l'échec de toutes les autres.
    let queries = list_queries(opts);
    let progress = &Progress::start(hub, SyncPhase::Listing, queries.len());
    let fetch = |q: ListQuery| async move {
        let r = hub
            .get_paginated::<ApiModel>(api::list_url(hub.base(), &q), q.limit as usize)
            .await;
        progress.step();
        (q, r)
    };
    let mut listed: HashMap<String, ListedRepo> = HashMap::new();
    let Some((&first, rest)) = queries.split_first() else {
        return Err(HubError::NoCatalog);
    };
    let (q, r) = fetch(first).await;
    merge_list(&mut listed, &q, r?);
    for (q, r) in join_all(rest.iter().map(|&q| fetch(q))).await {
        match r {
            Ok(models) => merge_list(&mut listed, &q, models),
            Err(e) => {
                tracing::warn!(?q, "liste indisponible : {e}");
                warnings.push(format!(
                    "Liste {} {} ({:?}) indisponible : {e}",
                    api::pipeline_tag(q.pipeline),
                    q.bucket.param(),
                    q.sort
                ));
            }
        }
    }

    // 2–3. Filtrage et dédoublonnage.
    let repos: Vec<ListedRepo> = listed.into_values().filter(|r| keep(r, opts)).collect();
    tracing::info!(repos = repos.len(), "dépôts GGUF retenus");
    let groups = group_repos(repos, Uploaders::embedded());

    // 4. Pré-classement et détails.
    let mut entries: Vec<ModelEntry> = prerank(&groups)
        .into_iter()
        .map(|i| entry_from_group(&groups[i]))
        .collect();
    let mut dropped = vec![false; entries.len()];
    let mut to_fetch = Vec::new();
    for (i, e) in entries.iter_mut().enumerate() {
        match cache.load_detail(&e.repo_id, &e.sha) {
            Some(d) => dropped[i] = !apply_detail(e, &d),
            None if i < opts.detail_groups => to_fetch.push(i),
            None => {}
        }
    }
    let progress = Progress::start(hub, SyncPhase::Details, to_fetch.len());
    let results = join_all(to_fetch.iter().map(|&i| {
        let repo = entries[i].repo_id.clone();
        let progress = &progress;
        async move {
            let r = fetch_detail(hub, &repo).await;
            progress.step();
            (i, r)
        }
    }))
    .await;
    for (i, r) in results {
        match r {
            Ok(d) => {
                if let Err(e) = cache.save_detail(&d) {
                    tracing::warn!("détail non mis en cache : {e}");
                }
                dropped[i] = !apply_detail(&mut entries[i], &d);
            }
            Err(e) if e.is_global() => return Err(e),
            Err(e) => warnings.push(format!(
                "Détail de {} indisponible : {e}",
                entries[i].repo_id
            )),
        }
    }

    // 5. Architecture des modèles détaillés.
    let mut to_probe = Vec::new();
    for (i, e) in entries.iter_mut().enumerate() {
        if e.variants.is_empty() || dropped[i] {
            continue;
        }
        match cache.load_arch(&e.repo_id, &e.sha) {
            Some(a) => apply_arch(e, &a),
            None => to_probe.push(i),
        }
    }
    let progress = Progress::start(hub, SyncPhase::Architecture, to_probe.len());
    let results = join_all(to_probe.iter().map(|&i| {
        let entry = &entries[i];
        let progress = &progress;
        async move {
            let r = fetch_arch(hub, entry).await;
            progress.step();
            (i, r)
        }
    }))
    .await;
    for (i, r) in results {
        match r {
            Ok(Some(a)) => {
                let e = &mut entries[i];
                if let Err(err) = cache.save_arch(&e.repo_id, &e.sha, &a) {
                    tracing::warn!("architecture non mise en cache : {err}");
                }
                apply_arch(e, &a);
            }
            Ok(None) => tracing::debug!(repo = entries[i].repo_id, "architecture inconnue"),
            Err(e) if e.is_global() => return Err(e),
            Err(e) => tracing::debug!(repo = entries[i].repo_id, "architecture : {e}"),
        }
    }

    // Un dépôt détaillé sans aucune quantification exploitable n'est pas un modèle GGUF.
    let models = entries
        .into_iter()
        .zip(dropped)
        .filter_map(|(e, drop)| (!drop).then_some(e))
        .collect();
    let catalog = Catalog {
        format: CATALOG_FORMAT,
        fetched_at: Utc::now(),
        max_params: opts.max_params,
        models,
    };
    Ok((catalog, warnings))
}

async fn fetch_detail<T: Transport>(hub: &Hub<T>, repo: &str) -> Result<RepoDetail> {
    let url = api::detail_url(hub.base(), repo);
    let m: ApiModel = hub.get_json(url.clone()).await?;
    RepoDetail::from_api(&m).ok_or_else(|| HubError::parse(url, "sha absent"))
}

/// Architecture d'un modèle détaillé : en-tête GGUF du plus petit fichier (première partie
/// s'il est découpé), sinon `config.json` du modèle de base. `Ok(None)` si rien n'est lisible.
async fn fetch_arch<T: Transport>(hub: &Hub<T>, e: &ModelEntry) -> Result<Option<ArchRecord>> {
    if let Some(file) = e.variants.first().and_then(|v| v.files.first()) {
        let url = api::resolve_url(hub.base(), &e.repo_id, &e.sha, file);
        for len in HEADER_READS {
            let resp = match hub.get(&Request::resolver(url.clone(), Some(0..len))).await {
                Ok(r) => r,
                Err(err) if err.is_global() => return Err(err),
                Err(err) => {
                    // 401 sur un dépôt protégé, par exemple.
                    tracing::debug!(url, "en-tête GGUF inaccessible : {err}");
                    break;
                }
            };
            match gguf::parse(&resp.body) {
                Ok(Parsed::Complete(m) | Parsed::Partial(m)) => {
                    if let Some(arch) = m.arch_info() {
                        return Ok(Some(ArchRecord {
                            arch,
                            params_active: m.params_active(e.params_total),
                            ctx_train: m.context_length.and_then(|c| u32::try_from(c).ok()),
                        }));
                    }
                    break;
                }
                // Le fichier est plus long que ce qu'on a lu : relire plus loin.
                Ok(Parsed::NeedMore) if resp.body.len() as u64 >= len => continue,
                Ok(Parsed::NeedMore) => break,
                Err(err) => {
                    tracing::debug!(url, "en-tête GGUF : {err}");
                    break;
                }
            }
        }
    }
    if let Some(base) = e.base_model.as_deref().filter(|b| valid_repo_id(b)) {
        let url = api::resolve_url(hub.base(), base, "main", "config.json");
        match hub.get(&Request::resolver(url, None)).await {
            Ok(resp) => {
                if let Some(c) = parse_config(&resp.body) {
                    return Ok(Some(ArchRecord {
                        arch: c.arch,
                        params_active: None,
                        ctx_train: c.ctx_train,
                    }));
                }
            }
            Err(err) if err.is_global() => return Err(err),
            // Modèle de base protégé (401 sans token) : fréquent.
            Err(err) => tracing::debug!(base, "config.json : {err}"),
        }
    }
    Ok(None)
}

/// Charge à la demande les tailles réelles et l'architecture d'un modèle (fiche détail,
/// commande `get_model_detail`).
pub async fn enrich<T: Transport>(
    hub: &Hub<T>,
    cache: &Cache,
    entry: &ModelEntry,
) -> Result<ModelEntry> {
    let mut e = entry.clone();
    let detail = match cache.load_detail(&e.repo_id, &e.sha) {
        Some(d) => d,
        None => {
            let d = fetch_detail(hub, &e.repo_id).await?;
            if let Err(err) = cache.save_detail(&d) {
                tracing::warn!("détail non mis en cache : {err}");
            }
            d
        }
    };
    apply_detail(&mut e, &detail);
    if e.arch.is_none() && !e.variants.is_empty() {
        let arch = match cache.load_arch(&e.repo_id, &e.sha) {
            Some(a) => Some(a),
            None => {
                let a = fetch_arch(hub, &e).await?;
                if let Some(a) = &a
                    && let Err(err) = cache.save_arch(&e.repo_id, &e.sha, a)
                {
                    tracing::warn!("architecture non mise en cache : {err}");
                }
                a
            }
        };
        if let Some(a) = arch {
            apply_arch(&mut e, &a);
        }
    }
    Ok(e)
}

#[cfg(test)]
mod tests;
