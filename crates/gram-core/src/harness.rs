//! Ajout d'un modèle à GRAM Harness, partie pure : nom du modèle dérivé, vérifications avant
//! l'ajout, progression du téléchargement dans Ollama et place sur le disque. Les requêtes à
//! Ollama sont dans `gram-ollama` (`install`), le manifeste partagé dans `gram-link`.

use crate::estimate::{Estimate, KvCacheType, RunMode};
use crate::fmt::{ctx_k, gb, thousands, tok_s};
use crate::hardware::DiskInfo;
use crate::launch::ollama_name;
use crate::model::{ModelEntry, QuantVariant, UseCase};
use crate::runtime::{OllamaModel, installed_variant};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Duration;
use ts_rs::TS;

// ── Nom du modèle dérivé ────────────────────────────────────────────────────────────────────

/// Espace des modèles dérivés de GRAM dans Ollama : `gram/qwen3-8b:q4_k_m`.
pub const ALIAS_PREFIX: &str = "gram/";
/// Longueur maximale du nom (hors préfixe et étiquette).
const ALIAS_MAX_CHARS: usize = 60;

/// Nom du modèle dérivé d'une quantification : `gram/` + nom affiché du modèle en minuscules,
/// sans accents, où toute suite d'autres caractères que `a-z`, `0-9` et `.` devient `-` (sans
/// `-` ni `.` aux deux bouts, 60 caractères au plus ; à défaut, le nom du dépôt), puis `:` et
/// l'étiquette de la quantification en minuscules.
///
/// Un nom `gram/…` n'a pas de dépôt Hugging Face pour [`OllamaModel`] : il ne compte pas comme
/// une quantification installée (sa base, si).
pub fn alias_name(model: &ModelEntry, variant: &QuantVariant) -> String {
    let mut name = slug(&model.display_name);
    if name.is_empty() {
        let repo = model.repo_id.rsplit('/').next().unwrap_or_default();
        name = slug(repo);
    }
    if name.is_empty() {
        name = "modele".into();
    }
    let tag: String = variant
        .label
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("{ALIAS_PREFIX}{name}:{tag}")
}

fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in text.to_lowercase().chars() {
        let folded = fold_accent(c);
        let chars: Vec<char> = match folded {
            Some(s) => s.chars().collect(),
            None => vec![c],
        };
        for c in chars {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' {
                if gap && !out.is_empty() {
                    out.push('-');
                }
                gap = false;
                out.push(c);
            } else {
                gap = true;
            }
        }
    }
    let capped: String = out.chars().take(ALIAS_MAX_CHARS).collect();
    capped.trim_matches(['-', '.']).to_string()
}

/// Lettre latine accentuée (déjà en minuscule) → lettres sans accent ; diacritique combinant →
/// rien.
fn fold_accent(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{300}'..='\u{36f}' => "",
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ą' => "a",
        'ç' | 'ć' | 'č' => "c",
        'ď' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' => "e",
        'ì' | 'í' | 'î' | 'ï' | 'ī' => "i",
        'ł' => "l",
        'ñ' | 'ń' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => "o",
        'ř' => "r",
        'ś' | 'š' => "s",
        'ť' => "t",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' => "u",
        'ý' | 'ÿ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        'æ' => "ae",
        'œ' => "oe",
        'ß' => "ss",
        _ => return None,
    })
}

// ── Vérifications avant l'ajout ─────────────────────────────────────────────────────────────

/// Commandes `harness_preflight` et `harness_add` : une quantification d'un modèle du
/// catalogue, avec les filtres de la fiche détail (les champs de `MeasureRequest`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, optional_fields)]
pub struct HarnessAddRequest {
    /// Dépôt canonique du modèle.
    pub repo_id: String,
    pub label: String,
    pub use_case: UseCase,
    /// Contexte souhaité ; absent : celui du cas d'usage.
    pub ctx: Option<u32>,
}

/// Ce que fera l'ajout d'une quantification à GRAM Harness, vérifié avant de le lancer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Preflight {
    /// Dépôt canonique du modèle (celui de la fiche).
    pub repo_id: String,
    pub display_name: String,
    pub label: String,
    /// Modèle dérivé, qui porte le contexte : `gram/qwen3-8b:q4_k_m`.
    pub ollama_name: String,
    /// Base du modèle dérivé : son nom exact dans Ollama si elle y est déjà (éventuellement
    /// depuis un autre dépôt du modèle), sinon le nom à télécharger (`hf.co/{dépôt}:{quant}`).
    pub base_name: String,
    /// La base est déjà dans Ollama : rien à télécharger.
    pub base_installed: bool,
    /// Taille de la quantification, en octets.
    pub size: u64,
    /// Octets à télécharger : la quantification et son projecteur de vision ; 0 si la base est
    /// déjà là.
    pub download: u64,
    /// Place sur le disque des modèles d'Ollama ; absente si ce disque est inconnu.
    pub disk: Option<DiskSpace>,
    /// Contexte réglé dans le modèle dérivé : celui retenu par l'estimation.
    pub ctx: u32,
    /// Cache KV avec lequel le contexte a été calculé (réglage global d'Ollama).
    pub kv_cache: KvCacheType,
    /// Vitesse estimée sur la machine réelle.
    pub estimated_tok_s: f32,
    /// Le catalogue annonce la vision (projecteur `mmproj` ou étiquette du dépôt).
    pub vision: bool,
    /// Précautions, en français.
    pub warnings: Vec<String>,
    /// Pourquoi l'ajout est refusé (trop gros, place insuffisante) ; absent s'il est possible.
    pub blocked: Option<String>,
}

/// Ce dont [`preflight`] a besoin.
#[derive(Debug, Clone, Copy)]
pub struct PreflightInput<'a> {
    pub model: &'a ModelEntry,
    pub variant: &'a QuantVariant,
    /// Estimation sur la machine réelle (simulation ignorée), au contexte souhaité.
    pub estimate: &'a Estimate,
    pub kv_cache: KvCacheType,
    /// Seuil de lenteur du cas d'usage, en tok/s.
    pub slow_tok_s: f32,
    /// Modèles installés dans Ollama.
    pub installed: &'a [OllamaModel],
    pub disks: &'a [DiskInfo],
    /// Dossier des modèles d'Ollama ; inconnu : la place n'est pas vérifiée.
    pub models_dir: Option<&'a str>,
    /// Réponse de Hugging Face pour la base à télécharger.
    pub availability: &'a Availability,
}

/// Ce que Hugging Face répond, avant l'ajout, pour la quantification à télécharger : c'est la
/// première requête d'Ollama (`GET /v2/{dépôt}/manifests/{quant}`), qui ne télécharge rien.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Availability {
    /// Pas demandé ([`needs_availability`]), ou hors ligne.
    #[default]
    Unchecked,
    /// Servie à Ollama : taille du fichier du modèle, et du projecteur de vision qui
    /// l'accompagne.
    Served { model: u64, projector: Option<u64> },
    /// Refusée : échec reconnu, sinon le message de Hugging Face.
    Refused {
        failure: Option<PullFailure>,
        message: String,
    },
    /// Pas de réponse exploitable (réseau, quota, délai dépassé) : pourquoi.
    Unknown(String),
}

/// La quantification est à demander à Hugging Face avant l'ajout : sa base n'est pas dans
/// Ollama, ses fichiers sont connus (une taille estimée est refusée sans rien demander) et son
/// dépôt n'est pas protégé (Ollama s'y authentifie avec sa propre clé, que GRAM n'a pas : la
/// réponse faite à GRAM ne vaudrait pas pour lui).
pub fn needs_availability(
    model: &ModelEntry,
    variant: &QuantVariant,
    installed: &[OllamaModel],
) -> bool {
    variant.exact
        && !model.repo_gated(&variant.repo_id)
        && installed_variant(installed, model, &variant.repo_id, &variant.label).is_none()
}

/// Écart de taille, en pour cent, au-delà duquel le fichier servi par Hugging Face n'est pas
/// celui que GRAM a évalué.
const SERVED_SIZE_TOLERANCE_PCT: u64 = 2;

/// Vérifie l'ajout d'une quantification : téléchargement nécessaire, place sur le disque,
/// contexte réglé, précautions et refus.
///
/// Refusé avant tout le reste, quand la base est à télécharger : une quantification dont la
/// taille n'est qu'estimée (aucun fichier connu), et une quantification que Hugging Face ne
/// sert pas à Ollama ([`Availability::Refused`]).
pub fn preflight(i: &PreflightInput) -> Preflight {
    let (model, variant, est) = (i.model, i.variant, i.estimate);
    let gated = model.repo_gated(&variant.repo_id);
    let installed = installed_variant(i.installed, model, &variant.repo_id, &variant.label);
    let base_installed = installed.is_some();
    let base_name = installed.map_or_else(|| ollama_name(variant, gated), |om| om.name.clone());
    let served = match *i.availability {
        Availability::Served { model, projector } if !base_installed => Some((model, projector)),
        _ => None,
    };
    let download = if base_installed {
        0
    } else if let Some((bytes, projector)) = served {
        bytes.saturating_add(projector.unwrap_or(0))
    } else {
        variant
            .size_bytes
            .saturating_add(model.mmproj_bytes.unwrap_or(0))
    };
    let unavailable = if base_installed {
        None
    } else if !variant.exact {
        Some(format!(
            "Taille de {} estimée : aucun fichier de cette quantification n'est connu, et Hugging \
             Face ne la propose peut-être pas. Seule une quantification dont les fichiers ont été \
             lus peut être ajoutée.",
            variant.label
        ))
    } else if let Availability::Refused { failure, message } = i.availability {
        Some(failure.map_or_else(
            || {
                format!(
                    "Hugging Face refuse de servir {} de {} à Ollama : {message}",
                    variant.label, variant.repo_id
                )
            },
            |f| f.message(&variant.repo_id, &variant.label, gated),
        ))
    } else {
        None
    };
    let disk = i
        .models_dir
        .and_then(|dir| disk_check(i.disks, dir, download));

    let mut warnings = Vec::new();
    if !base_installed && gated {
        warnings.push(
            "Dépôt protégé : accepter ses conditions sur Hugging Face, puis ajouter la clé \
             publique d'Ollama (fichier .ollama/id_ed25519.pub de votre dossier personnel) aux \
             clés SSH de votre compte Hugging Face."
                .to_string(),
        );
    }
    // Servie : Hugging Face a trouvé un fichier unique pour cette étiquette.
    if !base_installed && variant.files.len() > 1 && served.is_none() {
        warnings.push(format!(
            "Modèle découpé en {} fichiers : Hugging Face ne sert pas ces modèles à Ollama, \
             l'ajout risque d'échouer. Préférer une quantification en un seul fichier, ou \
             llama.cpp.",
            variant.files.len()
        ));
    }
    if let Some((bytes, _)) = served
        && bytes.abs_diff(variant.size_bytes) * 100 > variant.size_bytes * SERVED_SIZE_TOLERANCE_PCT
    {
        warnings.push(format!(
            "Hugging Face sert pour {} un fichier de {}, et non celui de {} que GRAM a évalué : \
             l'estimation ne vaut pas pour lui.",
            variant.label,
            gb(bytes),
            gb(variant.size_bytes)
        ));
    }
    if let Availability::Unknown(why) = i.availability
        && !base_installed
    {
        warnings.push(format!(
            "Disponibilité de {} non vérifiée auprès de Hugging Face ({why}) : l'ajout peut \
             échouer.",
            variant.label
        ));
    }
    if i.kv_cache != KvCacheType::F16 {
        warnings.push(format!(
            "Contexte de {} calculé avec un cache KV {} : c'est un réglage global d'Ollama, que \
             GRAM ne change pas (commandes dans « Lancer avec »). Sans lui, ce contexte occupe \
             plus de mémoire.",
            ctx_k(est.ctx_used),
            i.kv_cache.label()
        ));
    }
    if est.mode != RunMode::TooBig && est.tok_s < i.slow_tok_s {
        warnings.push(format!(
            "Lent sur cette machine : {} tok/s estimés.",
            tok_s(est.tok_s)
        ));
    }

    if unavailable.is_some() {
        // Rien d'autre ne compte : la base ne peut pas être téléchargée.
        warnings.clear();
    }

    let blocked = if unavailable.is_some() {
        unavailable
    } else if est.mode == RunMode::TooBig {
        Some(format!(
            "{} ne tient pas en mémoire sur cette machine : ajout refusé (le système risquerait \
             de saturer).",
            variant.label
        ))
    } else {
        disk.as_ref().filter(|d| !d.enough).map(|d| {
            format!(
                "Pas assez de place sur {} : {} libres, {} nécessaires ({} à télécharger, plus \
                 10 % de marge).",
                d.mount,
                gb(d.available),
                gb(d.needed),
                gb(download)
            )
        })
    };

    Preflight {
        repo_id: model.repo_id.clone(),
        display_name: model.display_name.clone(),
        label: variant.label.clone(),
        ollama_name: alias_name(model, variant),
        base_name,
        base_installed,
        size: variant.size_bytes,
        download,
        disk,
        ctx: est.ctx_used,
        kv_cache: i.kv_cache,
        estimated_tok_s: est.tok_s,
        vision: model.use_cases.contains(&UseCase::Vision),
        warnings,
        blocked,
    }
}

/// Avertissements après l'ajout, d'après ce qu'Ollama annonce du modèle dérivé : capacité
/// `vision` et contexte réglé.
pub fn added_warnings(p: &Preflight, has_vision: bool, num_ctx: Option<u32>) -> Vec<String> {
    let mut warnings = Vec::new();
    if p.vision && !has_vision {
        warnings.push(format!(
            "Vision attendue mais absente dans Ollama : {} ne fournit sans doute pas de \
             projecteur (mmproj). GRAM Harness ne pourra pas envoyer d'images à ce modèle.",
            p.base_name
        ));
    }
    if num_ctx != Some(p.ctx) {
        let found = num_ctx.map_or("aucun contexte".into(), |n| {
            format!("un contexte de {}", thousands(u64::from(n)))
        });
        warnings.push(format!(
            "Ollama annonce {found} pour {}, au lieu de {} : le modèle dérivé n'a peut-être \
             pas été réglé.",
            p.ollama_name,
            thousands(u64::from(p.ctx))
        ));
    }
    warnings
}

/// Échec d'un téléchargement depuis Hugging Face, reconnu dans l'erreur d'Ollama
/// (`gram-ollama` fait la reconnaissance).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullFailure {
    /// Accès refusé : dépôt protégé, ou inexistant (Hugging Face répond la même chose).
    Unauthorized,
    /// Quantification que Hugging Face ne propose pas à Ollama pour ce dépôt.
    UnknownQuant,
    /// Quantification découpée en plusieurs fichiers, refusée dès le manifeste.
    Sharded,
    /// Fichier qui n'est qu'une partie d'un modèle découpé, sous un nom qui ne le dit pas :
    /// Hugging Face le sert seul, Ollama le télécharge, puis refuse d'en créer un modèle.
    SplitPart,
}

impl PullFailure {
    /// Reconnaît un échec que l'utilisateur peut corriger, dans un message d'erreur d'Ollama
    /// (textes d'Ollama 0.40.16, dans `tests/fixtures/ollama/pull-error-*.ndjson`) ou de
    /// Hugging Face, qu'Ollama reprend (`tests/fixtures/hf/manifests/`) :
    ///
    /// - `realm host "huggingface.co" does not match original host "hf.co"` (dépôt protégé ou
    ///   inexistant depuis `hf.co/`) et `401: {"error":"Invalid username or password."}`
    ///   (depuis `huggingface.co/`, clé d'Ollama non enregistrée) ;
    /// - `The specified tag is not a valid quantization scheme…` et `The specified tag is not
    ///   available in the repository…` ;
    /// - `This repository only contains sharded GGUF files…` et `This tag is a sharded GGUF…` ;
    /// - `invalid split GGUF: split GGUF has 2 remaining shards, expected 5`, renvoyé par
    ///   `/api/create` (HTTP 500) quand le fichier téléchargé n'est que la première partie d'un
    ///   modèle découpé, sous un nom qui ne le dit pas (`create-error-split.json`).
    pub fn recognize(message: &str) -> Option<Self> {
        if message.contains("does not match original host")
            || message.contains("manifest: 401")
            || message.contains("Invalid username or password")
        {
            Some(Self::Unauthorized)
        } else if message.contains("not a valid quantization scheme")
            || message.contains("tag is not available")
        {
            Some(Self::UnknownQuant)
        } else if message.contains("sharded GGUF") {
            Some(Self::Sharded)
        } else if message.contains("invalid split GGUF") {
            Some(Self::SplitPart)
        } else {
            None
        }
    }

    /// Message pour l'utilisateur, avec ce qu'il peut faire.
    pub fn message(self, repo_id: &str, label: &str, gated: bool) -> String {
        match self {
            Self::Unauthorized if gated => format!(
                "Hugging Face a refusé le téléchargement : {repo_id} est un dépôt protégé. \
                 Accepter ses conditions sur huggingface.co, puis ajouter la clé publique \
                 d'Ollama (fichier .ollama/id_ed25519.pub de votre dossier personnel) aux clés \
                 SSH de votre compte Hugging Face, et réessayer."
            ),
            Self::Unauthorized => format!(
                "Hugging Face a refusé le téléchargement de {repo_id} : dépôt introuvable ou \
                 devenu protégé. Synchroniser le catalogue, puis réessayer."
            ),
            Self::UnknownQuant => format!(
                "Hugging Face ne propose pas {label} de {repo_id} à Ollama : choisir une autre \
                 quantification."
            ),
            Self::Sharded => format!(
                "{label} de {repo_id} est découpé en plusieurs fichiers, que Hugging Face ne sert \
                 pas à Ollama : choisir une quantification en un seul fichier, ou lancer le modèle \
                 avec llama.cpp."
            ),
            Self::SplitPart => format!(
                "{label} de {repo_id} n'est qu'une partie d'un modèle découpé en plusieurs \
                 fichiers (son nom ne l'indique pas) : Ollama ne peut pas s'en servir. Choisir \
                 une autre quantification ou un autre dépôt."
            ),
        }
    }
}

// ── Progression ─────────────────────────────────────────────────────────────────────────────

/// Étape de l'ajout, dans l'ordre où elles se suivent.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize, TS,
)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum InstallPhase {
    /// Modèles installés, puis manifeste du dépôt.
    #[default]
    Preparing,
    /// Couches du modèle en cours de téléchargement.
    Downloading,
    /// Ollama vérifie les empreintes et écrit le manifeste : aucune nouvelle pendant ce temps
    /// (environ une minute pour 40 Go sur un SSD NVMe).
    Verifying,
    /// Création du modèle dérivé, qui règle le contexte.
    Creating,
}

/// Avancement de l'ajout à un instant donné.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PullProgress {
    pub phase: InstallPhase,
    /// Octets sur le disque, y compris ceux d'un téléchargement repris.
    pub completed: u64,
    /// Taille des couches annoncées jusqu'ici : elle grandit quand une couche apparaît.
    pub total: u64,
    /// Débit sur les [`RATE_WINDOW`] dernières secondes, pendant le téléchargement et après
    /// au moins une seconde de suivi.
    pub bytes_per_s: Option<u64>,
}

/// Fenêtre de calcul du débit.
pub const RATE_WINDOW: Duration = Duration::from_secs(3);
/// En dessous, le débit n'est pas encore significatif.
const MIN_RATE_SPAN: Duration = Duration::from_secs(1);

/// Suivi d'un téléchargement d'après les lignes de `POST /api/pull`.
///
/// Ollama envoie, couche par couche, la taille (`total`) et les octets reçus (`completed`). Les
/// totaux s'additionnent sur les couches vues. Le débit ne compte que les octets reçus pendant
/// le suivi : une couche déjà présente ou un téléchargement repris arrive d'un coup, sans être
/// pris pour un débit.
///
/// `now` est le temps écoulé depuis une origine quelconque, la même pour tous les appels.
#[derive(Debug, Clone, Default)]
pub struct PullTracker {
    phase: InstallPhase,
    layers: Vec<Layer>,
    /// Octets reçus depuis le début du suivi.
    received: u64,
    /// `(instant, received)` : un échantillon antérieur à la fenêtre, puis ceux de la fenêtre.
    samples: VecDeque<(Duration, u64)>,
}

#[derive(Debug, Clone)]
struct Layer {
    digest: String,
    total: u64,
    completed: u64,
}

impl PullTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn phase(&self) -> InstallPhase {
        self.phase
    }

    /// Une ligne de progression d'une couche. `completed` manque tant qu'Ollama n'a rien reçu
    /// pour elle : la dernière valeur connue reste.
    pub fn layer(&mut self, digest: &str, total: u64, completed: Option<u64>, now: Duration) {
        match self.layers.iter_mut().find(|l| l.digest == digest) {
            Some(l) => {
                let completed = completed.unwrap_or(l.completed);
                self.received += completed.saturating_sub(l.completed);
                l.total = total;
                l.completed = completed;
            }
            None => self.layers.push(Layer {
                digest: digest.to_string(),
                total,
                completed: completed.unwrap_or(0),
            }),
        }
        self.samples.push_back((now, self.received));
        if let Some(start) = now.checked_sub(RATE_WINDOW) {
            while self.samples.len() > 1 && self.samples[1].0 <= start {
                self.samples.pop_front();
            }
        }
        self.set_phase(InstallPhase::Downloading);
    }

    /// Passe à l'étape `phase` ; une étape déjà dépassée est ignorée.
    pub fn set_phase(&mut self, phase: InstallPhase) {
        self.phase = self.phase.max(phase);
    }

    pub fn progress(&self, now: Duration) -> PullProgress {
        let total = self.layers.iter().map(|l| l.total).sum();
        // La dernière ligne d'une couche n'atteint pas toujours `total` : une fois la
        // vérification commencée, tout est arrivé.
        let completed = if self.phase >= InstallPhase::Verifying {
            total
        } else {
            self.layers.iter().map(|l| l.completed.min(l.total)).sum()
        };
        PullProgress {
            phase: self.phase,
            completed,
            total,
            bytes_per_s: if self.phase == InstallPhase::Downloading {
                self.rate(now)
            } else {
                None
            },
        }
    }

    /// Octets reçus depuis le dernier échantillon antérieur à la fenêtre (à défaut, le
    /// premier), divisés par le temps écoulé jusqu'à `now` : le débit retombe à zéro quand
    /// plus rien n'arrive.
    fn rate(&self, now: Duration) -> Option<u64> {
        let &(_, last) = self.samples.back()?;
        let window_start = now.checked_sub(RATE_WINDOW);
        let &(t0, r0) = window_start
            .and_then(|s| self.samples.iter().rev().find(|(t, _)| *t <= s))
            .or(self.samples.front())?;
        let span = now.checked_sub(t0)?;
        (span >= MIN_RATE_SPAN).then(|| ((last - r0) as f64 / span.as_secs_f64()).round() as u64)
    }
}

/// Événement `harness-progress` : avancement de l'ajout en cours.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InstallProgress {
    /// Dépôt canonique du modèle (celui de la fiche).
    pub repo_id: String,
    pub display_name: String,
    pub label: String,
    /// Modèle dérivé en cours d'ajout.
    pub ollama_name: String,
    pub phase: InstallPhase,
    /// Octets sur le disque, y compris ceux d'un téléchargement repris.
    pub completed: u64,
    /// Taille des couches annoncées jusqu'ici.
    pub total: u64,
    pub bytes_per_s: Option<u64>,
}

impl InstallProgress {
    pub fn new(p: &Preflight, progress: PullProgress) -> Self {
        Self {
            repo_id: p.repo_id.clone(),
            display_name: p.display_name.clone(),
            label: p.label.clone(),
            ollama_name: p.ollama_name.clone(),
            phase: progress.phase,
            completed: progress.completed,
            total: progress.total,
            bytes_per_s: progress.bytes_per_s,
        }
    }
}

/// Intervalle minimal entre deux nouvelles de la progression.
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

/// Limite les nouvelles de la progression (Ollama envoie une ligne toutes les 60 ms) : au plus
/// une par [`PROGRESS_INTERVAL`], sauf un changement d'étape, transmis aussitôt.
#[derive(Debug, Clone, Default)]
pub struct ProgressThrottle {
    last: Option<(Duration, InstallPhase)>,
}

impl ProgressThrottle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Faut-il transmettre cette nouvelle ? `now` : même origine que pour [`PullTracker`].
    pub fn ready(&mut self, phase: InstallPhase, now: Duration) -> bool {
        let ready = match self.last {
            None => true,
            Some((at, last_phase)) => {
                phase != last_phase || now.saturating_sub(at) >= PROGRESS_INTERVAL
            }
        };
        if ready {
            self.last = Some((now, phase));
        }
        ready
    }
}

/// Place sur le disque qui recevra un téléchargement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiskSpace {
    /// Point de montage du disque (« C:\ », « / »…).
    pub mount: String,
    /// Espace libre, en octets.
    pub available: u64,
    /// Place demandée : la taille plus 10 %.
    pub needed: u64,
    pub enough: bool,
}

/// Le disque qui contient `dir` est celui dont le point de montage est le plus long préfixe du
/// dossier (montages imbriqués) ; assez de place si l'espace libre couvre `size` plus 10 %.
/// `None` si aucun disque connu ne contient le dossier.
pub fn disk_check(disks: &[DiskInfo], dir: &str, size: u64) -> Option<DiskSpace> {
    let (_, disk) = disks
        .iter()
        .filter_map(|d| mount_prefix_len(&d.mount, dir).map(|len| (len, d)))
        .max_by_key(|&(len, _)| len)?;
    let needed = size.saturating_add(size / 10);
    Some(DiskSpace {
        mount: disk.mount.clone(),
        available: disk.available,
        needed,
        enough: disk.available >= needed,
    })
}

/// Longueur du point de montage s'il contient `dir`, segment par segment (`/mnt/d` ne contient
/// pas `/mnt/data`). Les chemins Windows se comparent sans tenir compte de la casse, avec `\`
/// ou `/`.
fn mount_prefix_len(mount: &str, dir: &str) -> Option<usize> {
    let windows = is_windows_path(mount) || is_windows_path(dir);
    let normalize = |s: &str| {
        if windows {
            s.replace('\\', "/").to_ascii_lowercase()
        } else {
            s.to_string()
        }
    };
    let (mount, dir) = (normalize(mount), normalize(dir));
    let mount = mount.trim_end_matches('/');
    let rest = dir.strip_prefix(mount)?;
    (rest.is_empty() || rest.starts_with('/')).then_some(mount.len())
}

/// `C:\…`, `d:/…`
fn is_windows_path(path: &str) -> bool {
    matches!(path.as_bytes(), [letter, b':', ..] if letter.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::estimate::{FitLevel, MemoryBreakdown};
    use crate::model::{Pipeline, RepoRef};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    const GIB: u64 = 1 << 30;

    fn variant(repo: &str, label: &str, files: usize) -> QuantVariant {
        QuantVariant {
            label: label.into(),
            repo_id: repo.into(),
            files: (1..=files).map(|i| format!("{label}-{i}.gguf")).collect(),
            size_bytes: 5 * GIB,
            bpw: 4.9,
            exact: true,
        }
    }

    fn model(display_name: &str) -> ModelEntry {
        ModelEntry {
            repo_id: "Qwen/Qwen3-8B-GGUF".into(),
            sha: String::new(),
            base_model: Some("Qwen/Qwen3-8B".into()),
            display_name: display_name.into(),
            params_total: 8_190_000_000,
            params_active: None,
            arch: None,
            ctx_train: 40_960,
            pipeline: Pipeline::TextGeneration,
            use_cases: vec![UseCase::General],
            downloads: 0,
            likes: 0,
            created_at: None,
            last_modified: None,
            gated: false,
            variants: vec![variant("Qwen/Qwen3-8B-GGUF", "Q4_K_M", 1)],
            mmproj_bytes: None,
            alternatives: vec![RepoRef {
                repo_id: "unsloth/Qwen3-8B-GGUF".into(),
                sha: String::new(),
                downloads: 0,
                gated: true,
            }],
        }
    }

    fn estimate(mode: RunMode, tok_s: f32) -> Estimate {
        Estimate {
            mode,
            mem_gpu: 0,
            mem_cpu: 0,
            breakdown: MemoryBreakdown {
                weights: 5 * GIB,
                kv: GIB,
                overhead: GIB / 2,
                vision: 0,
            },
            kv_per_token: 144 << 10,
            mem_usage: 0.6,
            ctx_used: 16_384,
            ctx_max: 32_768,
            tok_s,
            fit: FitLevel::Perfect,
            approximate: false,
            warnings: vec![],
        }
    }

    #[test]
    fn nom_du_modele_derive() {
        let q4 = variant("Qwen/Qwen3-8B-GGUF", "Q4_K_M", 1);
        assert_eq!(alias_name(&model("Qwen3 8B"), &q4), "gram/qwen3-8b:q4_k_m");
        let ud = variant("unsloth/Qwen3-8B-GGUF", "UD-Q4_K_XL", 1);
        assert_eq!(
            alias_name(&model("Qwen3-8B"), &ud),
            "gram/qwen3-8b:ud-q4_k_xl"
        );
        // Accents retirés, suites d'autres caractères réduites à un `-`, rien aux deux bouts.
        assert_eq!(
            alias_name(&model("  Mistral Pétit — Édition « Spéciale » 2.5! "), &q4),
            "gram/mistral-petit-edition-speciale-2.5:q4_k_m"
        );
        assert_eq!(
            alias_name(&model("Œuvre_Straße Ünï"), &q4),
            "gram/oeuvre-strasse-uni:q4_k_m"
        );
        assert_eq!(alias_name(&model(".v1."), &q4), "gram/v1:q4_k_m");
        // 60 caractères au plus, sans `-` final après la coupe.
        let long = format!("{} b", "a".repeat(59));
        let name = alias_name(&model(&long), &q4);
        assert_eq!(name, format!("gram/{}:q4_k_m", "a".repeat(59)));
        // Nom affiché inutilisable : le nom du dépôt.
        assert_eq!(
            alias_name(&model("通义千问"), &q4),
            "gram/qwen3-8b-gguf:q4_k_m"
        );
        // Étiquette assainie.
        let odd = variant("Qwen/Qwen3-8B-GGUF", "Q4 K/M", 1);
        assert_eq!(alias_name(&model("Qwen3 8B"), &odd), "gram/qwen3-8b:q4-k-m");
    }

    #[test]
    fn nom_derive_non_compte_comme_installe() {
        // `gram/…` n'a pas de dépôt : seule la base compte pour la fiche et la mesure.
        let alias = OllamaModel::new("gram/qwen3-8b:q4_k_m", 1, Some("Q4_K_M"));
        assert_eq!(alias.repo_id, None);
        let m = model("Qwen3 8B");
        assert!(installed_variant(&[alias], &m, "Qwen/Qwen3-8B-GGUF", "Q4_K_M").is_none());
    }

    fn input<'a>(
        m: &'a ModelEntry,
        v: &'a QuantVariant,
        est: &'a Estimate,
        installed: &'a [OllamaModel],
        disks: &'a [DiskInfo],
    ) -> PreflightInput<'a> {
        PreflightInput {
            model: m,
            variant: v,
            estimate: est,
            kv_cache: KvCacheType::F16,
            slow_tok_s: 5.0,
            installed,
            disks,
            models_dir: Some("C:\\Users\\gram\\.ollama\\models"),
            availability: &Availability::Unchecked,
        }
    }

    #[test]
    fn preflight_a_telecharger() {
        let mut m = model("Qwen3 8B");
        m.mmproj_bytes = Some(GIB / 4);
        m.use_cases.push(UseCase::Vision);
        let v = m.variants[0].clone();
        let est = estimate(RunMode::Gpu, 40.0);
        let disks = [disk("C:\\", 100 * GIB)];
        let p = preflight(&input(&m, &v, &est, &[], &disks));
        assert_eq!(p.ollama_name, "gram/qwen3-8b:q4_k_m");
        assert_eq!(p.base_name, "hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M");
        assert!(!p.base_installed);
        // La quantification et son projecteur.
        assert_eq!((p.size, p.download), (5 * GIB, 5 * GIB + GIB / 4));
        let disk = p.disk.as_ref().unwrap();
        assert_eq!(disk.mount, "C:\\");
        assert!(disk.enough);
        assert_eq!((p.ctx, p.estimated_tok_s), (16_384, 40.0));
        assert!(p.vision);
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        assert_eq!(p.blocked, None);

        // Camel case pour l'interface.
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["ollamaName"], "gram/qwen3-8b:q4_k_m");
        assert_eq!(json["kvCache"], "f16");
        assert_eq!(json["disk"]["available"], 100 * GIB);
    }

    #[test]
    fn preflight_base_deja_installee() {
        let m = model("Qwen3 8B");
        let v = m.variants[0].clone();
        let est = estimate(RunMode::Gpu, 40.0);
        // Installée depuis un autre dépôt du modèle, avec une autre casse : reprise telle quelle.
        let installed = [OllamaModel::new(
            "hf.co/unsloth/qwen3-8b-gguf:q4_k_m",
            5 * GIB,
            None,
        )];
        // Disque plein : sans téléchargement, rien ne bloque.
        let disks = [disk("C:\\", 0)];
        let p = preflight(&input(&m, &v, &est, &installed, &disks));
        assert!(p.base_installed);
        assert_eq!(p.base_name, "hf.co/unsloth/qwen3-8b-gguf:q4_k_m");
        assert_eq!(p.download, 0);
        assert!(p.disk.unwrap().enough);
        assert_eq!(p.blocked, None);
    }

    #[test]
    fn preflight_avertissements() {
        let m = model("Qwen3 8B");
        // Dépôt protégé (alternative d'unsloth), découpé en 2 fichiers.
        let v = variant("unsloth/Qwen3-8B-GGUF", "Q8_0", 2);
        let est = estimate(
            RunMode::Hybrid {
                gpu_layers: 20,
                layers: 36,
            },
            3.2,
        );
        let mut i = input(&m, &v, &est, &[], &[]);
        i.kv_cache = KvCacheType::Q8_0;
        i.models_dir = None;
        let p = preflight(&i);
        assert_eq!(p.base_name, "huggingface.co/unsloth/Qwen3-8B-GGUF:Q8_0");
        assert_eq!(p.disk, None, "dossier inconnu : place non vérifiée");
        assert_eq!(p.blocked, None);
        assert_eq!(p.kv_cache, KvCacheType::Q8_0);
        assert_eq!(p.warnings.len(), 4, "{:?}", p.warnings);
        assert!(p.warnings[0].starts_with("Dépôt protégé"));
        assert!(p.warnings[1].starts_with("Modèle découpé en 2 fichiers"));
        assert!(p.warnings[2].contains("16k") && p.warnings[2].contains("q8_0"));
        assert_eq!(
            p.warnings[3],
            "Lent sur cette machine : ~3,2 tok/s estimés."
        );

        // Base déjà là : ni dépôt protégé ni découpage ne comptent plus.
        let installed = [OllamaModel::new(
            "huggingface.co/unsloth/Qwen3-8B-GGUF:Q8_0",
            1,
            None,
        )];
        let p = preflight(&input(&m, &v, &est, &installed, &[]));
        assert_eq!(p.warnings.len(), 1, "{:?}", p.warnings);
    }

    #[test]
    fn preflight_refus() {
        let m = model("Qwen3 8B");
        let v = m.variants[0].clone();
        let too_big = estimate(RunMode::TooBig, 0.0);
        let p = preflight(&input(&m, &v, &too_big, &[], &[]));
        assert_eq!(
            p.blocked.as_deref(),
            Some(
                "Q4_K_M ne tient pas en mémoire sur cette machine : ajout refusé (le système \
                 risquerait de saturer)."
            )
        );
        assert!(
            p.warnings.is_empty(),
            "pas de « lent » pour un modèle refusé"
        );

        let est = estimate(RunMode::Gpu, 40.0);
        let disks = [disk("C:\\", 5 * GIB)];
        let p = preflight(&input(&m, &v, &est, &[], &disks));
        assert_eq!(
            p.blocked.as_deref(),
            Some(
                "Pas assez de place sur C:\\ : 5,0 Go libres, 5,5 Go nécessaires (5,0 Go à \
                 télécharger, plus 10 % de marge)."
            )
        );
    }

    #[test]
    fn preflight_selon_hugging_face() {
        let mut m = model("Qwen3 8B");
        m.mmproj_bytes = Some(GIB / 4);
        let est = estimate(RunMode::Gpu, 40.0);
        let disks = [disk("C:\\", 100 * GIB)];
        let with = |v: &QuantVariant, a: &Availability| {
            let mut i = input(&m, v, &est, &[], &disks);
            i.availability = a;
            preflight(&i)
        };

        // Servie : tailles de Hugging Face (fichier choisi et projecteur), sans avertissement
        // de découpage (Hugging Face a trouvé un fichier unique pour cette étiquette).
        let split = variant("Qwen/Qwen3-8B-GGUF", "Q4_K_M", 2);
        let served = Availability::Served {
            model: 5 * GIB,
            projector: Some(GIB / 2),
        };
        let p = with(&split, &served);
        assert_eq!(p.download, 5 * GIB + GIB / 2);
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        assert_eq!(p.blocked, None);
        // Pas vérifiée : avertissement de découpage, tailles du catalogue.
        let p = with(&split, &Availability::Unchecked);
        assert_eq!(p.download, 5 * GIB + GIB / 4);
        assert!(p.warnings[0].starts_with("Modèle découpé en 2 fichiers"));

        // Un autre fichier que celui évalué (au-delà de 2 %).
        let single = variant("Qwen/Qwen3-8B-GGUF", "Q4_K_M", 1);
        let other = Availability::Served {
            model: 6 * GIB,
            projector: None,
        };
        let p = with(&single, &other);
        assert_eq!(p.download, 6 * GIB);
        assert_eq!(
            p.warnings,
            vec![
                "Hugging Face sert pour Q4_K_M un fichier de 6,0 Go, et non celui de 5,0 Go que \
                 GRAM a évalué : l'estimation ne vaut pas pour lui."
            ]
        );

        // Refusée : ajout refusé, sans autre avertissement.
        let refused = Availability::Refused {
            failure: Some(PullFailure::UnknownQuant),
            message: "The specified tag is not available in the repository.".into(),
        };
        let p = with(&split, &refused);
        assert_eq!(
            p.blocked.as_deref(),
            Some(
                "Hugging Face ne propose pas Q4_K_M de Qwen/Qwen3-8B-GGUF à Ollama : choisir \
                 une autre quantification."
            )
        );
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        let p = with(
            &single,
            &Availability::Refused {
                failure: None,
                message: "Nope".into(),
            },
        );
        assert_eq!(
            p.blocked.as_deref(),
            Some("Hugging Face refuse de servir Q4_K_M de Qwen/Qwen3-8B-GGUF à Ollama : Nope")
        );
        // Avant le manque de mémoire : la base ne peut même pas être téléchargée.
        let too_big = estimate(RunMode::TooBig, 0.0);
        let mut i = input(&m, &single, &too_big, &[], &disks);
        i.availability = &refused;
        assert!(
            preflight(&i)
                .blocked
                .unwrap()
                .starts_with("Hugging Face ne propose pas")
        );

        // Pas de réponse : avertissement, l'ajout reste possible.
        let p = with(&single, &Availability::Unknown("délai dépassé".into()));
        assert_eq!(p.blocked, None);
        assert_eq!(
            p.warnings,
            vec![
                "Disponibilité de Q4_K_M non vérifiée auprès de Hugging Face (délai dépassé) : \
                 l'ajout peut échouer."
            ]
        );

        // Taille estimée (aucun fichier connu) : refusée, sans rien demander.
        let mut estimated = single.clone();
        estimated.exact = false;
        estimated.files.clear();
        let p = with(&estimated, &Availability::Unchecked);
        assert!(
            p.blocked
                .as_deref()
                .unwrap()
                .starts_with("Taille de Q4_K_M estimée : aucun fichier de cette quantification"),
            "{:?}",
            p.blocked
        );

        // Base déjà dans Ollama : rien à télécharger, la réponse ne compte pas.
        let installed = [OllamaModel::new("hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M", 1, None)];
        let mut i = input(&m, &estimated, &est, &installed, &disks);
        i.availability = &refused;
        let p = preflight(&i);
        assert_eq!((p.blocked, p.download), (None, 0));
    }

    #[test]
    fn disponibilite_a_demander() {
        let m = model("Qwen3 8B");
        let v = m.variants[0].clone();
        assert!(needs_availability(&m, &v, &[]));
        // Déjà dans Ollama.
        let installed = [OllamaModel::new("hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M", 1, None)];
        assert!(!needs_availability(&m, &v, &installed));
        // Dépôt protégé : Ollama s'authentifie avec sa propre clé.
        assert!(!needs_availability(
            &m,
            &variant("unsloth/Qwen3-8B-GGUF", "Q4_K_M", 1),
            &[]
        ));
        // Taille estimée : refusée sans rien demander.
        let mut estimated = v.clone();
        estimated.exact = false;
        assert!(!needs_availability(&m, &estimated, &[]));
    }

    #[test]
    fn echecs_reconnus() {
        let r = PullFailure::recognize;
        // Réponses de Hugging Face, telles quelles ou reprises par Ollama.
        assert_eq!(
            r(
                "The specified tag is not available in the repository. Please use another tag \
               or \"latest\""
            ),
            Some(PullFailure::UnknownQuant)
        );
        assert_eq!(
            r(
                "This tag is a sharded GGUF. Ollama does not yet support pulling sharded GGUF \
               via the registry"
            ),
            Some(PullFailure::Sharded)
        );
        assert_eq!(
            r("Invalid username or password."),
            Some(PullFailure::Unauthorized)
        );
        assert_eq!(r("pull model manifest: file does not exist"), None);
    }

    #[test]
    fn avertissements_apres_l_ajout() {
        let mut m = model("Qwen3 8B");
        m.use_cases.push(UseCase::Vision);
        let v = m.variants[0].clone();
        let est = estimate(RunMode::Gpu, 40.0);
        let p = preflight(&input(&m, &v, &est, &[], &[]));
        assert!(added_warnings(&p, true, Some(16_384)).is_empty());
        let w = added_warnings(&p, false, Some(8192));
        assert_eq!(w.len(), 2);
        assert!(w[0].starts_with("Vision attendue mais absente dans Ollama"));
        assert!(w[1].contains("un contexte de 8 192") && w[1].contains("16 384"));
        assert!(added_warnings(&p, true, None)[0].contains("aucun contexte"));
    }

    #[test]
    fn messages_des_echecs_de_telechargement() {
        let msg = |f: PullFailure, gated| f.message("Org/Repo-GGUF", "Q4_K_M", gated);
        assert!(msg(PullFailure::Unauthorized, true).contains("dépôt protégé"));
        assert!(msg(PullFailure::Unauthorized, true).contains("id_ed25519.pub"));
        assert!(msg(PullFailure::Unauthorized, false).contains("introuvable"));
        assert!(
            msg(PullFailure::UnknownQuant, false).starts_with("Hugging Face ne propose pas Q4_K_M")
        );
        assert!(msg(PullFailure::Sharded, false).contains("llama.cpp"));
        assert!(
            msg(PullFailure::SplitPart, false)
                .starts_with("Q4_K_M de Org/Repo-GGUF n'est qu'une partie d'un modèle découpé")
        );
    }

    #[test]
    fn progression_limitee() {
        let mut t = ProgressThrottle::new();
        let d = InstallPhase::Downloading;
        assert!(t.ready(InstallPhase::Preparing, ms(0)));
        // Changement d'étape : aussitôt.
        assert!(t.ready(d, ms(10)));
        assert!(!t.ready(d, ms(70)));
        assert!(!t.ready(d, ms(209)));
        assert!(t.ready(d, ms(210)));
        assert!(!t.ready(d, ms(270)));
        assert!(t.ready(InstallPhase::Verifying, ms(280)));
        assert!(t.ready(InstallPhase::Creating, ms(281)));
    }

    #[test]
    fn evenement_de_progression() {
        let m = model("Qwen3 8B");
        let v = m.variants[0].clone();
        let est = estimate(RunMode::Gpu, 40.0);
        let p = preflight(&input(&m, &v, &est, &[], &[]));
        let e = InstallProgress::new(
            &p,
            PullProgress {
                phase: InstallPhase::Downloading,
                completed: 1,
                total: 2,
                bytes_per_s: None,
            },
        );
        assert_eq!(
            serde_json::to_value(e).unwrap(),
            serde_json::json!({
                "repoId": "Qwen/Qwen3-8B-GGUF",
                "displayName": "Qwen3 8B",
                "label": "Q4_K_M",
                "ollamaName": "gram/qwen3-8b:q4_k_m",
                "phase": "downloading",
                "completed": 1,
                "total": 2,
                "bytesPerS": null,
            })
        );
    }

    #[test]
    fn couches_additionnees_et_reprise() {
        let mut t = PullTracker::new();
        assert_eq!(t.progress(ms(0)), PullProgress::default());

        // Couche reprise : 40 Mo déjà là, comptés dans l'avancement mais pas dans le débit.
        t.layer("sha256:a", 175_000_000, Some(40_000_000), ms(0));
        // Deuxième couche annoncée sans `completed`.
        t.layer("sha256:b", 100_000_000, None, ms(100));
        let p = t.progress(ms(100));
        assert_eq!(p.phase, InstallPhase::Downloading);
        assert_eq!((p.completed, p.total), (40_000_000, 275_000_000));
        assert_eq!(p.bytes_per_s, None, "moins d'une seconde de suivi");

        t.layer("sha256:a", 175_000_000, Some(60_000_000), ms(1000));
        t.layer("sha256:b", 100_000_000, None, ms(1500));
        t.layer("sha256:b", 100_000_000, Some(10_000_000), ms(2000));
        let p = t.progress(ms(2000));
        assert_eq!(p.completed, 70_000_000);
        // 30 Mo reçus en 2 s.
        assert_eq!(p.bytes_per_s, Some(15_000_000));

        // `completed` au-delà de `total` n'est pas compté deux fois.
        t.layer("sha256:c", 500, Some(600), ms(2100));
        assert_eq!(t.progress(ms(2100)).completed, 70_000_500);
    }

    #[test]
    fn debit_sur_trois_secondes() {
        let mut t = PullTracker::new();
        // 10 Mo/s pendant 5 s, puis 1 Mo/s.
        for s in 0..=5 {
            t.layer("sha256:a", u64::MAX / 2, Some(s * 10_000_000), ms(s * 1000));
        }
        assert_eq!(t.progress(ms(5000)).bytes_per_s, Some(10_000_000));
        for s in 6..=8 {
            let completed = 50_000_000 + (s - 5) * 1_000_000;
            t.layer("sha256:a", u64::MAX / 2, Some(completed), ms(s * 1000));
        }
        assert_eq!(t.progress(ms(8000)).bytes_per_s, Some(1_000_000));
        // Plus rien n'arrive : le débit retombe (2 Mo depuis 6 s, en 3,5 s), puis s'annule.
        assert_eq!(t.progress(ms(9500)).bytes_per_s, Some(571_429));
        assert_eq!(t.progress(ms(12_000)).bytes_per_s, Some(0));
        // Seuls les échantillons utiles sont gardés.
        assert!(t.samples.len() <= 4, "{:?}", t.samples);
    }

    #[test]
    fn etapes() {
        let mut t = PullTracker::new();
        assert_eq!(t.phase(), InstallPhase::Preparing);
        t.layer("sha256:a", 1000, Some(990), ms(0));
        t.layer("sha256:a", 1000, Some(999), ms(2000));
        assert_eq!(t.progress(ms(2000)).completed, 999);
        assert!(t.progress(ms(2000)).bytes_per_s.is_some());

        // La dernière ligne n'atteint pas `total` : la vérification vaut 100 %.
        t.set_phase(InstallPhase::Verifying);
        let p = t.progress(ms(2000));
        assert_eq!(
            (p.phase, p.completed, p.total),
            (InstallPhase::Verifying, 1000, 1000)
        );
        assert_eq!(p.bytes_per_s, None);

        // Une étape ne recule pas.
        t.layer("sha256:a", 1000, Some(1000), ms(3000));
        t.set_phase(InstallPhase::Preparing);
        assert_eq!(t.phase(), InstallPhase::Verifying);
        t.set_phase(InstallPhase::Creating);
        assert_eq!(t.progress(ms(3000)).phase, InstallPhase::Creating);
    }

    #[test]
    fn progression_en_camel_case() {
        let p = PullProgress {
            phase: InstallPhase::Downloading,
            completed: 1,
            total: 2,
            bytes_per_s: Some(3),
        };
        assert_eq!(
            serde_json::to_value(p).unwrap(),
            serde_json::json!({ "phase": "downloading", "completed": 1, "total": 2, "bytesPerS": 3 })
        );
    }

    fn disk(mount: &str, available: u64) -> DiskInfo {
        DiskInfo {
            mount: mount.into(),
            total: available * 2,
            available,
        }
    }

    #[test]
    fn disque_du_dossier() {
        let disks = [
            disk("/", 100),
            disk("/home", 200),
            disk("/mnt/d", 300),
            disk("/home/gram/data", 400),
        ];
        let mount = |dir| disk_check(&disks, dir, 0).map(|d| d.mount);
        // Montages imbriqués : le plus long préfixe.
        assert_eq!(mount("/home/gram/.ollama/models").as_deref(), Some("/home"));
        assert_eq!(
            mount("/home/gram/data/ollama").as_deref(),
            Some("/home/gram/data")
        );
        assert_eq!(mount("/home/gram/data").as_deref(), Some("/home/gram/data"));
        // Segment par segment : `/mnt/d` ne contient pas `/mnt/data`.
        assert_eq!(mount("/mnt/data/models").as_deref(), Some("/"));
        assert_eq!(mount("/mnt/d/models").as_deref(), Some("/mnt/d"));
        assert_eq!(mount("relatif/models"), None);

        let windows = [
            disk("C:\\", 100),
            disk("D:\\", 200),
            disk("C:\\mnt\\rapide\\", 300),
        ];
        let mount = |dir| disk_check(&windows, dir, 0).map(|d| d.mount);
        assert_eq!(
            mount("C:\\Users\\gram\\.ollama\\models").as_deref(),
            Some("C:\\")
        );
        // `OLLAMA_MODELS` sur un autre disque, casse et séparateurs libres.
        assert_eq!(mount("d:/Ollama/models").as_deref(), Some("D:\\"));
        assert_eq!(
            mount("C:\\MNT\\Rapide\\ollama").as_deref(),
            Some("C:\\mnt\\rapide\\")
        );
        assert_eq!(mount("C:\\mnt\\rapidement").as_deref(), Some("C:\\"));
        assert_eq!(mount("E:\\ollama"), None);
        assert_eq!(disk_check(&[], "C:\\", 1), None);
    }

    #[test]
    fn marge_de_dix_pour_cent() {
        let check = |available, size| disk_check(&[disk("C:\\", available)], "C:\\x", size);
        let ok = check(1100, 1000).unwrap();
        assert_eq!(
            ok,
            DiskSpace {
                mount: "C:\\".into(),
                available: 1100,
                needed: 1100,
                enough: true,
            }
        );
        assert!(!check(1099, 1000).unwrap().enough);
        // Rien à télécharger : toujours assez.
        assert!(check(0, 0).unwrap().enough);
        assert_eq!(check(1, u64::MAX).unwrap().needed, u64::MAX);
    }
}
