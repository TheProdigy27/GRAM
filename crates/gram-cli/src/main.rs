//! `gram-cli` : binaire de debug pour valider chaque brique de GRAM sans l'interface.

mod fmt;
mod hw;
mod sync;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use gram_core::UseCase;
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
