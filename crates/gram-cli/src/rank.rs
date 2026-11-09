//! `gram-cli rank` : classement des modèles du catalogue pour cette machine ou pour un profil
//! matériel enregistré (§9 du plan), avec les paramètres par défaut. Aucun appel réseau.

use crate::estimate::hardware;
use crate::sync::{cache, source_label};
use anyhow::{Context, Result};
use chrono::Utc;
use gram_core::fmt::{gb, num, thousands, tok_s};
use gram_core::{
    HardwareProfile, RankContext, RankItem, RankRequest, RankResponse, Runtime, Settings, UseCase,
};
use std::fmt::Write as _;
use std::path::PathBuf;

const GIB: f64 = (1u64 << 30) as f64;

pub struct RankArgs {
    pub use_case: UseCase,
    pub ctx: Option<u32>,
    pub runtime: Option<Runtime>,
    pub max_size_gb: Option<f64>,
    pub show_too_big: bool,
    /// Lignes par liste ; 0 : toutes.
    pub limit: u32,
    pub profile: Option<PathBuf>,
    pub json: bool,
    pub cache_dir: Option<PathBuf>,
}

pub fn run(args: RankArgs) -> Result<()> {
    let hw = hardware(args.profile.as_deref())?;
    // Les runtimes installés n'ont de sens que pour cette machine.
    let runtimes = args.profile.is_none().then(gram_hw::detect_runtimes);
    let cache = cache(args.cache_dir)?;
    let (catalog, source) = gram_hub::load_offline(&cache)
        .context("aucun catalogue disponible : lancer `gram-cli sync`")?;
    let settings = Settings::default();
    let request = RankRequest {
        use_case: args.use_case,
        ctx: args.ctx,
        runtime: args.runtime,
        max_size: args
            .max_size_gb
            .filter(|g| g.is_finite() && *g > 0.0)
            .map(|g| (g * GIB) as u64),
        show_too_big: args.show_too_big,
        limit: (args.limit > 0).then_some(args.limit),
    };
    let cx = RankContext::new(&hw, runtimes.as_ref(), &settings, Utc::now());
    let response = gram_core::rank(&catalog.models, &request, &cx, source);
    if args.json {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        print!("{}", render(&hw, &response));
    }
    Ok(())
}

const LABEL_WIDTH: usize = 11;
const NAME_WIDTH: usize = 34;

fn line(out: &mut String, label: &str, text: &str) {
    let _ = writeln!(out, "{label:<LABEL_WIDTH$}{text}");
}

fn header(out: &mut String) {
    let _ = writeln!(
        out,
        "{:>3}  {:<NAME_WIDTH$} {:<13} {:>8}  {:<14} {:>6} {:>9} {:>5}  Adéquation",
        "#", "Modèle", "Quantif.", "Taille", "Mode", "tok/s", "Ctx max", "Score"
    );
}

fn row(out: &mut String, i: usize, it: &RankItem, explain: bool) {
    let (v, e) = (&it.best.variant, &it.best.estimate);
    let name: String = it.display_name.chars().take(NAME_WIDTH).collect();
    // Certains libellés reprennent tout le nom du fichier : tronqués à la largeur de la colonne.
    let mut label: String = v.label.chars().take(12).collect();
    if label.chars().count() < v.label.chars().count() {
        label.pop();
        label.push('…');
    }
    if e.approximate {
        label.push('*');
    }
    let ctx_max = if e.ctx_max > 0 {
        thousands(u64::from(e.ctx_max))
    } else {
        "—".into()
    };
    let _ = writeln!(
        out,
        "{:>3}  {:<NAME_WIDTH$} {:<13} {:>8}  {:<14} {:>6} {:>9} {:>5}  {}",
        i + 1,
        name,
        label,
        gb(v.size_bytes),
        e.mode.label(),
        tok_s(e.tok_s),
        ctx_max,
        it.score.total.round(),
        e.fit.label()
    );
    if explain {
        let _ = writeln!(out, "{:>5}{}", "", it.explanation);
    }
}

pub fn render(hw: &HardwareProfile, r: &RankResponse) -> String {
    let mut out = String::new();
    let b = &r.budget;
    let gpu = match (&hw.gpu, b.gpu_offload()) {
        (Some(g), true) => format!("{} : {} de VRAM utilisables", g.name, gb(b.vram)),
        (Some(g), false) => format!("{} (intégré) : exécution sur le CPU", g.name),
        (None, _) => "aucun GPU : exécution sur le CPU".into(),
    };
    line(
        &mut out,
        "Machine",
        &format!("{gpu} · RAM {} utilisables", gb(b.ram)),
    );
    line(
        &mut out,
        "Demande",
        &format!(
            "{} · contexte {} tokens · runtime {} · lent sous {} tok/s",
            r.use_case.label(),
            thousands(u64::from(r.ctx)),
            r.runtime.label(),
            num(f64::from(r.slow_tok_s), 0)
        ),
    );
    line(&mut out, "Données", &source_label(&r.data_source));

    out.push('\n');
    if r.items.is_empty() {
        out.push_str("Aucun modèle exécutable assez rapide.\n");
    } else {
        header(&mut out);
        for (i, it) in r.items.iter().enumerate() {
            row(&mut out, i, it, true);
        }
        if r.total_items as usize > r.items.len() {
            let _ = writeln!(
                out,
                "\n{} modèles affichés sur {} (--limit 0 pour tout afficher).",
                r.items.len(),
                r.total_items
            );
        }
    }

    if r.total_slow > 0 {
        let _ = writeln!(out, "\nExécutable mais lent ({}) :", r.total_slow);
        header(&mut out);
        for (i, it) in r.slow.iter().enumerate() {
            row(&mut out, i, it, false);
        }
    }
    if r.too_big_count > 0 {
        if r.too_big.is_empty() {
            let _ = writeln!(
                out,
                "\nTrop gros : {} modèles (--show-too-big pour les lister).",
                r.too_big_count
            );
        } else {
            let _ = writeln!(out, "\nTrop gros ({}) :", r.too_big_count);
            header(&mut out);
            for (i, it) in r.too_big.iter().enumerate() {
                row(&mut out, i, it, true);
            }
        }
    }

    let _ = writeln!(
        out,
        "\nEstimations à ±30 % ; tok/s = vitesse de génération ; * estimation approximative."
    );
    for n in &r.notes {
        let _ = writeln!(out, "  - {n}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gram_core::{DataSource, DataSourceKind, ModelEntry};

    #[test]
    fn rendu() {
        let hw: HardwareProfile = serde_json::from_str(include_str!(
            "../../../tests/fixtures/profiles/p3-rtx4090-24go-64go-ddr5.json"
        ))
        .unwrap();
        let models: Vec<ModelEntry> = [
            include_str!("../../../tests/fixtures/models/Qwen--Qwen3-8B-GGUF.json"),
            include_str!(
                "../../../tests/fixtures/models/unsloth--Llama-3.3-70B-Instruct-GGUF.json"
            ),
        ]
        .iter()
        .map(|j| serde_json::from_str(j).unwrap())
        .collect();
        let settings = Settings::default();
        let now = chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 11, 9, 12, 0, 0).unwrap();
        let cx = RankContext::new(&hw, None, &settings, now);
        let request = RankRequest {
            use_case: UseCase::Code,
            ctx: Some(16_384),
            ..RankRequest::default()
        };
        let source = DataSource {
            kind: DataSourceKind::Snapshot,
            fetched_at: now,
        };
        let r = gram_core::rank(&models, &request, &cx, source);
        let out = render(&hw, &r);
        assert!(
            out.contains("NVIDIA GeForce RTX 4090 : 21,6 Go de VRAM utilisables"),
            "{out}"
        );
        assert!(out.contains("Code · contexte 16 384 tokens · runtime Ollama · lent sous 5 tok/s"));
        let first = out.lines().find(|l| l.starts_with("  1  ")).unwrap();
        assert!(
            first.contains("Qwen3-8B") && first.contains("GPU"),
            "{first}"
        );
        assert!(out.contains("Tient entièrement en VRAM"));
        // Le 70B a une quantification assez rapide (entièrement en VRAM) : il n'est pas lent.
        assert!(out.contains("Llama-3.3-70B-Instruct"));
        assert!(out.contains("Sans réglage, Ollama choisit le contexte"));
    }
}
