//! `gram-cli harness` : ajout à GRAM Harness sans l'interface, pour vérifier sur Ollama réel.
//!
//! Mêmes règles que l'application : estimation sur la machine réelle avec les paramètres et
//! les corrections du matériel enregistrés par GRAM (s'ils existent), même nom de modèle
//! dérivé, même manifeste partagé (`fr.gram.shared/models.json`).

use crate::estimate::find;
use crate::sync::{cache, hub, runtime};
use anyhow::{Context, Result, bail};
use gram_core::estimate::{candidate_variants, estimate};
use gram_core::fmt::{ctx_k, gb, num, thousands, tok_s};
use gram_core::harness::{
    ALIAS_PREFIX, Availability, InstallPhase, PreflightInput, ProgressThrottle, PullProgress,
    added_warnings, needs_availability, preflight,
};
use gram_core::{Measurement, MemoryBudget, Overrides, QuantTable, Settings, UseCase, Workload};
use gram_link::json;
use gram_link::{SharedModel, read_manifest, same_model, update_manifest};
use gram_ollama::{Capability, OllamaClient};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;
use std::time::Instant;

/// Fichiers de l'application GRAM, dans son dossier (`fr.gram.desktop`).
const SETTINGS_FILE: &str = "settings.json";
const CORRECTIONS_FILE: &str = "corrections.json";
const MEASUREMENTS_FILE: &str = "measurements.json";

pub enum HarnessCmd {
    List {
        json: bool,
    },
    Add {
        query: String,
        quant: String,
        ctx: Option<u32>,
        use_case: UseCase,
        dry_run: bool,
        cache_dir: Option<PathBuf>,
    },
    Remove {
        name: String,
        base: bool,
    },
}

pub fn run(cmd: HarnessCmd) -> Result<()> {
    let shared = gram_link::shared_dir()
        .context("dossier des données locales introuvable (variable LOCALAPPDATA ou HOME)")?;
    match cmd {
        HarnessCmd::List { json } => list(&shared, json),
        HarnessCmd::Add {
            query,
            quant,
            ctx,
            use_case,
            dry_run,
            cache_dir,
        } => add(&shared, &query, &quant, ctx, use_case, dry_run, cache_dir),
        HarnessCmd::Remove { name, base } => remove(&shared, &name, base),
    }
}

fn client() -> Result<OllamaClient> {
    OllamaClient::from_env()
        .context("adresse de l'API d'Ollama inutilisable (variable OLLAMA_HOST)")
}

fn list(shared: &std::path::Path, as_json: bool) -> Result<()> {
    let manifest = read_manifest(shared);
    if as_json {
        println!("{}", serde_json::to_string_pretty(&manifest)?);
        return Ok(());
    }
    // Ollama arrêté : la liste du manifeste, sans savoir ce qui est encore installé.
    let installed = runtime()?.block_on(client()?.tags()).ok();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "GRAM Harness ({}) : {} modèle(s) ajouté(s)",
        shared.display(),
        manifest.models.len()
    );
    for m in &manifest.models {
        let state = match &installed {
            Some(list) if list.iter().any(|om| same_model(&om.name, &m.ollama_name)) => "",
            Some(_) => "  (absent d'Ollama)",
            None => "  (Ollama ne répond pas)",
        };
        let measured = m.measured_tok_s.map_or(String::new(), |t| {
            format!(", mesuré {} tok/s", num(f64::from(t), 1))
        });
        let _ = writeln!(
            out,
            "  {:<40} {} {} · contexte {} · {} tok/s estimés{measured}{state}",
            m.ollama_name,
            m.display_name,
            m.quant,
            ctx_k(m.ctx),
            tok_s(m.estimated_tok_s),
        );
        let _ = writeln!(
            out,
            "  {:<40} base {}{}",
            "",
            m.base_name,
            if m.base_installed_by_gram {
                " (téléchargée par GRAM)"
            } else {
                ""
            }
        );
    }
    print!("{out}");
    Ok(())
}

fn add(
    shared: &std::path::Path,
    query: &str,
    quant: &str,
    ctx: Option<u32>,
    use_case: UseCase,
    dry_run: bool,
    cache_dir: Option<PathBuf>,
) -> Result<()> {
    let client = client()?;
    let rt = runtime()?;
    let cache = cache(cache_dir)?;
    let (catalog, _) = gram_hub::load_offline(&cache)
        .context("aucun catalogue disponible : lancer `gram-cli sync`")?;
    let (model, _) = find(&catalog.models, query)?;

    // Les réglages de l'application, pour la même estimation que sa fiche.
    let app_dir = gram_hub::cache::app_dir();
    let load = |file: &str| app_dir.as_ref().map(|d| d.join(file));
    let settings = load(SETTINGS_FILE)
        .map(|p| json::load::<Settings>(&p, "paramètres"))
        .unwrap_or_default()
        .sanitized();
    let corrections = load(CORRECTIONS_FILE)
        .map(|p| json::load::<Overrides>(&p, "corrections"))
        .unwrap_or_default()
        .corrections();
    let measurements: Vec<Measurement> = load(MEASUREMENTS_FILE)
        .map(|p| json::load(&p, "mesures"))
        .unwrap_or_default();

    let hw = gram_hw::build_profile(&gram_hw::collect(), &corrections);
    let params = &settings.estimate;
    let variant = candidate_variants(model, params, QuantTable::embedded())
        .into_iter()
        .find(|v| v.label.eq_ignore_ascii_case(quant))
        .with_context(|| format!("quantification {quant} inconnue pour {}", model.repo_id))?;
    let work = Workload {
        ctx: settings.ctx_for(use_case, ctx),
        vision: use_case == UseCase::Vision,
    };
    let est = estimate(
        model,
        &variant,
        &MemoryBudget::from_profile(&hw, params),
        work,
        params,
    );
    let installed = rt.block_on(client.tags())?;
    let models_dir = gram_ollama::models_dir().map(|d| d.to_string_lossy().into_owned());
    // Comme l'application : Hugging Face sert-il cette quantification à Ollama ?
    let availability = if needs_availability(model, &variant, &installed) {
        rt.block_on(gram_hub::ollama_availability(&hub()?, &variant))
            .unwrap_or_else(|e| Availability::Unknown(e.to_string()))
    } else {
        Availability::Unchecked
    };
    let plan = preflight(&PreflightInput {
        model,
        variant: &variant,
        estimate: &est,
        kv_cache: params.kv_cache,
        slow_tok_s: settings.use_cases.get(use_case).slow_tok_s,
        installed: &installed,
        disks: &hw.disks,
        models_dir: models_dir.as_deref(),
        availability: &availability,
    });

    eprintln!(
        "{} {} → {} · contexte {} · {} tok/s estimés",
        plan.display_name,
        plan.label,
        plan.ollama_name,
        thousands(u64::from(plan.ctx)),
        tok_s(plan.estimated_tok_s)
    );
    if plan.base_installed {
        eprintln!("Base déjà dans Ollama : {}", plan.base_name);
    } else {
        let disk = plan.disk.as_ref().map_or(String::new(), |d| {
            format!(" · {} libres sur {}", gb(d.available), d.mount)
        });
        eprintln!(
            "Téléchargement de {} : {}{disk}",
            plan.base_name,
            gb(plan.download)
        );
    }
    for w in &plan.warnings {
        eprintln!("! {w}");
    }
    if let Some(reason) = &plan.blocked {
        bail!("{reason}");
    }
    if dry_run {
        eprintln!("Vérification seule (--dry-run) : rien n'a été ajouté.");
        return Ok(());
    }

    let clock = Instant::now();
    let mut throttle = ProgressThrottle::new();
    let mut progress = Progress::default();
    let cancel = async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };
    let outcome = rt.block_on(gram_ollama::install(
        &client,
        &plan.base_name,
        &plan.ollama_name,
        plan.ctx,
        |p| {
            if throttle.ready(p.phase, clock.elapsed()) {
                progress.show(&p);
            }
        },
        cancel,
    ));
    progress.end_line();
    let done = match outcome {
        Ok(Some(done)) => done,
        Ok(None) => {
            eprintln!("Ajout annulé : Ollama garde ce qu'il a reçu, le prochain ajout reprendra.");
            return Ok(());
        }
        Err(e) => {
            let gated = model.repo_gated(&variant.repo_id);
            match e.pull_failure() {
                Some(f) => bail!("{}", f.message(&variant.repo_id, &variant.label, gated)),
                None => return Err(e.into()),
            }
        }
    };

    let measured = measurements
        .iter()
        .find(|m| same_model(&m.ollama_model, &done.base))
        .map(|m| m.tok_s);
    let entry = SharedModel::new(
        &plan,
        use_case,
        done.base.clone(),
        done.pulled,
        measured,
        chrono::Utc::now(),
    );
    let old_base = update_manifest(shared, |m| m.upsert(entry))
        .context("manifeste de GRAM Harness non mis à jour")?;
    if let Some(base) = old_base {
        match rt.block_on(client.delete(&base)) {
            Ok(_) => eprintln!("Ancienne base supprimée d'Ollama : {base}"),
            Err(e) => eprintln!("! Ancienne base {base} non supprimée d'Ollama : {e}"),
        }
    }
    for w in added_warnings(&plan, done.show.has(&Capability::Vision), done.show.num_ctx) {
        eprintln!("! {w}");
    }
    println!(
        "Ajouté à GRAM Harness : {} (base {}{}). Pour discuter : ollama run {}",
        plan.ollama_name,
        done.base,
        if done.pulled { ", téléchargée" } else { "" },
        plan.ollama_name
    );
    Ok(())
}

/// Progression sur la sortie d'erreur : une ligne par étape, et le téléchargement réécrit sur
/// place (le journal d'Ollama s'affiche donc sur sa propre ligne).
#[derive(Default)]
struct Progress {
    phase: Option<InstallPhase>,
    /// Une ligne réécrite sur place est en cours, sans fin de ligne.
    open_line: bool,
}

impl Progress {
    fn show(&mut self, p: &PullProgress) {
        let new_phase = self.phase.replace(p.phase) != Some(p.phase);
        let mut err = std::io::stderr();
        if p.phase == InstallPhase::Downloading {
            let rate = p.bytes_per_s.map_or(String::new(), |b| {
                format!(" · {} Mo/s", num(b as f64 / f64::from(1u32 << 20), 0))
            });
            let line = format!(
                "Téléchargement · {} / {}{rate}",
                gram_core::fmt::gb_value(p.completed),
                gb(p.total)
            );
            let _ = write!(err, "\r{line:<60}");
            let _ = err.flush();
            self.open_line = true;
            return;
        }
        if !new_phase {
            return;
        }
        self.end_line();
        let _ = writeln!(
            err,
            "{}",
            match p.phase {
                InstallPhase::Preparing => "Préparation…",
                InstallPhase::Verifying => "Vérification…",
                _ => "Réglage du contexte…",
            }
        );
    }

    fn end_line(&mut self) {
        if std::mem::take(&mut self.open_line) {
            eprintln!();
        }
    }
}

fn remove(shared: &std::path::Path, name: &str, with_base: bool) -> Result<()> {
    let client = client()?;
    let rt = runtime()?;
    let manifest = read_manifest(shared);
    let entry = manifest.get(name);
    if entry.is_none() && !name.to_ascii_lowercase().starts_with(ALIAS_PREFIX) {
        bail!("{name} n'a pas été ajouté par GRAM : le supprimer depuis Ollama");
    }
    let base = entry
        .filter(|e| with_base && manifest.base_removable(e))
        .map(|e| e.base_name.clone());
    if with_base && base.is_none() {
        eprintln!(
            "Base gardée : GRAM ne l'a pas téléchargée, ou un autre modèle ajouté l'utilise."
        );
    }
    let removed = rt.block_on(client.delete(name))?;
    update_manifest(shared, |m| m.remove(name))
        .context("manifeste de GRAM Harness non mis à jour")?;
    println!(
        "{name} {}",
        if removed {
            "supprimé d'Ollama"
        } else {
            "déjà absent d'Ollama"
        }
    );
    if let Some(base) = base {
        rt.block_on(client.delete(&base))
            .with_context(|| format!("{base} non supprimé d'Ollama"))?;
        println!("{base} supprimé d'Ollama");
    }
    Ok(())
}
