//! `gram-cli` : binaire de debug pour valider chaque brique de GRAM sans l'interface.

mod estimate;
mod harness;
mod hw;
mod ollama;
mod rank;
mod sync;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use gram_core::{KvCacheType, Runtime, UseCase};
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
        /// Dossier du cache (défaut : `fr.gram.desktop/cache` dans les données locales :
        /// %LOCALAPPDATA% sous Windows, ~/Library/Application Support sous macOS).
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
        /// Dossier du cache (défaut : `fr.gram.desktop/cache` dans les données locales :
        /// %LOCALAPPDATA% sous Windows, ~/Library/Application Support sous macOS).
        #[arg(long)]
        cache_dir: Option<PathBuf>,
    },
    /// Classe les modèles du catalogue pour cette machine (sans réseau).
    Rank {
        #[arg(long, value_enum, default_value_t = Usage::General)]
        usage: Usage,
        /// Contexte souhaité en tokens (défaut : celui du cas d'usage).
        #[arg(long)]
        ctx: Option<u32>,
        /// Runtime des avertissements (défaut : Ollama).
        #[arg(long, value_enum)]
        runtime: Option<Rt>,
        /// Taille maximale d'une quantification, en Go.
        #[arg(long)]
        max_size_gb: Option<f64>,
        /// Recherche par nom, dépôt ou modèle de base (ex. « qwen3 8b ») : tous les mots
        /// doivent figurer ; le rang affiché reste celui du classement complet.
        #[arg(long)]
        search: Option<String>,
        /// Liste aussi les modèles trop gros.
        #[arg(long)]
        show_too_big: bool,
        /// Lignes par liste (0 : toutes).
        #[arg(long, default_value_t = 25)]
        limit: u32,
        /// Profil matériel JSON (`HardwareProfile`) à la place de cette machine.
        #[arg(long)]
        profile: Option<PathBuf>,
        /// Sortie JSON (`RankResponse`).
        #[arg(long)]
        json: bool,
        /// Dossier du cache (défaut : `fr.gram.desktop/cache` dans les données locales :
        /// %LOCALAPPDATA% sous Windows, ~/Library/Application Support sous macOS).
        #[arg(long)]
        cache_dir: Option<PathBuf>,
    },
    /// Modèles installés dans Ollama, rapprochés du catalogue ; avec `--measure`, mesure réelle
    /// d'une quantification installée face à l'estimation (Ollama doit tourner).
    Ollama {
        /// Modèle du catalogue à mesurer (dépôt, nom ou modèle de base).
        #[arg(long)]
        measure: Option<String>,
        /// Quantification à mesurer (défaut : la première installée pour ce modèle).
        #[arg(long, requires = "measure")]
        quant: Option<String>,
        /// Contexte de la mesure, en tokens.
        #[arg(long, default_value_t = 8192, requires = "measure")]
        ctx: u32,
        /// Sortie JSON.
        #[arg(long)]
        json: bool,
        /// Dossier du cache (défaut : `fr.gram.desktop/cache` dans les données locales :
        /// %LOCALAPPDATA% sous Windows, ~/Library/Application Support sous macOS).
        #[arg(long)]
        cache_dir: Option<PathBuf>,
    },
    /// Ajout à GRAM Harness : modèles ajoutés, ajout d'une quantification à Ollama (modèle
    /// dérivé `gram/…` réglé au contexte calculé par GRAM), retrait.
    Harness {
        #[command(subcommand)]
        action: HarnessAction,
    },
    /// Génère `data/seed_catalog.json` (catalogue embarqué pour le hors ligne).
    Snapshot {
        #[arg(long, default_value = "data/seed_catalog.json")]
        output: PathBuf,
        /// Dossier du cache (défaut : `fr.gram.desktop/cache` dans les données locales :
        /// %LOCALAPPDATA% sous Windows, ~/Library/Application Support sous macOS).
        #[arg(long)]
        cache_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum HarnessAction {
    /// Modèles ajoutés (manifeste partagé `fr.gram.shared/models.json`), et s'ils sont encore
    /// dans Ollama.
    List {
        /// Sortie JSON (`SharedManifest`).
        #[arg(long)]
        json: bool,
    },
    /// Ajoute une quantification d'un modèle du catalogue : téléchargement dans Ollama si
    /// besoin (Ctrl C annule), puis modèle dérivé au contexte estimé avec les paramètres et
    /// les corrections enregistrés par GRAM.
    Add {
        /// Dépôt, nom ou modèle de base (recherche sans tenir compte de la casse).
        model: String,
        /// Quantification (ex. Q4_K_M).
        #[arg(long)]
        quant: String,
        /// Contexte souhaité en tokens (défaut : celui du cas d'usage).
        #[arg(long)]
        ctx: Option<u32>,
        #[arg(long, value_enum, default_value_t = Usage::General)]
        usage: Usage,
        /// Affiche les vérifications (téléchargement, place, contexte, précautions) sans rien
        /// ajouter.
        #[arg(long)]
        dry_run: bool,
        /// Dossier du cache (défaut : `fr.gram.desktop/cache` dans les données locales).
        #[arg(long)]
        cache_dir: Option<PathBuf>,
    },
    /// Retire un modèle ajouté : supprime le modèle dérivé d'Ollama et son entrée du manifeste.
    Remove {
        /// Nom du modèle dérivé (ex. gram/qwen3-8b:q4_k_m).
        name: String,
        /// Supprime aussi la base, si GRAM l'a téléchargée et qu'aucun autre modèle ajouté ne
        /// l'utilise.
        #[arg(long)]
        base: bool,
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
enum Rt {
    Ollama,
    LmStudio,
    LlamaCpp,
}

impl From<Rt> for Runtime {
    fn from(r: Rt) -> Self {
        match r {
            Rt::Ollama => Runtime::Ollama,
            Rt::LmStudio => Runtime::LmStudio,
            Rt::LlamaCpp => Runtime::LlamaCpp,
        }
    }
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
        Command::Rank {
            usage,
            ctx,
            runtime,
            max_size_gb,
            search,
            show_too_big,
            limit,
            profile,
            json,
            cache_dir,
        } => rank::run(rank::RankArgs {
            use_case: usage.into(),
            ctx,
            runtime: runtime.map(Into::into),
            max_size_gb,
            search,
            show_too_big,
            limit,
            profile,
            json,
            cache_dir,
        }),
        Command::Ollama {
            measure,
            quant,
            ctx,
            json,
            cache_dir,
        } => ollama::run(ollama::OllamaArgs {
            measure,
            quant,
            ctx,
            json,
            cache_dir,
        }),
        Command::Harness { action } => harness::run(match action {
            HarnessAction::List { json } => harness::HarnessCmd::List { json },
            HarnessAction::Add {
                model,
                quant,
                ctx,
                usage,
                dry_run,
                cache_dir,
            } => harness::HarnessCmd::Add {
                query: model,
                quant,
                ctx,
                use_case: usage.into(),
                dry_run,
                cache_dir,
            },
            HarnessAction::Remove { name, base } => harness::HarnessCmd::Remove { name, base },
        }),
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
    fn parse_ollama() {
        let cli = Cli::try_parse_from([
            "gram-cli",
            "ollama",
            "--measure",
            "Qwen3-8B",
            "--quant",
            "Q4_K_M",
            "--ctx",
            "4096",
        ])
        .unwrap();
        match cli.command {
            Command::Ollama {
                measure,
                quant,
                ctx,
                json,
                ..
            } => {
                assert_eq!(measure.as_deref(), Some("Qwen3-8B"));
                assert_eq!(quant.as_deref(), Some("Q4_K_M"));
                assert_eq!((ctx, json), (4096, false));
            }
            _ => panic!("sous-commande inattendue"),
        }
        match Cli::try_parse_from(["gram-cli", "ollama"]).unwrap().command {
            Command::Ollama { measure, ctx, .. } => assert_eq!((measure, ctx), (None, 8192)),
            _ => panic!("sous-commande inattendue"),
        }
        // La quantification et le contexte n'ont de sens qu'avec `--measure`.
        assert!(Cli::try_parse_from(["gram-cli", "ollama", "--quant", "Q4_K_M"]).is_err());
        assert!(Cli::try_parse_from(["gram-cli", "ollama", "--ctx", "4096"]).is_err());
    }

    #[test]
    fn parse_harness() {
        let cli = Cli::try_parse_from([
            "gram-cli",
            "harness",
            "add",
            "Qwen3-1.7B",
            "--quant",
            "Q4_K_M",
            "--ctx",
            "16384",
        ])
        .unwrap();
        match cli.command {
            Command::Harness {
                action:
                    HarnessAction::Add {
                        model,
                        quant,
                        ctx,
                        usage,
                        dry_run,
                        ..
                    },
            } => {
                assert_eq!((model.as_str(), quant.as_str()), ("Qwen3-1.7B", "Q4_K_M"));
                assert!(!dry_run);
                assert_eq!(ctx, Some(16384));
                assert_eq!(UseCase::from(usage), UseCase::General);
            }
            _ => panic!("sous-commande inattendue"),
        }
        // La quantification est obligatoire.
        assert!(Cli::try_parse_from(["gram-cli", "harness", "add", "Qwen3-1.7B"]).is_err());
        match Cli::try_parse_from(["gram-cli", "harness", "remove", "gram/x:q4_k_m", "--base"])
            .unwrap()
            .command
        {
            Command::Harness {
                action: HarnessAction::Remove { name, base },
            } => assert_eq!((name.as_str(), base), ("gram/x:q4_k_m", true)),
            _ => panic!("sous-commande inattendue"),
        }
        assert!(matches!(
            Cli::try_parse_from(["gram-cli", "harness", "list", "--json"])
                .unwrap()
                .command,
            Command::Harness {
                action: HarnessAction::List { json: true }
            }
        ));
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
            Command::Rank {
                usage,
                ctx,
                json,
                limit,
                runtime,
                ..
            } => {
                assert_eq!(UseCase::from(usage), UseCase::Code);
                assert_eq!(ctx, Some(16384));
                assert!(json);
                assert_eq!(limit, 25);
                assert!(runtime.is_none());
            }
            _ => panic!("sous-commande inattendue"),
        }
        let cli = Cli::try_parse_from([
            "gram-cli",
            "rank",
            "--runtime",
            "llama-cpp",
            "--max-size-gb",
            "12.5",
            "--show-too-big",
            "--limit",
            "0",
            "--search",
            "qwen3 8b",
        ])
        .unwrap();
        match cli.command {
            Command::Rank {
                runtime,
                max_size_gb,
                search,
                show_too_big,
                limit,
                ..
            } => {
                assert_eq!(runtime.map(Runtime::from), Some(Runtime::LlamaCpp));
                assert_eq!(max_size_gb, Some(12.5));
                assert_eq!(search.as_deref(), Some("qwen3 8b"));
                assert!(show_too_big);
                assert_eq!(limit, 0);
            }
            _ => panic!("sous-commande inattendue"),
        }
    }
}
