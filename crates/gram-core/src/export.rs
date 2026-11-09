//! Export du classement en Markdown ou en CSV (commande `export_ranking`).
//!
//! Le CSV suit les conventions françaises d'Excel : séparateur `;`, virgule décimale, UTF-8
//! avec BOM (sans lui, Excel lit le fichier en Windows-1252 et casse les accents).

use crate::fmt::{gb, gb_value, num, thousands, tok_s};
use crate::hardware::HardwareProfile;
use crate::rank::{RankItem, RankResponse};
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use ts_rs::TS;

/// Format de la commande `export_ranking`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ExportFormat {
    Md,
    Csv,
}

impl ExportFormat {
    /// Extension attendue du fichier, sans le point.
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Md => "md",
            Self::Csv => "csv",
        }
    }
}

/// Résumé du matériel sur une ligne : « GPU … · RAM … · CPU … ».
pub fn hardware_summary(hw: &HardwareProfile) -> String {
    let mut parts = Vec::new();
    match &hw.gpu {
        Some(g) => {
            let mut s = format!("GPU {} ({}", g.name, gb(g.vram_total));
            if let Some(bw) = g.bandwidth_gbs {
                let _ = write!(s, ", ≈{} Go/s", num(f64::from(bw), 0));
            }
            if g.integrated {
                s.push_str(", intégré");
            }
            s.push(')');
            parts.push(s);
        }
        None => parts.push("aucun GPU".into()),
    }
    let r = &hw.ram;
    let mut ram = format!(
        "RAM {} {}",
        gb(r.installed.unwrap_or(r.total)),
        r.kind.label()
    );
    if let Some(speed) = r.speed_mts {
        let _ = write!(ram, "-{speed}");
    }
    parts.push(ram);
    parts.push(format!("CPU {}", hw.cpu.name));
    parts.join(" · ")
}

fn md_cell(text: &str) -> String {
    text.replace('|', "\\|").replace(['\r', '\n'], " ")
}

fn md_row(out: &mut String, i: usize, it: &RankItem) {
    let (v, e) = (&it.best.variant, &it.best.estimate);
    let ctx_max = if e.ctx_max > 0 {
        thousands(u64::from(e.ctx_max))
    } else {
        "—".into()
    };
    let _ = writeln!(
        out,
        "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
        i + 1,
        md_cell(&it.display_name),
        md_cell(&v.label),
        gb(v.size_bytes),
        e.mode.label(),
        tok_s(e.tok_s),
        ctx_max,
        it.score.total.round(),
        e.fit.label(),
        md_cell(&it.explanation)
    );
}

fn md_table(out: &mut String, items: &[RankItem]) {
    out.push_str(
        "| # | Modèle | Quantif. | Taille | Mode | tok/s | Ctx max | Score | Adéquation | Explication |\n\
         |---:|---|---|---:|---|---:|---:|---:|---|---|\n",
    );
    for (i, it) in items.iter().enumerate() {
        md_row(out, i, it);
    }
}

/// Classement en Markdown, avec l'en-tête de la machine et des réglages.
pub fn to_markdown(
    r: &RankResponse,
    hw: &HardwareProfile,
    generated_at: DateTime<FixedOffset>,
) -> String {
    let mut out = String::from("# Classement GRAM\n\n");
    let _ = writeln!(out, "- Date : {}", generated_at.format("%d/%m/%Y %H:%M"));
    let _ = writeln!(out, "- Machine : {}", md_cell(&hardware_summary(hw)));
    let mut usable = format!("{} de RAM", gb(r.budget.ram));
    if r.budget.gpu_offload() {
        usable = format!("{} de VRAM · {usable}", gb(r.budget.vram));
    }
    let _ = writeln!(out, "- Mémoire utilisable : {usable}");
    let _ = writeln!(
        out,
        "- Cas d'usage : {} · contexte {} tokens · runtime {}",
        r.use_case.label(),
        thousands(u64::from(r.ctx)),
        r.runtime.label()
    );
    let fetched = r
        .data_source
        .fetched_at
        .with_timezone(generated_at.offset());
    let _ = writeln!(
        out,
        "- Données : {} ({})",
        r.data_source.kind.label(),
        fetched.format("%d/%m/%Y %H:%M")
    );

    let _ = writeln!(out, "\n## Exécutables ({})\n", r.items.len());
    md_table(&mut out, &r.items);
    if !r.slow.is_empty() {
        let _ = writeln!(
            out,
            "\n## Exécutable mais lent : moins de {} tok/s ({})\n",
            num(f64::from(r.slow_tok_s), 0),
            r.slow.len()
        );
        md_table(&mut out, &r.slow);
    }
    if !r.too_big.is_empty() {
        let _ = writeln!(out, "\n## Trop gros ({})\n", r.too_big.len());
        md_table(&mut out, &r.too_big);
    } else if r.too_big_count > 0 {
        let _ = writeln!(
            out,
            "\n{} modèles trop gros pour cette machine ne sont pas listés.",
            r.too_big_count
        );
    }
    if !r.notes.is_empty() {
        out.push_str("\n## Remarques\n\n");
        for n in &r.notes {
            let _ = writeln!(out, "- {}", md_cell(n));
        }
    }
    out.push_str(
        "\n_Estimations à ±30 % ; tok/s = vitesse de génération (traitement du prompt non \
         modélisé). La qualité est une heuristique (taille, quantification, récence), pas un \
         benchmark._\n",
    );
    out
}

fn csv_field(text: &str) -> String {
    if text.contains([';', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

fn csv_num(x: f32, decimals: usize) -> String {
    num(f64::from(x), decimals)
}

const CSV_HEADER: &str = "section;rang;modele;depot;depot_quantification;quantification;\
    taille_go;bits_par_poids;mode;couches_gpu;couches;vram_go;ram_go;tok_s;ctx;ctx_max;score;\
    qualite;vitesse;marge;contexte;adequation;approximatif;telechargements;explication";

fn csv_rows(out: &mut String, section: &str, items: &[RankItem]) {
    use crate::estimate::RunMode;
    for (i, it) in items.iter().enumerate() {
        let (v, e) = (&it.best.variant, &it.best.estimate);
        let (gpu_layers, layers) = match e.mode {
            RunMode::Hybrid { gpu_layers, layers } => (gpu_layers.to_string(), layers.to_string()),
            _ => (String::new(), String::new()),
        };
        let fields = [
            section.to_owned(),
            (i + 1).to_string(),
            csv_field(&it.display_name),
            csv_field(&it.repo_id),
            csv_field(&v.repo_id),
            csv_field(&v.label),
            gb_value(v.size_bytes),
            csv_num(v.bpw, 2),
            csv_field(&e.mode.label()),
            gpu_layers,
            layers,
            gb_value(e.mem_gpu),
            gb_value(e.mem_cpu),
            csv_num(e.tok_s, 1),
            e.ctx_used.to_string(),
            e.ctx_max.to_string(),
            csv_num(it.score.total, 1),
            csv_num(it.score.quality, 1),
            csv_num(it.score.speed, 1),
            csv_num(it.score.fit, 1),
            csv_num(it.score.context, 1),
            e.fit.label().to_owned(),
            if e.approximate { "oui" } else { "non" }.to_owned(),
            it.downloads.to_string(),
            csv_field(&it.explanation),
        ];
        out.push_str(&fields.join(";"));
        out.push_str("\r\n");
    }
}

/// Classement en CSV (une ligne par modèle, toutes sections confondues).
pub fn to_csv(r: &RankResponse) -> String {
    let mut out = String::from("\u{feff}");
    out.push_str(CSV_HEADER);
    out.push_str("\r\n");
    csv_rows(&mut out, "executable", &r.items);
    csv_rows(&mut out, "lent", &r.slow);
    csv_rows(&mut out, "trop gros", &r.too_big);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn champs_csv() {
        assert_eq!(csv_field("Qwen3-8B"), "Qwen3-8B");
        assert_eq!(csv_field("a;b"), "\"a;b\"");
        assert_eq!(csv_field("dit \"x\""), "\"dit \"\"x\"\"\"");
        assert_eq!(CSV_HEADER.split(';').count(), 25);
    }

    #[test]
    fn cellules_markdown() {
        assert_eq!(md_cell("a | b\nc"), "a \\| b c");
    }
}
