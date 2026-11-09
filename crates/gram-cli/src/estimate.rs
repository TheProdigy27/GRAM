//! `gram-cli estimate` : toutes les quantifications d'un modèle du catalogue, estimées pour
//! cette machine ou pour un profil matériel enregistré (§8 du plan). Aucun appel réseau.

use crate::fmt::{gb, num, thousands};
use crate::sync::{cache, params, source_label};
use anyhow::{Context, Result, bail};
use gram_core::{
    DataSource, EstimateParams, FitLevel, HardwareProfile, KvCacheType, MemoryBudget, ModelEntry,
    QuantTable, RunMode, VariantEstimate, Workload, estimate_variants,
};
use serde::Serialize;
use std::cmp::Reverse;
use std::fmt::Write as _;
use std::path::PathBuf;

pub struct EstimateArgs {
    pub query: String,
    pub ctx: u32,
    pub vision: bool,
    pub kv_cache: KvCacheType,
    pub profile: Option<PathBuf>,
    pub json: bool,
    pub cache_dir: Option<PathBuf>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report<'a> {
    repo_id: &'a str,
    display_name: &'a str,
    ctx: u32,
    vision: bool,
    kv_cache: KvCacheType,
    budget: &'a MemoryBudget,
    source: &'a DataSource,
    variants: &'a [VariantEstimate],
}

pub fn run(args: EstimateArgs) -> Result<()> {
    let hw: HardwareProfile = match &args.profile {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("lecture de {}", path.display()))?;
            serde_json::from_str(&text)
                .with_context(|| format!("{} n'est pas un HardwareProfile", path.display()))?
        }
        None => gram_hw::detect(),
    };
    let cache = cache(args.cache_dir)?;
    let (catalog, source) = gram_hub::load_offline(&cache)
        .context("aucun catalogue disponible : lancer `gram-cli sync`")?;
    let (model, others) = find(&catalog.models, &args.query)?;
    if !others.is_empty() {
        let names: Vec<&str> = others.iter().take(5).map(|m| m.repo_id.as_str()).collect();
        let more = others.len().saturating_sub(names.len());
        eprintln!(
            "Autres correspondances : {}{}",
            names.join(", "),
            if more > 0 {
                format!(" (+{more})")
            } else {
                String::new()
            }
        );
    }

    let params = EstimateParams {
        kv_cache: args.kv_cache,
        ..EstimateParams::default()
    };
    let budget = MemoryBudget::from_profile(&hw, &params);
    let work = Workload {
        ctx: args.ctx,
        vision: args.vision,
    };
    let rows = estimate_variants(model, &budget, work, &params, QuantTable::embedded());

    if args.json {
        let report = Report {
            repo_id: &model.repo_id,
            display_name: &model.display_name,
            ctx: work.ctx,
            vision: work.vision,
            kv_cache: params.kv_cache,
            budget: &budget,
            source: &source,
            variants: &rows,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!(
            "{}",
            render(model, &hw, &budget, work, &params, &source, &rows)
        );
    }
    Ok(())
}

/// Le modèle demandé : correspondance exacte (dépôt, nom ou modèle de base) d'abord, puis
/// sous-chaîne ; à égalité, les modèles détaillés puis les plus téléchargés.
fn find<'a>(
    models: &'a [ModelEntry],
    query: &str,
) -> Result<(&'a ModelEntry, Vec<&'a ModelEntry>)> {
    let q = query.trim().to_lowercase();
    let exact = |m: &ModelEntry| {
        m.repo_id.to_lowercase() == q
            || m.display_name.to_lowercase() == q
            || m.base_model.as_deref().map(str::to_lowercase).as_deref() == Some(q.as_str())
    };
    let mut matches: Vec<&ModelEntry> = models
        .iter()
        .filter(|m| {
            exact(m)
                || m.repo_id.to_lowercase().contains(&q)
                || m.display_name.to_lowercase().contains(&q)
        })
        .collect();
    if q.is_empty() || matches.is_empty() {
        bail!("aucun modèle du catalogue ne correspond à « {query} »");
    }
    matches.sort_by_key(|m| (!exact(m), m.variants.is_empty(), Reverse(m.downloads)));
    let first = matches.remove(0);
    Ok((first, matches))
}

const LABEL_WIDTH: usize = 11;

fn line(out: &mut String, label: &str, text: &str) {
    let _ = writeln!(out, "{label:<LABEL_WIDTH$}{text}");
}

fn mode(m: RunMode) -> String {
    match m {
        RunMode::Gpu => "GPU".into(),
        RunMode::Hybrid { gpu_layers, layers } => format!("hybride {gpu_layers}/{layers}"),
        RunMode::Cpu => "CPU".into(),
        RunMode::TooBig => "trop gros".into(),
    }
}

fn fit(f: FitLevel) -> &'static str {
    match f {
        FitLevel::Perfect => "parfait",
        FitLevel::Good => "bon",
        FitLevel::Marginal => "limite",
        FitLevel::TooTight => "trop juste",
    }
}

fn mem(bytes: u64) -> String {
    if bytes == 0 { "—".into() } else { gb(bytes) }
}

fn speed(tok_s: f32) -> String {
    match tok_s {
        t if t <= 0.0 => "—".into(),
        t if t < 10.0 => format!("~{}", num(f64::from(t), 1)),
        t => format!("~{}", t.round()),
    }
}

fn render(
    m: &ModelEntry,
    hw: &HardwareProfile,
    b: &MemoryBudget,
    work: Workload,
    p: &EstimateParams,
    source: &DataSource,
    rows: &[VariantEstimate],
) -> String {
    let mut out = String::new();
    line(
        &mut out,
        "Modèle",
        &format!("{} · {}", m.display_name, m.repo_id),
    );
    let mut about = vec![format!("{} paramètres", params(m.params_total))];
    if let Some(active) = m.params_active {
        about.push(format!("MoE, {} actifs", params(active)));
    }
    match &m.arch {
        Some(a) => {
            let layers = match a.kv_layers {
                Some(kv) => format!("{} couches dont {kv} d'attention", a.n_layers),
                None => format!("{} couches", a.n_layers),
            };
            about.push(format!(
                "{} · {layers} · {} têtes KV × {}",
                a.architecture, a.n_kv_heads, a.head_dim
            ));
        }
        None => about.push("architecture inconnue".into()),
    }
    if m.ctx_train > 0 {
        about.push(format!(
            "contexte d'entraînement {}",
            thousands(u64::from(m.ctx_train))
        ));
    }
    line(&mut out, "", &about.join(" · "));

    let gpu = match (&hw.gpu, b.gpu_offload()) {
        (Some(g), true) => format!(
            "{} : {} de VRAM utilisables · ≈{} Go/s",
            g.name,
            gb(b.vram),
            num(f64::from(b.gpu_bandwidth_gbs.unwrap_or_default()), 0)
        ),
        (Some(g), false) => format!("{} (intégré) : exécution sur le CPU", g.name),
        (None, _) => "aucun : exécution sur le CPU".into(),
    };
    line(&mut out, "GPU", &gpu);
    line(
        &mut out,
        "RAM",
        &format!(
            "{} utilisables sur {} · ≈{} Go/s",
            gb(b.ram),
            gb(hw.ram.total),
            num(f64::from(b.ram_bandwidth_gbs), 0)
        ),
    );
    let mut demand = format!(
        "contexte {} tokens · cache KV {}",
        thousands(u64::from(work.ctx)),
        p.kv_cache.label()
    );
    if let Some(kv) = rows.first().map(|r| r.estimate.kv_per_token) {
        let _ = write!(demand, " ({} Kio par token)", thousands(kv.div_ceil(1024)));
    }
    if work.vision {
        demand.push_str(" · vision");
    }
    line(&mut out, "Demande", &demand);
    line(&mut out, "Données", &source_label(source));

    if rows.is_empty() {
        let _ = writeln!(
            out,
            "\nAucune quantification à estimer (ni fichiers détaillés, ni nombre de paramètres)."
        );
        return out;
    }

    let _ = writeln!(
        out,
        "\n{:<14} {:>8} {:>6}  {:<16} {:>8} {:>8} {:>7} {:>9}  Adéquation",
        "Quantif.", "Taille", "bpw", "Mode", "VRAM", "RAM", "tok/s", "Ctx max"
    );
    for r in rows {
        let (v, e) = (&r.variant, &r.estimate);
        let label = if e.approximate {
            format!("{}*", v.label)
        } else {
            v.label.clone()
        };
        let bpw = if v.bpw > 0.0 {
            num(f64::from(v.bpw), 2)
        } else {
            "—".into()
        };
        let ctx_max = if e.ctx_max > 0 {
            thousands(u64::from(e.ctx_max))
        } else {
            "—".into()
        };
        let _ = writeln!(
            out,
            "{:<14} {:>8} {:>6}  {:<16} {:>8} {:>8} {:>7} {:>9}  {}",
            label,
            gb(v.size_bytes),
            bpw,
            mode(e.mode),
            mem(e.mem_gpu),
            mem(e.mem_cpu),
            speed(e.tok_s),
            ctx_max,
            fit(e.fit)
        );
    }

    let _ = writeln!(
        out,
        "\nEstimations à ±30 % ; tok/s = vitesse de génération (traitement du prompt non modélisé)."
    );
    if rows.iter().any(|r| r.estimate.approximate) {
        let _ = writeln!(out, "* estimation approximative");
    }
    let mut notes: Vec<&str> = Vec::new();
    for w in rows.iter().flat_map(|r| &r.estimate.warnings) {
        if !notes.contains(&w.as_str()) {
            notes.push(w);
        }
    }
    for n in notes {
        let _ = writeln!(out, "  - {n}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(repo: &str, name: &str, downloads: u64, detailed: bool) -> ModelEntry {
        let mut m: ModelEntry = serde_json::from_str(include_str!(
            "../../../tests/fixtures/models/Qwen--Qwen3-8B-GGUF.json"
        ))
        .unwrap();
        m.repo_id = repo.into();
        m.display_name = name.into();
        m.base_model = None;
        m.downloads = downloads;
        if !detailed {
            m.variants.clear();
        }
        m
    }

    #[test]
    fn recherche_du_modele() {
        let models = vec![
            entry(
                "a/Qwen3-8B-Finetune-GGUF",
                "Qwen3-8B-Finetune",
                9_000_000,
                true,
            ),
            entry("b/Qwen3-8B-GGUF", "Qwen3-8B", 10, false),
            entry("Qwen/Qwen3-8B-GGUF", "Qwen3-8B", 1_000, true),
        ];
        // Nom exact : les modèles détaillés d'abord, même moins téléchargés.
        let (m, others) = find(&models, "qwen3-8b").unwrap();
        assert_eq!(m.repo_id, "Qwen/Qwen3-8B-GGUF");
        assert_eq!(others.len(), 2);
        assert_eq!(others[0].repo_id, "b/Qwen3-8B-GGUF");
        // Sous-chaîne : le plus téléchargé.
        assert_eq!(
            find(&models, "finetune").unwrap().0.repo_id,
            "a/Qwen3-8B-Finetune-GGUF"
        );
        assert!(find(&models, "llama").is_err());
        assert!(find(&models, "  ").is_err());
    }

    #[test]
    fn rendu() {
        let model = entry("Qwen/Qwen3-8B-GGUF", "Qwen3-8B", 1_000, true);
        let hw: HardwareProfile = serde_json::from_str(include_str!(
            "../../../tests/fixtures/profiles/p2-rtx3060-12go-32go-ddr4.json"
        ))
        .unwrap();
        let p = EstimateParams::default();
        let b = MemoryBudget::from_profile(&hw, &p);
        let work = Workload {
            ctx: 8192,
            vision: false,
        };
        let rows = estimate_variants(&model, &b, work, &p, QuantTable::embedded());
        let source = DataSource {
            kind: gram_core::DataSourceKind::Snapshot,
            fetched_at: chrono::Utc::now(),
        };
        let out = render(&model, &hw, &b, work, &p, &source, &rows);
        assert!(
            out.contains("Modèle     Qwen3-8B · Qwen/Qwen3-8B-GGUF"),
            "{out}"
        );
        assert!(out.contains("qwen3 · 36 couches · 8 têtes KV × 128"));
        assert!(out.contains("NVIDIA GeForce RTX 3060 : 10,8 Go de VRAM utilisables · ≈360 Go/s"));
        assert!(out.contains("contexte 8 192 tokens · cache KV f16 (144 Kio par token)"));
        let q4 = out.lines().find(|l| l.starts_with("Q4_K_M ")).unwrap();
        assert!(
            q4.contains("GPU") && q4.contains("~37") && q4.ends_with("parfait"),
            "{q4}"
        );
        let bf16 = out.lines().find(|l| l.starts_with("BF16 ")).unwrap();
        assert!(bf16.contains("hybride "), "{bf16}");
        assert!(
            !out.contains("UD-IQ1_S"),
            "sous le minimum de bits par poids"
        );
    }
}
