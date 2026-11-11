//! Paramètres utilisateur (§12 du plan), tous modifiables dans l'interface et enregistrés dans
//! `settings.json` du dossier de l'application (`%LOCALAPPDATA%\fr.gram.desktop` sous Windows,
//! `~/Library/Application Support/fr.gram.desktop` sous macOS), sans le token Hugging Face.

use crate::estimate::EstimateParams;
use crate::launch::Runtime;
use crate::model::UseCase;
use crate::score::ScoreWeights;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

const GIB: u64 = 1 << 30;

/// Contexte le plus long proposé (curseur de l'interface : 2k → 128k, saisie libre au-delà).
pub const MAX_CTX: u32 = 1 << 20;

/// Durée de vie maximale du catalogue en cache : 30 jours.
pub const MAX_CATALOG_TTL_HOURS: u32 = 720;

/// Groupes détaillés au plus par synchronisation : environ 600 appels à l'API, au-dessus du
/// quota anonyme de Hugging Face (500 par 5 minutes) ; un token est alors nécessaire.
pub const MAX_DETAIL_GROUPS: u32 = 400;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Settings {
    /// Runtime des commandes générées (§10).
    pub runtime: Runtime,
    /// Marges, efficacités, surcoût, cache KV, minimums.
    pub estimate: EstimateParams,
    /// Contexte, seuil de lenteur et poids du score de chaque cas d'usage.
    pub use_cases: UseCaseProfiles,
    /// Téléchargements minimum d'un dépôt à la synchronisation (sauf s'il est en tendance).
    pub min_downloads: u64,
    /// Durée de vie du catalogue en cache, en heures : au-delà, le lancement le resynchronise.
    pub catalog_ttl_hours: u32,
    /// Groupes de modèles dont la synchronisation charge les tailles réelles (quota de l'API).
    pub detail_groups: u32,
    /// Lignes affichées avant « Voir plus ».
    pub page_size: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            runtime: Runtime::Ollama,
            estimate: EstimateParams::default(),
            use_cases: UseCaseProfiles::default(),
            min_downloads: 500,
            catalog_ttl_hours: 24,
            detail_groups: 150,
            page_size: 25,
        }
    }
}

/// Réglages d'un cas d'usage.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UseCaseProfile {
    /// Contexte souhaité par défaut, en tokens.
    pub ctx: u32,
    /// Sous cette vitesse (tok/s), le modèle passe dans « Exécutable mais lent ».
    pub slow_tok_s: f32,
    pub weights: ScoreWeights,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct UseCaseProfiles {
    pub general: UseCaseProfile,
    pub code: UseCaseProfile,
    pub reasoning: UseCaseProfile,
    pub fast_chat: UseCaseProfile,
    pub vision: UseCaseProfile,
}

impl Default for UseCaseProfiles {
    /// §9 (poids, inspirés de llmfit) et §12 (contextes et seuils).
    fn default() -> Self {
        let p = |ctx, slow_tok_s, q, v, f, c| UseCaseProfile {
            ctx,
            slow_tok_s,
            weights: ScoreWeights::new(q, v, f, c),
        };
        Self {
            general: p(8192, 5.0, 0.45, 0.30, 0.15, 0.10),
            code: p(16_384, 5.0, 0.50, 0.20, 0.10, 0.20),
            reasoning: p(16_384, 8.0, 0.55, 0.20, 0.10, 0.15),
            fast_chat: p(4096, 10.0, 0.30, 0.50, 0.15, 0.05),
            vision: p(8192, 5.0, 0.45, 0.25, 0.15, 0.15),
        }
    }
}

impl UseCaseProfiles {
    pub fn get(&self, use_case: UseCase) -> &UseCaseProfile {
        match use_case {
            UseCase::General => &self.general,
            UseCase::Code => &self.code,
            UseCase::Reasoning => &self.reasoning,
            UseCase::FastChat => &self.fast_chat,
            UseCase::Vision => &self.vision,
        }
    }

    fn all_mut(&mut self) -> [(UseCase, &mut UseCaseProfile); 5] {
        [
            (UseCase::General, &mut self.general),
            (UseCase::Code, &mut self.code),
            (UseCase::Reasoning, &mut self.reasoning),
            (UseCase::FastChat, &mut self.fast_chat),
            (UseCase::Vision, &mut self.vision),
        ]
    }
}

fn finite_clamp(x: f32, min: f32, max: f32, default: f32) -> f32 {
    if x.is_finite() {
        x.clamp(min, max)
    } else {
        default
    }
}

impl Settings {
    /// Ramène chaque valeur dans un intervalle raisonnable (fichier modifié à la main, saisie
    /// hors bornes). Des poids invalides (négatifs, tous nuls) reprennent leur valeur par défaut.
    pub fn sanitized(mut self) -> Self {
        let d = EstimateParams::default();
        let e = &mut self.estimate;
        e.vram_margin = finite_clamp(e.vram_margin, 0.0, 0.9, d.vram_margin);
        e.ram_share = finite_clamp(e.ram_share, 0.1, 1.0, d.ram_share);
        e.overhead_bytes = e.overhead_bytes.min(16 * GIB);
        e.gpu_efficiency = finite_clamp(e.gpu_efficiency, 0.05, 2.0, d.gpu_efficiency);
        e.ram_efficiency = finite_clamp(e.ram_efficiency, 0.05, 2.0, d.ram_efficiency);
        e.token_overhead_ms = finite_clamp(e.token_overhead_ms, 0.0, 100.0, d.token_overhead_ms);
        e.min_bpw = finite_clamp(e.min_bpw, 0.0, 16.0, d.min_bpw);
        e.min_ctx = e.min_ctx.clamp(256, 131_072);
        let min_ctx = e.min_ctx;

        let defaults = UseCaseProfiles::default();
        for (use_case, p) in self.use_cases.all_mut() {
            let def = defaults.get(use_case);
            p.ctx = p.ctx.clamp(min_ctx, MAX_CTX);
            p.slow_tok_s = finite_clamp(p.slow_tok_s, 0.0, 1000.0, def.slow_tok_s);
            if !p.weights.is_valid() {
                p.weights = def.weights;
            }
        }
        self.catalog_ttl_hours = self.catalog_ttl_hours.clamp(1, MAX_CATALOG_TTL_HOURS);
        self.detail_groups = self.detail_groups.min(MAX_DETAIL_GROUPS);
        self.page_size = self.page_size.clamp(5, 500);
        self
    }

    /// Contexte souhaité : celui demandé, sinon celui du cas d'usage ; au moins le contexte
    /// minimal et au plus [`MAX_CTX`].
    pub fn ctx_for(&self, use_case: UseCase, requested: Option<u32>) -> u32 {
        requested
            .unwrap_or(self.use_cases.get(use_case).ctx)
            .clamp(self.estimate.min_ctx.max(1), MAX_CTX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valeurs_par_defaut_du_plan() {
        let s = Settings::default();
        assert_eq!(s.runtime, Runtime::Ollama);
        assert_eq!(s.use_cases.get(UseCase::General).ctx, 8192);
        assert_eq!(s.use_cases.get(UseCase::FastChat).ctx, 4096);
        assert_eq!(s.use_cases.get(UseCase::Code).ctx, 16_384);
        assert_eq!(s.use_cases.get(UseCase::Reasoning).slow_tok_s, 8.0);
        assert_eq!(s.use_cases.get(UseCase::FastChat).slow_tok_s, 10.0);
        assert_eq!(s.use_cases.get(UseCase::Vision).slow_tok_s, 5.0);
        for uc in [
            UseCase::General,
            UseCase::Code,
            UseCase::Reasoning,
            UseCase::FastChat,
            UseCase::Vision,
        ] {
            let w = s.use_cases.get(uc).weights;
            assert!((w.sum() - 1.0).abs() < 1e-6, "{uc:?}");
        }
        assert_eq!((s.min_downloads, s.page_size), (500, 25));
        assert_eq!((s.catalog_ttl_hours, s.detail_groups), (24, 150));
        assert_eq!(s.clone().sanitized(), s, "les défauts sont déjà valides");
    }

    #[test]
    fn json_partiel_complete_par_les_defauts() {
        let s: Settings = serde_json::from_str(
            r#"{ "runtime": "llamaCpp", "useCases": { "code": { "ctx": 32768, "slowTokS": 4,
                 "weights": { "quality": 1, "speed": 0, "fit": 0, "context": 0 } } },
                 "estimate": { "kvCache": "q8_0" } }"#,
        )
        .unwrap();
        assert_eq!(s.runtime, Runtime::LlamaCpp);
        assert_eq!(s.use_cases.code.ctx, 32_768);
        assert_eq!(s.use_cases.reasoning, UseCaseProfiles::default().reasoning);
        assert_eq!(s.estimate.kv_cache.label(), "q8_0");
        assert_eq!(s.estimate.gpu_efficiency, 0.81);
        assert_eq!(s.estimate.ram_efficiency, 0.62);
        assert_eq!(s.page_size, 25);
    }

    #[test]
    fn assainissement() {
        let mut s = Settings::default();
        s.estimate.vram_margin = 3.0;
        s.estimate.gpu_efficiency = f32::NAN;
        s.estimate.ram_efficiency = 9.0;
        s.estimate.min_ctx = 10;
        s.use_cases.general.ctx = 100;
        s.use_cases.code.ctx = u32::MAX;
        s.use_cases.fast_chat.weights = ScoreWeights::new(0.0, 0.0, 0.0, 0.0);
        s.use_cases.vision.slow_tok_s = -1.0;
        s.page_size = 0;
        s.catalog_ttl_hours = 0;
        s.detail_groups = 10_000;
        let s = s.sanitized();
        assert_eq!(s.estimate.vram_margin, 0.9);
        assert_eq!(s.estimate.gpu_efficiency, 0.81);
        assert_eq!(s.estimate.ram_efficiency, 2.0);
        assert_eq!(s.estimate.min_ctx, 256);
        assert_eq!(s.use_cases.general.ctx, 256);
        assert_eq!(s.use_cases.code.ctx, MAX_CTX);
        assert_eq!(
            s.use_cases.fast_chat.weights,
            UseCaseProfiles::default().fast_chat.weights
        );
        assert_eq!(s.use_cases.vision.slow_tok_s, 0.0);
        assert_eq!(s.page_size, 5);
        assert_eq!(s.catalog_ttl_hours, 1);
        assert_eq!(s.detail_groups, MAX_DETAIL_GROUPS);
    }

    #[test]
    fn contexte_demande() {
        let s = Settings::default();
        assert_eq!(s.ctx_for(UseCase::Code, None), 16_384);
        assert_eq!(s.ctx_for(UseCase::Code, Some(32_768)), 32_768);
        assert_eq!(s.ctx_for(UseCase::General, Some(10)), 2048);
        assert_eq!(s.ctx_for(UseCase::General, Some(u32::MAX)), MAX_CTX);
    }
}
