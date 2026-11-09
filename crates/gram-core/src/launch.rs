//! Runtimes et commandes de lancement (§10 du plan).
//!
//! Toutes les règles propres à Ollama, LM Studio et llama.cpp sont ici : leur comportement
//! évolue souvent (§16), elles doivent rester faciles à mettre à jour. Le runtime ne change pas
//! l'estimation ; il change la commande et les avertissements affichés.

use crate::estimate::{Estimate, KvCacheType, MemoryBudget, RunMode};
use crate::fmt::thousands;
use crate::hardware::HardwareProfile;
use crate::model::QuantVariant;
use crate::runtime::{RuntimeStatus, RuntimesStatus};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

const GIB: u64 = 1 << 30;

/// Runtime choisi pour lancer les modèles (§10.1 : Ollama par défaut).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Runtime {
    #[default]
    Ollama,
    LmStudio,
    LlamaCpp,
}

impl Runtime {
    pub const ALL: [Runtime; 3] = [Runtime::Ollama, Runtime::LmStudio, Runtime::LlamaCpp];

    /// Nom affiché.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ollama => "Ollama",
            Self::LmStudio => "LM Studio",
            Self::LlamaCpp => "llama.cpp",
        }
    }

    fn status(self, s: &RuntimesStatus) -> &RuntimeStatus {
        match self {
            Self::Ollama => &s.ollama,
            Self::LmStudio => &s.lm_studio,
            Self::LlamaCpp => &s.llama_cpp,
        }
    }
}

/// Une ligne de commande à copier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CommandLine {
    /// À quoi sert la commande (« Lancer le modèle », « Puis, dans la session »…).
    pub label: String,
    pub command: String,
}

/// Comment lancer une quantification avec un runtime (fiche détail, bouton « Copier »).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LaunchPlan {
    pub runtime: Runtime,
    /// Commandes, dans l'ordre (vide pour LM Studio, qui passe par la page Hugging Face).
    pub commands: Vec<CommandLine>,
    /// Page à ouvrir dans le navigateur (LM Studio : menu « Use this model »).
    pub open_url: Option<String>,
    /// Réglages et précautions, en français.
    pub notes: Vec<String>,
}

/// Page Hugging Face d'un dépôt.
pub fn hf_url(repo_id: &str) -> String {
    format!("https://huggingface.co/{repo_id}")
}

/// Couches à placer sur le GPU (`-ngl` de llama.cpp) : toutes (999) en mode GPU.
pub fn gpu_layers_arg(mode: RunMode) -> u32 {
    match mode {
        RunMode::Gpu => 999,
        RunMode::Hybrid { gpu_layers, .. } => gpu_layers,
        RunMode::Cpu | RunMode::TooBig => 0,
    }
}

/// Commandes et réglages pour lancer `variant` au contexte retenu par l'estimation (§10.3).
pub fn launch_plan(
    runtime: Runtime,
    variant: &QuantVariant,
    estimate: &Estimate,
    gated: bool,
    kv: KvCacheType,
) -> LaunchPlan {
    let ctx = estimate.ctx_used;
    let tag = format!("{}:{}", variant.repo_id, variant.label);
    let mut commands = Vec::new();
    let mut open_url = None;
    let mut notes = Vec::new();
    if estimate.mode == RunMode::TooBig {
        notes.push(
            "Ne tient pas en mémoire sur cette machine : le lancement risque d'échouer.".into(),
        );
    }

    match runtime {
        Runtime::Ollama => {
            commands.push(CommandLine {
                label: "Lancer le modèle".into(),
                command: format!("ollama run hf.co/{tag}"),
            });
            commands.push(CommandLine {
                label: "Puis, dans la session".into(),
                command: format!("/set parameter num_ctx {ctx}"),
            });
            commands.push(CommandLine {
                label: "Ou, pour tous les modèles (puis relancer Ollama)".into(),
                command: format!("setx OLLAMA_CONTEXT_LENGTH {ctx}"),
            });
            if kv != KvCacheType::F16 {
                // Deux commandes : `&&` n'existe pas dans Windows PowerShell 5.1.
                commands.push(CommandLine {
                    label: format!("Cache KV {} : Flash Attention requise", kv.label()),
                    command: "setx OLLAMA_FLASH_ATTENTION 1".into(),
                });
                commands.push(CommandLine {
                    label: format!("Cache KV {} (puis relancer Ollama)", kv.label()),
                    command: format!("setx OLLAMA_KV_CACHE_TYPE {}", kv.label()),
                });
            }
            if matches!(estimate.mode, RunMode::Hybrid { .. }) {
                notes.push(
                    "Ollama répartit seul les couches entre la VRAM et la RAM (`ollama ps` \
                     affiche la répartition)."
                        .into(),
                );
            }
            if variant.files.len() > 1 {
                notes.push(format!(
                    "Modèle découpé en {} fichiers : Ollama peut refuser de le charger ; \
                     llama.cpp le gère.",
                    variant.files.len()
                ));
            }
            if gated {
                notes.push(
                    "Dépôt protégé : accepter ses conditions sur Hugging Face, puis ajouter la \
                     clé SSH d'Ollama à votre compte Hugging Face."
                        .into(),
                );
            }
        }
        Runtime::LlamaCpp => {
            let mut command = format!(
                "llama-server -hf {tag} -c {ctx} -ngl {}",
                gpu_layers_arg(estimate.mode)
            );
            if kv != KvCacheType::F16 {
                command.push_str(&format!(" -fa on -ctk {0} -ctv {0}", kv.label()));
            }
            commands.push(CommandLine {
                label: "Lancer le serveur (interface sur http://localhost:8080)".into(),
                command,
            });
            if gated {
                notes.push(
                    "Dépôt protégé : accepter ses conditions sur Hugging Face et définir la \
                     variable HF_TOKEN avant de lancer llama-server."
                        .into(),
                );
            }
        }
        Runtime::LmStudio => {
            open_url = Some(hf_url(&variant.repo_id));
            notes.push(format!(
                "Sur la page Hugging Face : « Use this model » → LM Studio, puis choisir la \
                 quantification {}.",
                variant.label
            ));
            notes.push(format!(
                "Au chargement, régler la longueur de contexte sur {} tokens.",
                thousands(u64::from(ctx))
            ));
            notes.push(match estimate.mode {
                RunMode::Gpu => "Déchargement GPU (GPU offload) : toutes les couches.".into(),
                RunMode::Hybrid { gpu_layers, layers } => {
                    format!("Déchargement GPU (GPU offload) : {gpu_layers} couches sur {layers}.")
                }
                RunMode::Cpu | RunMode::TooBig => "Déchargement GPU (GPU offload) : 0.".into(),
            });
            if kv != KvCacheType::F16 {
                notes.push(format!(
                    "Activer Flash Attention et choisir le cache KV {}.",
                    kv.label()
                ));
            }
        }
    }
    LaunchPlan {
        runtime,
        commands,
        open_url,
        notes,
    }
}

/// Contexte qu'Ollama choisit seul selon la VRAM (§10.2) : 4k sous 24 Gio, 32k jusqu'à
/// 48 Gio, 256k au-delà. Les seuils sont nominaux : une carte de 24 Go en annonce un peu moins
/// (23,99 Gio pour une RTX 4090), d'où une tolérance de 1 Gio.
pub fn ollama_default_ctx(vram_total: u64) -> u32 {
    match vram_total.saturating_add(GIB) {
        v if v < 24 * GIB => 4096,
        v if v < 48 * GIB => 32_768,
        _ => 262_144,
    }
}

/// Avertissements généraux pour le runtime choisi (§10.2), affichés au-dessus du classement.
/// `runtimes` vaut `None` tant que la détection des runtimes n'a pas eu lieu.
pub fn runtime_notes(
    runtime: Runtime,
    hw: &HardwareProfile,
    budget: &MemoryBudget,
    runtimes: Option<&RuntimesStatus>,
) -> Vec<String> {
    let mut notes = Vec::new();
    let igpu_only = !budget.gpu_offload() && hw.all_gpus().any(|g| g.integrated);

    if let Some(rt) = runtimes
        && !runtime.status(rt).installed
    {
        let others: Vec<&str> = Runtime::ALL
            .into_iter()
            .filter(|&r| r != runtime && r.status(rt).installed)
            .map(Runtime::label)
            .collect();
        notes.push(match others.as_slice() {
            [] => format!(
                "{} n'est pas installé, ni aucun autre runtime : installer Ollama depuis \
                 https://ollama.com.",
                runtime.label()
            ),
            [one] => format!(
                "{} n'est pas installé ; {one} l'est : le choisir dans le filtre Runtime ?",
                runtime.label()
            ),
            many => format!(
                "{} n'est pas installé ; {} le sont.",
                runtime.label(),
                many.join(" et ")
            ),
        });
    }

    match runtime {
        Runtime::Ollama => {
            let vram = hw
                .gpu
                .as_ref()
                .filter(|g| !g.integrated)
                .map_or(0, |g| g.vram_total);
            let default = ollama_default_ctx(vram);
            notes.push(format!(
                "Sans réglage, Ollama choisit le contexte selon la VRAM ({} tokens sur cette \
                 machine) : régler num_ctx sur le contexte indiqué, sinon la mémoire utilisée \
                 diffère de l'estimation.",
                thousands(u64::from(default))
            ));
            if igpu_only {
                notes.push(
                    "Ollama ignore les GPU intégrés par défaut : LM Studio (Vulkan) peut \
                     utiliser celui-ci."
                        .into(),
                );
            }
            notes.push(
                "Une architecture très récente peut être prise en charge par llama.cpp avant \
                 Ollama : en cas d'échec au lancement, essayer llama.cpp."
                    .into(),
            );
        }
        Runtime::LlamaCpp | Runtime::LmStudio if igpu_only => {
            notes.push(format!(
                "{} peut utiliser le GPU intégré avec Vulkan : l'estimation le compte comme du \
                 CPU (même mémoire, même bande passante).",
                runtime.label()
            ));
        }
        Runtime::LlamaCpp | Runtime::LmStudio => {}
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::estimate::{FitLevel, MemoryBreakdown};

    fn variant(files: usize) -> QuantVariant {
        QuantVariant {
            label: "Q4_K_M".into(),
            repo_id: "bartowski/Qwen_Qwen3-8B-GGUF".into(),
            files: (1..=files)
                .map(|i| format!("Qwen3-8B-Q4_K_M-0000{i}-of-0000{files}.gguf"))
                .collect(),
            size_bytes: 5 << 30,
            bpw: 4.9,
            exact: true,
        }
    }

    fn estimate(mode: RunMode) -> Estimate {
        Estimate {
            mode,
            mem_gpu: 0,
            mem_cpu: 0,
            breakdown: MemoryBreakdown {
                weights: 5 << 30,
                kv: 1 << 30,
                overhead: 1 << 29,
                vision: 0,
            },
            kv_per_token: 144 << 10,
            mem_usage: 0.6,
            ctx_used: 16_384,
            ctx_max: 32_768,
            tok_s: 40.0,
            fit: FitLevel::Perfect,
            approximate: false,
            warnings: vec![],
        }
    }

    fn commands(p: &LaunchPlan) -> Vec<&str> {
        p.commands.iter().map(|c| c.command.as_str()).collect()
    }

    #[test]
    fn ollama() {
        let p = launch_plan(
            Runtime::Ollama,
            &variant(1),
            &estimate(RunMode::Gpu),
            false,
            KvCacheType::F16,
        );
        assert_eq!(
            commands(&p),
            [
                "ollama run hf.co/bartowski/Qwen_Qwen3-8B-GGUF:Q4_K_M",
                "/set parameter num_ctx 16384",
                "setx OLLAMA_CONTEXT_LENGTH 16384",
            ]
        );
        assert!(p.notes.is_empty() && p.open_url.is_none());

        // Hybride, découpé, protégé, cache KV quantifié.
        let p = launch_plan(
            Runtime::Ollama,
            &variant(3),
            &estimate(RunMode::Hybrid {
                gpu_layers: 20,
                layers: 36,
            }),
            true,
            KvCacheType::Q8_0,
        );
        assert_eq!(
            commands(&p)[3..],
            [
                "setx OLLAMA_FLASH_ATTENTION 1",
                "setx OLLAMA_KV_CACHE_TYPE q8_0"
            ]
        );
        assert_eq!(p.notes.len(), 3, "{:?}", p.notes);
        assert!(p.notes[1].contains("3 fichiers"));
        assert!(p.notes[2].contains("protégé"));
    }

    #[test]
    fn llama_cpp_par_mode() {
        let cmd = |mode, kv| {
            launch_plan(Runtime::LlamaCpp, &variant(1), &estimate(mode), false, kv).commands[0]
                .command
                .clone()
        };
        let base = "llama-server -hf bartowski/Qwen_Qwen3-8B-GGUF:Q4_K_M -c 16384";
        assert_eq!(
            cmd(RunMode::Gpu, KvCacheType::F16),
            format!("{base} -ngl 999")
        );
        assert_eq!(
            cmd(
                RunMode::Hybrid {
                    gpu_layers: 37,
                    layers: 80
                },
                KvCacheType::F16
            ),
            format!("{base} -ngl 37")
        );
        assert_eq!(
            cmd(RunMode::Cpu, KvCacheType::F16),
            format!("{base} -ngl 0")
        );
        assert_eq!(
            cmd(RunMode::Gpu, KvCacheType::Q4_0),
            format!("{base} -ngl 999 -fa on -ctk q4_0 -ctv q4_0")
        );
        let gated = launch_plan(
            Runtime::LlamaCpp,
            &variant(1),
            &estimate(RunMode::Gpu),
            true,
            KvCacheType::F16,
        );
        assert!(gated.notes[0].contains("HF_TOKEN"));
    }

    #[test]
    fn lm_studio() {
        let p = launch_plan(
            Runtime::LmStudio,
            &variant(1),
            &estimate(RunMode::Hybrid {
                gpu_layers: 20,
                layers: 36,
            }),
            false,
            KvCacheType::F16,
        );
        assert!(p.commands.is_empty());
        assert_eq!(
            p.open_url.as_deref(),
            Some("https://huggingface.co/bartowski/Qwen_Qwen3-8B-GGUF")
        );
        assert!(p.notes[0].contains("Q4_K_M"));
        assert!(p.notes[1].contains("16 384 tokens"));
        assert!(p.notes[2].contains("20 couches sur 36"));

        let too_big = launch_plan(
            Runtime::LmStudio,
            &variant(1),
            &estimate(RunMode::TooBig),
            false,
            KvCacheType::F16,
        );
        assert!(too_big.notes[0].contains("Ne tient pas"));
    }

    #[test]
    fn contexte_par_defaut_d_ollama() {
        assert_eq!(ollama_default_ctx(0), 4096);
        assert_eq!(ollama_default_ctx(16 * GIB), 4096);
        assert_eq!(ollama_default_ctx(24 * GIB), 32_768);
        // RTX 4090 : 24 564 Mio annoncés.
        assert_eq!(ollama_default_ctx(24_564 << 20), 32_768);
        assert_eq!(ollama_default_ctx(20 * GIB), 4096);
        assert_eq!(ollama_default_ctx(48 * GIB), 262_144);
    }
}
