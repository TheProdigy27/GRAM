//! `gram-cli ollama` : modèles installés dans Ollama, rapprochés du catalogue, et mesure réelle
//! d'une quantification face à l'estimation (phase 7 du plan).

use crate::estimate::{find, mem};
use crate::sync::{cache, runtime};
use anyhow::{Context, Result, bail};
use gram_core::calibration::{Calibration, Measurement, calibrate, measurement};
use gram_core::estimate::{candidate_variants, estimate};
use gram_core::fmt::{gb, num, thousands, tok_s};
use gram_core::runtime::{installed_for, installed_variant};
use gram_core::{
    EstimateParams, MemoryBudget, ModelEntry, OllamaModel, QuantTable, RunMode, Workload,
};
use gram_ollama::OllamaClient;
use serde::Serialize;
use std::fmt::Write as _;
use std::path::PathBuf;

pub struct OllamaArgs {
    pub measure: Option<String>,
    pub quant: Option<String>,
    pub ctx: u32,
    pub json: bool,
    pub cache_dir: Option<PathBuf>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Installed<'a> {
    #[serde(flatten)]
    model: &'a OllamaModel,
    /// Modèle du catalogue correspondant.
    catalog_repo_id: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeasureReport {
    measurement: Measurement,
    calibration: Calibration,
}

pub fn run(args: OllamaArgs) -> Result<()> {
    let client = OllamaClient::from_env()
        .context("adresse de l'API d'Ollama inutilisable (variable OLLAMA_HOST)")?;
    let rt = runtime()?;
    let installed = rt.block_on(client.tags())?;
    let cache = cache(args.cache_dir)?;
    let (catalog, _) = gram_hub::load_offline(&cache)
        .context("aucun catalogue disponible : lancer `gram-cli sync`")?;

    let Some(query) = args.measure else {
        let rows: Vec<Installed> = installed
            .iter()
            .map(|om| Installed {
                model: om,
                catalog_repo_id: catalog_match(&catalog.models, om).map(|m| m.repo_id.as_str()),
            })
            .collect();
        if args.json {
            println!("{}", serde_json::to_string_pretty(&rows)?);
        } else {
            print!("{}", render_list(client.base_url(), &rows));
        }
        return Ok(());
    };

    let (model, _) = find(&catalog.models, &query)?;
    let hw = gram_hw::detect();
    let params = EstimateParams::default();
    let budget = MemoryBudget::from_profile(&hw, &params);
    let candidates = candidate_variants(model, &params, QuantTable::embedded());
    let pick = |label: &str| {
        candidates
            .iter()
            .find(|v| v.label.eq_ignore_ascii_case(label))
    };
    let variant = match &args.quant {
        Some(q) => pick(q)
            .with_context(|| format!("quantification {q} inconnue pour {}", model.repo_id))?,
        None => installed_for(&installed, model)
            .find_map(|om| pick(om.quant.as_deref()?))
            .with_context(|| {
                format!(
                    "aucune quantification de {} n'est installée dans Ollama",
                    model.repo_id
                )
            })?,
    };
    let Some(om) = installed_variant(&installed, model, &variant.repo_id, &variant.label) else {
        bail!(
            "{} n'est pas installé dans Ollama : {}",
            variant.label,
            gram_core::launch::ollama_pull_command(variant)
        );
    };
    let work = Workload {
        ctx: args.ctx,
        vision: false,
    };
    let est = estimate(model, variant, &budget, work, &params);
    if est.mode == RunMode::TooBig {
        bail!(
            "{} ne tient pas en mémoire d'après l'estimation",
            variant.label
        );
    }
    eprintln!(
        "Mesure de {} au contexte {} (chargement, puis {} tokens)…",
        om.name,
        thousands(u64::from(est.ctx_used)),
        gram_ollama::MEASURE_TOKENS
    );
    let run = rt.block_on(gram_ollama::measure(&client, &om.name, est.ctx_used))?;
    let m = measurement(model, variant, &est, &om.name, &run, chrono::Utc::now());
    let calibration = calibrate(vec![m.clone()], &budget, &params);
    if args.json {
        let report = MeasureReport {
            measurement: m,
            calibration,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", render_measurement(&m, &calibration));
    }
    Ok(())
}

/// Modèle du catalogue installé sous ce nom (dépôt canonique, alternatif ou d'une variante).
fn catalog_match<'a>(models: &'a [ModelEntry], om: &OllamaModel) -> Option<&'a ModelEntry> {
    models
        .iter()
        .find(|m| installed_for(std::slice::from_ref(om), m).next().is_some())
}

fn render_list(base: &str, rows: &[Installed]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Ollama ({base}) : {} modèle(s) installé(s)",
        rows.len()
    );
    for r in rows {
        let catalog = match r.catalog_repo_id {
            Some(repo) => format!("catalogue : {repo}"),
            None if r.model.repo_id.is_some() => "absent du catalogue".into(),
            None => "bibliothèque d'Ollama (non rapprochée)".into(),
        };
        let _ = writeln!(
            out,
            "  {:<52} {:>9}  {:<10} {catalog}",
            r.model.name,
            gb(r.model.size),
            r.model.quant.as_deref().unwrap_or("—"),
        );
    }
    out
}

fn percent(x: Option<f32>) -> String {
    match x {
        Some(x) => format!(
            "{}{} %",
            if x >= 0.0 { "+" } else { "" },
            num(f64::from(x) * 100.0, 0)
        ),
        None => "—".into(),
    }
}

fn render_measurement(m: &Measurement, c: &Calibration) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} {} ({}) · contexte {}",
        m.display_name,
        m.label,
        m.ollama_model,
        thousands(u64::from(m.ctx))
    );
    let _ = writeln!(
        out,
        "  Vitesse   mesurée {} tok/s ({} tokens) · estimée {} · écart {}",
        num(f64::from(m.tok_s), 1),
        m.eval_count,
        tok_s(m.estimated_tok_s),
        percent(m.tok_s_error)
    );
    let _ = writeln!(
        out,
        "  Mémoire   mesurée {} dont {} en VRAM · estimée {} dont {} en VRAM ({}) · écart {}",
        m.mem_total.map_or("—".into(), gb),
        m.mem_vram.map_or("—".into(), gb),
        gb(m.estimated_mem_total),
        mem(m.estimated_mem_vram),
        m.estimated_mode.label(),
        percent(m.mem_error)
    );
    let _ = writeln!(
        out,
        "  Chargement {} s · prompt {}",
        num(m.load_ms as f64 / 1000.0, 1),
        m.prompt_tok_s
            .map_or("—".into(), |t| format!("{} tok/s", num(f64::from(t), 0)))
    );
    for n in &m.notes {
        let _ = writeln!(out, "  ! {n}");
    }
    let _ = writeln!(out, "Calibration : {}", c.summary);
    out
}
