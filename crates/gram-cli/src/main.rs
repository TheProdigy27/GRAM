//! `gram-cli` : binaire de debug pour valider chaque brique de GRAM sans l'interface.

mod estimate;
mod fmt;
mod hw;
mod sync;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use gram_core::{KvCacheType, UseCase};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "gram-cli", version, about = "Outils de debug de GRAM")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Affiche le matériel détecté et les runtimes installés.
    Hw {
        /// Sortie JSON : `{ hardware: HardwareProfile, runtimes: RuntimesStatus }`.
        #[arg(long)]
        json: bool,
        /// Données brutes de détection (JSON), à enregistrer comme fixture de test.
        #[arg(long, conflicts_with = "json")]
        raw: bool,
    },
    /// Synchronise le catalogue Hugging Face (token optionnel : variable HF_TOKEN).
    Sync {
        /// Ignore le catalogue en cache, même récent (les détails par commit restent utilisés).
        #[arg(long)]
        force: bool,
        /// Aucun appel réseau : catalogue du cache, sinon catalogue embarqué.
        #[arg(long, conflicts_with = "force")]
        offline: bool,
        /// Toutes les tranches de taille, sans tenir compte du matériel de cette machine.
        #[arg(long)]
        all_sizes: bool,
        /// Sortie JSON (`SyncSummary`).
        #[arg(long)]
        json: bool,
        /// Dossier du cache (défaut : %LOCALAPPDATA%\GRAM\cache).
        #[arg(long)]
        cache_dir: Option<PathBuf>,
    },
    /// Estime toutes les quantifications d'un modèle du catalogue (sans réseau).
    Estimate {
        /// Dépôt, nom ou modèle de base (recherche sans tenir compte de la casse).
        model: String,
        /// Contexte souhaité en tokens.
        #[arg(long, default_value_t = 8192)]
        ctx: u32,
        /// Usage vision : ajoute le projecteur (mmproj) au besoin mémoire.
        #[arg(long)]
        vision: bool,
        /// Type du cache KV.
        #[arg(long, value_enum, default_value_t = Kv::F16)]
        kv: Kv,
        /// Profil matériel JSON (`HardwareProfile`) à la place de cette machine,
        /// ex. tests/fixtures/profiles/p2-rtx3060-12go-32go-ddr4.json.
        #[arg(long)]
        profile: Option<PathBuf>,
        /// Sortie JSON (budget mémoire et estimation de chaque quantification).
        #[arg(long)]
        json: bool,
        /// Dossier du cache (défaut : %LOCALAPPDATA%\GRAM\cache).
        #[arg(long)]
        cache_dir: Option<PathBuf>,
    },
    /// Classe les modèles pour cette machine.
    Rank {
        #[arg(long, value_enum, default_value_t = Usage::General)]
        usage: Usage,
        /// Contexte souhaité en tokens (défaut : celui du cas d'usage).
        #[arg(long)]
        ctx: Option<u32>,
        #[arg(long)]
        json: bool,
    },
    /// Génère `data/seed_catalog.json` (catalogue embarqué pour le hors ligne).
    Snapshot {
        #[arg(long, default_value = "data/seed_catalog.json")]
        output: PathBuf,
        /// Dossier du cache (défaut : %LOCALAPPDATA%\GRAM\cache).
        #[arg(long)]
        cache_dir: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Usage {
    General,
    Code,
    Reasoning,
    FastChat,
    Vision,
}

#[derive(Clone, Copy, ValueEnum)]
enum Kv {
    F16,
    #[value(name = "q8_0")]
    Q8_0,
    #[value(name = "q4_0")]
    Q4_0,
}

impl From<Kv> for KvCacheType {
    fn from(k: Kv) -> Self {
        match k {
            Kv::F16 => KvCacheType::F16,
            Kv::Q8_0 => KvCacheType::Q8_0,
            Kv::Q4_0 => KvCacheType::Q4_0,
        }
    }
}

impl From<Usage> for UseCase {
    fn from(u: Usage) -> Self {
        match u {
            Usage::General => UseCase::General,
            Usage::Code => UseCase::Code,
            Usage::Reasoning => UseCase::Reasoning,
            Usage::FastChat => UseCase::FastChat,
            Usage::Vision => UseCase::Vision,
        }
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    tracing::debug!(version = gram_core::VERSION, "gram-cli démarré");

    match cli.command {
        Command::Hw { json, raw } => hw::run(json, raw),
        Command::Sync {
            force,
            offline,
            all_sizes,
            json,
            cache_dir,
        } => sync::run(sync::SyncArgs {
            force,
            offline,
            all_sizes,
            json,
            cache_dir,
        }),
        Command::Estimate {
            model,
            ctx,
            vision,
            kv,
            profile,
            json,
            cache_dir,
        } => estimate::run(estimate::EstimateArgs {
            query: model,
            ctx,
            vision,
            kv_cache: kv.into(),
            profile,
            json,
            cache_dir,
        }),
        Command::Rank { usage, .. } => {
            let _usage = UseCase::from(usage);
            bail!("`rank` n'est pas encore implémenté (phase 4)")
        }
        Command::Snapshot { output, cache_dir } => sync::snapshot(&output, cache_dir),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn definition_cli_valide() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parse_sync_et_snapshot() {
        let cli = Cli::try_parse_from(["gram-cli", "sync", "--offline", "--cache-dir", "D:/cache"])
            .unwrap();
        match cli.command {
            Command::Sync {
                offline,
                force,
                cache_dir,
                ..
            } => {
                assert!(offline && !force);
                assert_eq!(cache_dir, Some(PathBuf::from("D:/cache")));
            }
            _ => panic!("sous-commande inattendue"),
        }
        // Hors ligne et forcé : contradictoire.
        assert!(Cli::try_parse_from(["gram-cli", "sync", "--offline", "--force"]).is_err());
        match Cli::try_parse_from(["gram-cli", "snapshot"])
            .unwrap()
            .command
        {
            Command::Snapshot { output, .. } => {
                assert_eq!(output, PathBuf::from("data/seed_catalog.json"));
            }
            _ => panic!("sous-commande inattendue"),
        }
    }

    #[test]
    fn parse_estimate() {
        let cli = Cli::try_parse_from([
            "gram-cli",
            "estimate",
            "Qwen3-8B",
            "--kv",
            "q8_0",
            "--profile",
            "p2.json",
        ])
        .unwrap();
        match cli.command {
            Command::Estimate {
                model,
                ctx,
                vision,
                kv,
                profile,
                ..
            } => {
                assert_eq!(model, "Qwen3-8B");
                assert_eq!((ctx, vision), (8192, false));
                assert_eq!(KvCacheType::from(kv), KvCacheType::Q8_0);
                assert_eq!(profile, Some(PathBuf::from("p2.json")));
            }
            _ => panic!("sous-commande inattendue"),
        }
        assert!(Cli::try_parse_from(["gram-cli", "estimate"]).is_err());
    }

    #[test]
    fn parse_rank() {
        let cli = Cli::try_parse_from([
            "gram-cli", "rank", "--usage", "code", "--ctx", "16384", "--json",
        ])
        .unwrap();
        match cli.command {
            Command::Rank { usage, ctx, json } => {
                assert_eq!(UseCase::from(usage), UseCase::Code);
                assert_eq!(ctx, Some(16384));
                assert!(json);
            }
            _ => panic!("sous-commande inattendue"),
        }
    }
}
