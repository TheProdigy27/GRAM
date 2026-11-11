//! `gram-cli sync` et `gram-cli snapshot` : catalogue Hugging Face.

use anyhow::{Context, Result};
use chrono::Local;
use gram_core::fmt::{gb, num, thousands};
use gram_core::{DataSource, DataSourceKind, ModelEntry, SyncPhase, SyncSummary};
use gram_hub::{
    Cache, Catalog, HfToken, Hub, HubEvent, ReqwestTransport, SyncOptions, api::SizeBucket,
    snapshot,
};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

pub struct SyncArgs {
    pub force: bool,
    pub offline: bool,
    pub all_sizes: bool,
    pub json: bool,
    pub cache_dir: Option<PathBuf>,
}

pub(crate) fn cache(dir: Option<PathBuf>) -> Result<Cache> {
    let root = dir
        .or_else(Cache::default_root)
        .context("dossier des données locales introuvable : préciser --cache-dir")?;
    Ok(Cache::new(root))
}

fn phase(p: SyncPhase) -> &'static str {
    match p {
        SyncPhase::Listing => "listes",
        SyncPhase::Details => "détails",
        SyncPhase::Architecture => "architecture",
    }
}

/// Client Hugging Face ; le token éventuel vient de la variable `HF_TOKEN` (jamais affiché).
fn hub() -> Result<Hub<ReqwestTransport>> {
    let token = std::env::var("HF_TOKEN").ok().and_then(HfToken::new);
    if token.is_some() {
        eprintln!("Token Hugging Face : configuré (HF_TOKEN)");
    }
    Ok(Hub::new(ReqwestTransport::new(token)?).on_event(|e| {
        if let HubEvent::Progress(p) = e
            && p.total > 0
        {
            let mut err = std::io::stderr();
            let _ = write!(err, "\r  {:<13}{:>4}/{}", phase(p.phase), p.done, p.total);
            if p.done == p.total {
                let _ = writeln!(err);
            }
        }
    }))
}

pub(crate) fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?)
}

pub fn run(args: SyncArgs) -> Result<()> {
    let cache = cache(args.cache_dir)?;
    let max_params = if args.all_sizes || args.offline {
        None
    } else {
        let hw = gram_hw::detect();
        Some(gram_hub::max_params_for(&hw))
    };
    if !args.offline {
        let buckets = SizeBucket::up_to(max_params).len();
        match max_params {
            Some(m) => eprintln!(
                "Synchronisation Hugging Face : modèles jusqu'à {} milliards de paramètres ({buckets} tranches)",
                thousands(m / 1_000_000_000)
            ),
            None => {
                eprintln!("Synchronisation Hugging Face : toutes les tailles ({buckets} tranches)")
            }
        }
    }
    let opts = SyncOptions {
        force: args.force,
        offline: args.offline,
        max_params,
        ..SyncOptions::default()
    };
    let hub = hub()?;
    let out = runtime()?.block_on(gram_hub::sync(&hub, &cache, &opts))?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&out.summary)?);
    } else {
        print!("{}", render(&out.summary, &out.catalog, cache.root()));
    }
    Ok(())
}

pub fn snapshot(output: &Path, cache_dir: Option<PathBuf>) -> Result<()> {
    let cache = cache(cache_dir)?;
    eprintln!("Snapshot du catalogue : toutes les tailles, indépendamment de cette machine");
    let opts = SyncOptions {
        force: true,
        max_params: None,
        ..SyncOptions::default()
    };
    let hub = hub()?;
    let out = runtime()?.block_on(gram_hub::sync(&hub, &cache, &opts))?;
    anyhow::ensure!(
        out.summary.source.kind == DataSourceKind::Online,
        "Hugging Face injoignable : snapshot non généré ({})",
        out.summary.warnings.join(" ; ")
    );
    let bytes = snapshot::to_seed_json(&out.catalog)?;
    std::fs::write(output, &bytes).with_context(|| format!("écriture de {}", output.display()))?;
    print!("{}", render(&out.summary, &out.catalog, cache.root()));
    println!(
        "Snapshot    {} ({} Ko)",
        output.display(),
        thousands(bytes.len() as u64 / 1024)
    );
    Ok(())
}

const LABEL_WIDTH: usize = 12;

fn line(out: &mut String, label: &str, text: &str) {
    let _ = writeln!(out, "{label:<LABEL_WIDTH$}{text}");
}

/// Nombre de paramètres : « 8,2B », « 70B ».
pub(crate) fn params(n: u64) -> String {
    let b = n as f64 / 1e9;
    format!("{}B", num(b, if b < 10.0 { 1 } else { 0 }))
}

fn ctx(n: u32) -> String {
    if n >= 1024 {
        format!("{}k", n / 1024)
    } else {
        n.to_string()
    }
}

fn model_row(i: usize, m: &ModelEntry) -> String {
    let sizes = match (m.variants.first(), m.variants.last()) {
        (Some(a), Some(b)) => format!(
            "{}–{}",
            gb(a.size_bytes).trim_end_matches(" Go"),
            gb(b.size_bytes)
        ),
        _ => "—".into(),
    };
    let arch = match &m.arch {
        Some(a) => {
            let mut s = format!(
                "{} · {} couches · {} KV",
                a.architecture, a.n_layers, a.n_kv_heads
            );
            if let Some(k) = a.kv_layers {
                let _ = write!(s, " ({k} couches KV)");
            }
            s
        }
        None => "inconnue".into(),
    };
    let mut flags = Vec::new();
    let mut repos: Vec<&str> = m.variants.iter().map(|v| v.repo_id.as_str()).collect();
    repos.sort_unstable();
    repos.dedup();
    if repos.len() > 1 {
        flags.push(format!("{} dépôts", repos.len()));
    }
    if let Some(active) = m.params_active {
        flags.push(format!("MoE {} actifs", params(active)));
    }
    if m.mmproj_bytes.is_some() {
        flags.push("vision".into());
    }
    if m.gated {
        flags.push("protégé".into());
    }
    let name: String = m.display_name.chars().take(34).collect();
    format!(
        "{:>3}  {:<34} {:>6}  {:>2} quantifs  {:<18} {:>5}  {}{}\n",
        i + 1,
        name,
        params(m.params_total),
        m.variants.len(),
        sizes,
        ctx(m.ctx_train),
        arch,
        if flags.is_empty() {
            String::new()
        } else {
            format!(" · {}", flags.join(", "))
        }
    )
}

/// « en ligne (…) », « cache (synchronisé le …) », « catalogue embarqué (snapshot du …) ».
pub(crate) fn source_label(source: &DataSource) -> String {
    let when = source
        .fetched_at
        .with_timezone(&Local)
        .format("%d/%m/%Y %H:%M");
    match source.kind {
        DataSourceKind::Online => format!("en ligne ({when})"),
        DataSourceKind::Cache => format!("cache (synchronisé le {when})"),
        DataSourceKind::Snapshot => format!("catalogue embarqué (snapshot du {when})"),
    }
}

pub fn render(s: &SyncSummary, c: &Catalog, cache_root: &Path) -> String {
    let mut out = String::new();
    out.push('\n');
    line(&mut out, "Source", &source_label(&s.source));
    line(
        &mut out,
        "Catalogue",
        &format!(
            "{} dépôts GGUF · {} modèles · {} détaillés (tailles réelles) · {} architectures",
            thousands(u64::from(s.repos)),
            thousands(u64::from(s.models)),
            s.detailed,
            s.with_arch
        ),
    );
    let mut calls = format!("{} API · {} resolver", s.api_calls, s.resolver_calls);
    if let Some(rl) = s.rate_limit {
        let _ = write!(
            calls,
            " · quota API restant {} (remise à zéro dans {} s)",
            rl.remaining, rl.reset_s
        );
    }
    line(&mut out, "Appels", &calls);
    line(
        &mut out,
        "Durée",
        &format!("{} s", num(s.duration_ms as f64 / 1000.0, 1)),
    );
    line(&mut out, "Cache", &cache_root.display().to_string());
    for w in s.warnings.iter().take(10) {
        line(&mut out, "Attention", w);
    }
    if s.warnings.len() > 10 {
        line(
            &mut out,
            "",
            &format!("… et {} autres avertissements", s.warnings.len() - 10),
        );
    }

    let detailed: Vec<&ModelEntry> = c.models.iter().filter(|m| !m.variants.is_empty()).collect();
    if !detailed.is_empty() {
        let _ = writeln!(
            out,
            "\nModèles détaillés (ordre du pré-classement, 25 premiers) :"
        );
        for (i, m) in detailed.iter().take(25).enumerate() {
            out.push_str(&model_row(i, m));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use gram_core::{
        ArchInfo, ArchSource, DataSource, Pipeline, QuantVariant, RateLimitInfo, UseCase,
    };

    fn entry() -> ModelEntry {
        let variant = |label: &str, repo: &str, size_bytes: u64| QuantVariant {
            label: label.into(),
            repo_id: repo.into(),
            files: vec![format!("m-{label}.gguf")],
            size_bytes,
            bpw: 4.9,
            exact: true,
        };
        ModelEntry {
            repo_id: "ggml-org/Qwen3.6-35B-A3B-GGUF".into(),
            sha: "eb0b3cc9".into(),
            base_model: Some("Qwen/Qwen3.6-35B-A3B".into()),
            display_name: "Qwen3.6-35B-A3B".into(),
            params_total: 34_660_610_688,
            params_active: Some(3_454_988_928),
            arch: Some(ArchInfo {
                architecture: "qwen35moe".into(),
                n_layers: 40,
                n_heads: 16,
                n_kv_heads: 2,
                head_dim: 256,
                kv_layers: Some(10),
                embedding_length: Some(2048),
                expert_count: Some(256),
                expert_used_count: Some(8),
                sliding_window: None,
                source: ArchSource::GgufHeader,
            }),
            ctx_train: 262_144,
            pipeline: Pipeline::ImageTextToText,
            use_cases: vec![UseCase::General, UseCase::Vision],
            downloads: 197_332,
            likes: 10,
            created_at: None,
            last_modified: None,
            gated: false,
            variants: vec![
                variant("Q4_K_M", "unsloth/Qwen3.6-35B-A3B-GGUF", 20_542_732_576),
                variant("BF16", "ggml-org/Qwen3.6-35B-A3B-GGUF", 69_376_637_536),
            ],
            mmproj_bytes: Some(902_822_144),
            alternatives: vec![],
        }
    }

    #[test]
    fn rendu() {
        let fetched_at = Utc.with_ymd_and_hms(2026, 11, 9, 19, 0, 0).unwrap();
        let summary = SyncSummary {
            source: DataSource {
                kind: DataSourceKind::Online,
                fetched_at,
            },
            repos: 1386,
            models: 1067,
            detailed: 146,
            with_arch: 137,
            api_calls: 178,
            resolver_calls: 149,
            rate_limit: Some(RateLimitInfo {
                remaining: 322,
                reset_s: 36,
            }),
            duration_ms: 82_400,
            warnings: vec!["Détail de a/b indisponible".into()],
        };
        let catalog = Catalog {
            format: gram_hub::catalog::CATALOG_FORMAT,
            fetched_at,
            max_params: None,
            models: vec![entry()],
        };
        let out = render(&summary, &catalog, Path::new("D:/cache"));
        assert!(out.contains("Source      en ligne ("), "{out}");
        assert!(out.contains(
            "1 386 dépôts GGUF · 1 067 modèles · 146 détaillés (tailles réelles) · 137 architectures"
        ));
        assert!(out.contains("178 API · 149 resolver · quota API restant 322"));
        assert!(out.contains("Durée       82,4 s"));
        assert!(out.contains("Attention   Détail de a/b indisponible"));
        assert!(out.contains("Qwen3.6-35B-A3B"));
        assert!(out.contains("35B   2 quantifs  19,1–64,6 Go"), "{out}");
        assert!(out.contains("256k  qwen35moe · 40 couches · 2 KV (10 couches KV)"));
        assert!(out.contains("· 2 dépôts, MoE 3,5B actifs, vision"), "{out}");
    }
}
