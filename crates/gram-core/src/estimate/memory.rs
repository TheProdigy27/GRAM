//! Paramètres de l'estimation, mémoire utilisable (§6.5) et cache KV (§8.1).

use crate::hardware::{DEFAULT_GPU_BANDWIDTH_GBS, DEFAULT_RAM_BANDWIDTH_GBS, HardwareProfile};
use crate::model::{ArchInfo, ArchSource, ModelEntry};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;
const KIB_F: f64 = 1024.0;

/// La VRAM libre peut dépasser un peu la valeur lue : elle varie d'un instant à l'autre.
pub const VRAM_FREE_TOLERANCE: u64 = 256 * MIB;

/// Mode « je ferme tout avant de lancer le modèle » : RAM laissée à Windows.
pub const CLOSE_APPS_RAM_RESERVE: u64 = 4 * GIB;

/// Réglages de l'estimation (§12 du plan), tous modifiables dans les Paramètres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct EstimateParams {
    /// Part de la VRAM gardée pour l'affichage Windows et les applications ouvertes.
    pub vram_margin: f32,
    /// Part de la RAM totale utilisable (et au plus la RAM disponible).
    pub ram_share: f32,
    /// « Je ferme tout avant de lancer le modèle » : RAM totale moins 4 Go, VRAM sans tenir
    /// compte de l'occupation actuelle.
    pub close_apps: bool,
    /// Surcoût du runtime (contexte CUDA/Vulkan, buffers de calcul), en octets.
    pub overhead_bytes: u64,
    /// Facteur d'efficacité de la vitesse (fraction de la bande passante réellement exploitée).
    pub efficiency: f32,
    pub kv_cache: KvCacheType,
    /// Les quantifications sous ce nombre de bits par poids ne sont pas proposées.
    pub min_bpw: f32,
    /// Contexte minimal, en tokens, quand il faut réduire le contexte pour tenir en mémoire.
    pub min_ctx: u32,
}

impl Default for EstimateParams {
    fn default() -> Self {
        Self {
            vram_margin: 0.10,
            ram_share: 0.75,
            close_apps: false,
            overhead_bytes: 512 * MIB,
            efficiency: 0.55,
            kv_cache: KvCacheType::F16,
            min_bpw: 2.5,
            min_ctx: 2048,
        }
    }
}

/// Type du cache KV (`--cache-type-k/v` de llama.cpp, `OLLAMA_KV_CACHE_TYPE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum KvCacheType {
    #[default]
    #[serde(rename = "f16")]
    F16,
    #[serde(rename = "q8_0")]
    Q8_0,
    #[serde(rename = "q4_0")]
    Q4_0,
}

impl KvCacheType {
    /// Octets par élément, en fraction exacte : les types quantifiés stockent 32 éléments par
    /// bloc (34 octets pour q8_0, 18 pour q4_0).
    const fn bytes_ratio(self) -> (u64, u64) {
        match self {
            Self::F16 => (2, 1),
            Self::Q8_0 => (34, 32),
            Self::Q4_0 => (18, 32),
        }
    }

    pub fn bytes_per_element(self) -> f64 {
        let (num, den) = self.bytes_ratio();
        num as f64 / den as f64
    }

    /// Nom affiché et attendu par les runtimes.
    pub const fn label(self) -> &'static str {
        match self {
            Self::F16 => "f16",
            Self::Q8_0 => "q8_0",
            Self::Q4_0 => "q4_0",
        }
    }
}

/// Mémoire utilisable et bandes passantes de la machine, calculées une fois par classement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MemoryBudget {
    /// VRAM utilisable du GPU retenu, en octets ; 0 sans GPU discret (un iGPU partage la RAM).
    pub vram: u64,
    /// RAM utilisable, en octets.
    pub ram: u64,
    /// Bande passante du GPU retenu en Go/s ; absente sans GPU discret.
    pub gpu_bandwidth_gbs: Option<f32>,
    /// Bande passante de la RAM en Go/s (valeur prudente si la vitesse est inconnue).
    pub ram_bandwidth_gbs: f32,
}

impl MemoryBudget {
    /// §6.5 : `VRAM_u = min(VRAM × (1 − marge), VRAM libre + tolérance)` et
    /// `RAM_u = min(RAM disponible, RAM × part)`. En mode « je ferme tout », la VRAM ne dépend
    /// plus de son occupation actuelle et la RAM passe à `RAM − 4 Go` (sans jamais baisser).
    pub fn from_profile(hw: &HardwareProfile, p: &EstimateParams) -> Self {
        let gpu = hw
            .gpu
            .as_ref()
            .filter(|g| !g.integrated && g.vram_total > 0);
        let margin = f64::from(p.vram_margin.clamp(0.0, 0.9));
        let vram = gpu.map_or(0, |g| {
            let cap = (g.vram_total as f64 * (1.0 - margin)) as u64;
            match g.vram_free {
                Some(free) if !p.close_apps => cap.min(free.saturating_add(VRAM_FREE_TOLERANCE)),
                _ => cap,
            }
        });

        let share = (hw.ram.total as f64 * f64::from(p.ram_share.clamp(0.0, 1.0))) as u64;
        let mut ram = match hw.ram.available {
            0 => share,
            available => share.min(available),
        };
        if p.close_apps {
            ram = ram.max(hw.ram.total.saturating_sub(CLOSE_APPS_RAM_RESERVE));
        }

        Self {
            vram,
            ram,
            gpu_bandwidth_gbs: gpu.map(|g| {
                g.bandwidth_gbs
                    .filter(|&b| b > 0.0)
                    .unwrap_or(DEFAULT_GPU_BANDWIDTH_GBS)
            }),
            ram_bandwidth_gbs: hw
                .ram
                .bandwidth_gbs
                .filter(|&b| b > 0.0)
                .unwrap_or(DEFAULT_RAM_BANDWIDTH_GBS),
        }
    }

    /// Peut-on placer des couches sur un GPU discret ?
    pub fn gpu_offload(&self) -> bool {
        self.vram > 0 && self.gpu_bandwidth_gbs.is_some()
    }
}

/// §8.1 : `2 × couches × têtes KV × head_dim × octets par élément`. Seules les couches qui ont
/// un cache KV comptent (modèles hybrides attention + Mamba / attention linéaire). L'attention
/// à fenêtre glissante n'est pas prise en compte : le résultat est alors surestimé (prudent).
pub fn kv_bytes_per_token(arch: &ArchInfo, kv: KvCacheType) -> u64 {
    let layers = u64::from(arch.kv_layers.unwrap_or(arch.n_layers));
    let elements = 2 * layers * u64::from(arch.n_kv_heads) * u64::from(arch.head_dim);
    let (num, den) = kv.bytes_ratio();
    (elements * num).div_ceil(den)
}

/// Cache KV par token quand l'architecture est inconnue (§7.4, repli) : en f16,
/// `max(144 Kio, 64 Kio × P^0,4)` avec `P` en milliards de paramètres (effectifs pour un MoE).
/// Au-dessus des valeurs réelles des modèles denses courants (Llama-3.1-8B : 128 Kio,
/// Qwen3-32B : 256 Kio, Llama-3.3-70B : 320 Kio), petits modèles compris.
pub fn fallback_kv_bytes_per_token(params_eff: f64, kv: KvCacheType) -> u64 {
    let billions = (params_eff / 1e9).max(0.0);
    let f16 = (64.0 * KIB_F * billions.powf(0.4)).max(144.0 * KIB_F);
    (f16 * kv.bytes_per_element() / 2.0).ceil() as u64
}

/// Nombre de couches quand l'architecture est inconnue : `34 × (P / 8)^0,39`, ajusté sur les
/// modèles denses courants (8B : 32 à 36 couches, 70B : 80). Ne sert qu'au nombre de couches
/// sur le GPU en mode hybride.
pub fn fallback_layers(params: u64) -> u32 {
    let billions = params as f64 / 1e9;
    (34.0 * (billions / 8.0).powf(0.39))
        .round()
        .clamp(16.0, 160.0) as u32
}

/// Ce qu'il faut savoir d'un modèle pour placer ses quantifications.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ModelShape {
    pub kv_per_token: u64,
    /// Couches du modèle (option `-ngl` en hybride).
    pub layers: u32,
    /// Paramètres actifs / paramètres totaux (1 pour un modèle dense).
    pub active_ratio: f64,
    /// `false` : KV et couches estimés par le repli.
    pub arch_known: bool,
    pub sliding_window: bool,
}

impl ModelShape {
    /// `weights` (taille de la variante) remplace le nombre de paramètres s'il est inconnu.
    pub fn of(model: &ModelEntry, weights: u64, kv: KvCacheType) -> Self {
        let params = match model.params_total {
            0 => weights.saturating_mul(2), // ≈ 4 bits par poids
            p => p,
        };
        let active = model.params_active.filter(|&a| a > 0 && a < params);
        let active_ratio = active.map_or(1.0, |a| a as f64 / params as f64);

        if let Some(a) = model.arch.as_ref().filter(|a| usable(a)) {
            return Self {
                kv_per_token: kv_bytes_per_token(a, kv),
                layers: a.n_layers,
                active_ratio,
                arch_known: a.source != ArchSource::Fallback,
                sliding_window: a.sliding_window.is_some_and(|w| w > 0),
            };
        }
        // MoE : √(total × actifs), comme pour la qualité (§9).
        let params_eff = active.map_or(params as f64, |a| (params as f64 * a as f64).sqrt());
        Self {
            kv_per_token: fallback_kv_bytes_per_token(params_eff, kv),
            layers: fallback_layers(params),
            active_ratio,
            arch_known: false,
            sliding_window: false,
        }
    }
}

fn usable(a: &ArchInfo) -> bool {
    a.n_layers > 0 && a.n_kv_heads > 0 && a.head_dim > 0
}
