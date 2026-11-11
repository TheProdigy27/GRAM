//! Les trois conseils en tête du classement : meilleur choix, le plus rapide, le plus long
//! contexte.
//!
//! Ils sont choisis parmi les modèles exécutables (assez rapides, sous la taille maximale),
//! avant la recherche par nom et `limit` : chercher un modèle ne les change pas. Un modèle
//! n'est conseillé qu'une fois, et un conseil sans candidat est omis.
//!
//! - **Meilleur choix** : le premier du classement, dans sa meilleure quantification.
//! - **Le plus rapide** : la quantification la plus rapide parmi celles qui ne perdent pas
//!   beaucoup en qualité, au moins [`MIN_GAIN`] fois plus rapide que le meilleur choix.
//! - **Le plus long contexte** : la quantification au plus grand contexte maximal parmi les
//!   mêmes, avec au moins [`MIN_GAIN`] fois le contexte du meilleur choix ; à contexte égal, la
//!   mieux notée.
//!
//! Les quantifications candidates sont celles des modèles à moins de [`SCORE_GAP`] points de
//! score du meilleur choix, à moins de [`QUANT_GAP`] points de qualité de la quantification du
//! classement du même modèle, et à moins de [`QUALITY_GAP`] points de qualité du meilleur
//! choix. La qualité est l'axe du score, pas le total : sur une machine puissante, la vitesse
//! (plafonnée à 40 tok/s), la marge et le contexte sont souvent au maximum, si bien qu'un petit
//! modèle très rapide peut rester à quelques points du meilleur score avec une qualité bien
//! moindre. Le conseil peut retenir une autre quantification que le classement : gemma-4-12B-it
//! en IQ4_NL (~106 tok/s, tout en VRAM) plutôt qu'en UD-Q5_K_XL (~57 tok/s, hybride), pour
//! 3 points de qualité de moins.

use super::{Ranked, Setup, item};
use crate::estimate::{MemoryBudget, RunMode, VariantEstimate};
use crate::fmt::{ctx_k, num, tok_s};
use crate::launch::{LaunchPlan, Runtime, launch_plan};
use crate::rank::RankItem;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Écart de score (total) toléré entre le modèle d'un conseil et le meilleur choix, en points.
pub const SCORE_GAP: f32 = 6.0;

/// Perte de qualité (axe du score) tolérée entre la quantification d'un conseil et celle du
/// classement pour le même modèle, en points.
pub const QUANT_GAP: f32 = 3.0;

/// Perte de qualité (axe du score) tolérée entre un conseil et le meilleur choix, en points :
/// environ un modèle 2,4 fois plus petit à quantification égale.
pub const QUALITY_GAP: f32 = 15.0;

/// Gain minimal sur le meilleur choix : 25 % plus rapide, ou 25 % de contexte en plus.
pub const MIN_GAIN: f32 = 1.25;

/// Un conseil en tête du classement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RankPick {
    pub kind: PickKind,
    /// Le modèle conseillé. `item.best` est la quantification retenue par le conseil, pas
    /// forcément celle du classement ; `item.rank` est le rang du modèle parmi les exécutables.
    pub item: RankItem,
    /// Pourquoi ce conseil (« ~100 tok/s, tout en VRAM : 1,8× plus rapide que le meilleur
    /// choix »).
    pub reason: String,
    /// Lancement de cette quantification avec le runtime de la requête (bouton « Commande »).
    pub launch: LaunchPlan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PickKind {
    Best,
    Fastest,
    LongestContext,
}

impl PickKind {
    /// Nom affiché.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Best => "Meilleur choix",
            Self::Fastest => "Le plus rapide",
            Self::LongestContext => "Le plus long contexte",
        }
    }
}

/// Un modèle candidat dans la quantification retenue pour un conseil.
struct Candidate {
    /// Indice dans la liste des exécutables triée.
    model: usize,
    /// Indice de la quantification dans `Evaluation::rows`.
    row: usize,
}

/// Les conseils, `runnable` étant la liste complète des exécutables, du meilleur score au moins
/// bon.
pub(super) fn picks(runnable: &[Ranked], s: &Setup, runtime: Runtime) -> Vec<RankPick> {
    let Some(first) = runnable.first() else {
        return Vec::new();
    };
    let best = &first.item;
    let reason = format!(
        "Meilleur score ({}/100) : {} tok/s, {}",
        best.score.total.round(),
        tok_s(best.best.estimate.tok_s),
        placement(&best.best, &s.budget)
    );
    let mut out = vec![with_launch(
        PickKind::Best,
        best.clone(),
        reason,
        s,
        runtime,
    )];

    let speed = |ve: &VariantEstimate| ve.estimate.tok_s;
    if let Some(c) = choose(runnable, &out, speed)
        .filter(|c| row_of(runnable, c).estimate.tok_s >= MIN_GAIN * best.best.estimate.tok_s)
    {
        let ve = row_of(runnable, &c);
        let ratio = ve.estimate.tok_s / best.best.estimate.tok_s;
        let reason = format!(
            "{} tok/s, {} : {}× plus rapide que le meilleur choix",
            tok_s(ve.estimate.tok_s),
            placement(ve, &s.budget),
            num(f64::from(ratio), if ratio < 10.0 { 1 } else { 0 })
        );
        let item = candidate_item(runnable, &c, s);
        out.push(with_launch(PickKind::Fastest, item, reason, s, runtime));
    }

    let ctx = |ve: &VariantEstimate| ve.estimate.ctx_max as f32;
    let best_ctx = best.best.estimate.ctx_max;
    if let Some(c) = choose(runnable, &out, ctx)
        .filter(|c| row_of(runnable, c).estimate.ctx_max as f32 >= MIN_GAIN * best_ctx as f32)
    {
        let reason = format!(
            "Contexte jusqu'à {}, contre {} pour le meilleur choix",
            ctx_k(row_of(runnable, &c).estimate.ctx_max),
            ctx_k(best_ctx)
        );
        let item = candidate_item(runnable, &c, s);
        out.push(with_launch(
            PickKind::LongestContext,
            item,
            reason,
            s,
            runtime,
        ));
    }
    out
}

/// La quantification candidate (voir l'en-tête du module) qui maximise `value`, parmi les
/// modèles pas encore conseillés ; à valeur égale, la mieux notée, puis celle du modèle le mieux
/// classé, puis la plus petite.
fn choose(
    runnable: &[Ranked],
    picked: &[RankPick],
    value: impl Fn(&VariantEstimate) -> f32,
) -> Option<Candidate> {
    let best = &runnable.first()?.item;
    let score_floor = best.score.total - SCORE_GAP;
    let quality_floor = best.score.quality - QUALITY_GAP;
    let row = |c: &Candidate| &runnable[c.model].ev.rows[c.row];
    runnable
        .iter()
        .enumerate()
        .take_while(|(_, r)| r.item.score.total >= score_floor)
        .filter(|(_, r)| !picked.iter().any(|p| p.item.repo_id == r.item.repo_id))
        .flat_map(|(model, r)| {
            let floor = quality_floor.max(r.item.score.quality - QUANT_GAP);
            r.ev.rows
                .iter()
                .enumerate()
                .filter(move |(_, row)| {
                    row.runnable() && row.within_size && !row.slow && row.score.quality >= floor
                })
                .map(move |(row, _)| Candidate { model, row })
        })
        .max_by(|a, b| {
            let (ra, rb) = (row(a), row(b));
            value(&ra.ve)
                .total_cmp(&value(&rb.ve))
                .then(ra.score.total.total_cmp(&rb.score.total))
                .then(b.model.cmp(&a.model))
                .then(rb.ve.variant.size_bytes.cmp(&ra.ve.variant.size_bytes))
        })
}

fn row_of<'a>(runnable: &'a [Ranked], c: &Candidate) -> &'a VariantEstimate {
    &runnable[c.model].ev.rows[c.row].ve
}

/// La ligne du classement du candidat, dans la quantification retenue par le conseil.
fn candidate_item(runnable: &[Ranked], c: &Candidate, s: &Setup) -> RankItem {
    let r = &runnable[c.model];
    let mut it = item(r.model, &r.ev, c.row, s, r.item.family.clone());
    it.rank = r.item.rank;
    it
}

fn with_launch(
    kind: PickKind,
    item: RankItem,
    reason: String,
    s: &Setup,
    runtime: Runtime,
) -> RankPick {
    let (v, e) = (&item.best.variant, &item.best.estimate);
    RankPick {
        kind,
        launch: launch_plan(
            runtime,
            v,
            e,
            item.gated,
            s.params.kv_cache,
            s.cx.hardware.os,
        ),
        item,
        reason,
    }
}

/// « tout en VRAM », « 46 couches sur 48 en VRAM », « sur le CPU ».
fn placement(ve: &VariantEstimate, budget: &MemoryBudget) -> String {
    let vram = if budget.unified {
        "sur le GPU"
    } else {
        "en VRAM"
    };
    match ve.estimate.mode {
        RunMode::Gpu => format!("tout {vram}"),
        RunMode::Hybrid { gpu_layers, layers } => {
            format!("{gpu_layers} couches sur {layers} {vram}")
        }
        RunMode::Cpu => "sur le CPU".into(),
        RunMode::TooBig => "trop gros".into(),
    }
}
