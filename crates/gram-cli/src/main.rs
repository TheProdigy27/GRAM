//! `gram-cli` : binaire de debug pour valider chaque brique de GRAM sans l'interface.

use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use gram_core::UseCase;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "gram-cli", version, about = "Outils de debug de GRAM")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Affiche le matériel détecté.
    Hw {
        /// Sortie JSON (HardwareProfile).
        #[arg(long)]
        json: bool,
    },
    /// Synchronise le catalogue Hugging Face.
    Sync {
        /// Ignore le cache.
        #[arg(long)]
        force: bool,
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
    Snapshot,
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
        Command::Hw { .. } => bail!("`hw` n'est pas encore implémenté (phase 1)"),
        Command::Sync { .. } => bail!("`sync` n'est pas encore implémenté (phase 2)"),
        Command::Rank { usage, .. } => {
            let _usage = UseCase::from(usage);
            bail!("`rank` n'est pas encore implémenté (phase 4)")
        }
        Command::Snapshot => bail!("`snapshot` n'est pas encore implémenté (phase 2)"),
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
