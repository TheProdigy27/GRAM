//! `models.json` : les modèles que GRAM a ajoutés à Ollama pour GRAM Harness.
//!
//! Écrit par GRAM seul (application ou `gram-cli`), lu par les deux applications. Il enrichit
//! la liste d'Ollama, qui fait foi : nom lisible, usage, vitesses estimée et mesurée. Ses
//! règles :
//!
//! - un modèle dérivé par modèle et quantification : le rajouter remplace son entrée
//!   ([`SharedManifest::upsert`]) ;
//! - la base d'un modèle dérivé ne se supprime que si GRAM l'a téléchargée et qu'aucune autre
//!   entrée ne l'utilise ([`SharedManifest::base_removable`]).

use crate::{json, same_model};
use chrono::{DateTime, Utc};
use gram_core::{KvCacheType, Preflight, UseCase};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;
use std::sync::Mutex;
use ts_rs::TS;

/// Nom du fichier, dans le dossier partagé.
pub const MANIFEST_FILE: &str = "models.json";
/// Version du format : un manifeste d'une version supérieure est ignoré à la lecture, et
/// jamais écrasé.
pub const MANIFEST_VERSION: u32 = 1;

/// Contenu de `models.json`. Un champ ajouté plus tard prend `#[serde(default)]` ; les champs
/// inconnus sont ignorés.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct SharedManifest {
    pub version: u32,
    pub updated_at: DateTime<Utc>,
    pub models: Vec<SharedModel>,
}

impl Default for SharedManifest {
    fn default() -> Self {
        Self {
            version: MANIFEST_VERSION,
            updated_at: DateTime::<Utc>::default(),
            models: Vec::new(),
        }
    }
}

/// Un modèle dérivé ajouté par GRAM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SharedModel {
    /// Nom du modèle dérivé dans Ollama, clé de l'entrée : `gram/qwen3-8b:q4_k_m`.
    pub ollama_name: String,
    /// Nom exact de la base dans `/api/tags` : `hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M`.
    pub base_name: String,
    /// GRAM a téléchargé la base : « Retirer » peut la supprimer.
    pub base_installed_by_gram: bool,
    pub display_name: String,
    /// Dépôt canonique du modèle dans le catalogue de GRAM.
    pub repo_id: String,
    /// Étiquette de la quantification : « Q4_K_M ».
    pub quant: String,
    /// Taille de la quantification, en octets.
    pub size: u64,
    /// Contexte réglé dans le modèle dérivé, en tokens.
    pub ctx: u32,
    /// Cache KV conseillé (réglage global d'Ollama), avec lequel le contexte a été calculé.
    pub kv_cache: KvCacheType,
    /// Usage actif dans GRAM lors de l'ajout.
    pub use_case: UseCase,
    /// Accepte des images d'après le catalogue.
    pub vision: bool,
    /// Vitesse estimée sur la machine réelle, en tok/s.
    pub estimated_tok_s: f32,
    /// Dernière mesure réelle de la base, en tok/s ; absente sans mesure.
    pub measured_tok_s: Option<f32>,
    pub added_at: DateTime<Utc>,
}

impl SharedModel {
    /// Entrée d'un modèle dérivé qui vient d'être créé : `base_name` est le nom exact de la
    /// base dans Ollama, `pulled` dit si GRAM vient de la télécharger, `measured_tok_s` reprend
    /// la dernière mesure de cette base.
    pub fn new(
        p: &Preflight,
        use_case: UseCase,
        base_name: String,
        pulled: bool,
        measured_tok_s: Option<f32>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            ollama_name: p.ollama_name.clone(),
            base_name,
            base_installed_by_gram: pulled,
            display_name: p.display_name.clone(),
            repo_id: p.repo_id.clone(),
            quant: p.label.clone(),
            size: p.size,
            ctx: p.ctx,
            kv_cache: p.kv_cache,
            use_case,
            vision: p.vision,
            estimated_tok_s: p.estimated_tok_s,
            measured_tok_s,
            added_at: now,
        }
    }
}

impl SharedManifest {
    /// Entrée d'un modèle dérivé.
    pub fn get(&self, ollama_name: &str) -> Option<&SharedModel> {
        self.models
            .iter()
            .find(|m| same_model(&m.ollama_name, ollama_name))
    }

    /// Ajoute une entrée, ou remplace celle du même modèle dérivé (autre contexte, autre dépôt
    /// du même modèle). Renvoie la base de l'entrée remplacée quand il faut la supprimer
    /// d'Ollama : GRAM l'avait téléchargée et plus aucune entrée ne l'utilise.
    ///
    /// Une base que GRAM a téléchargée pour une entrée existante le reste pour la nouvelle :
    /// « Retirer » pourra toujours la supprimer.
    pub fn upsert(&mut self, mut entry: SharedModel) -> Option<String> {
        entry.base_installed_by_gram |= self
            .models
            .iter()
            .any(|m| m.base_installed_by_gram && same_model(&m.base_name, &entry.base_name));
        let replaced = match self
            .models
            .iter()
            .position(|m| same_model(&m.ollama_name, &entry.ollama_name))
        {
            Some(i) => Some(std::mem::replace(&mut self.models[i], entry)),
            None => {
                self.models.push(entry);
                None
            }
        }?;
        (replaced.base_installed_by_gram && !self.uses_base(&replaced.base_name))
            .then_some(replaced.base_name)
    }

    /// Retire l'entrée d'un modèle dérivé et la renvoie.
    pub fn remove(&mut self, ollama_name: &str) -> Option<SharedModel> {
        let i = self
            .models
            .iter()
            .position(|m| same_model(&m.ollama_name, ollama_name))?;
        Some(self.models.remove(i))
    }

    /// « Retirer » peut supprimer aussi la base de `entry` : GRAM l'a téléchargée et aucune
    /// autre entrée ne l'utilise.
    pub fn base_removable(&self, entry: &SharedModel) -> bool {
        entry.base_installed_by_gram
            && !self.models.iter().any(|m| {
                !same_model(&m.ollama_name, &entry.ollama_name)
                    && same_model(&m.base_name, &entry.base_name)
            })
    }

    fn uses_base(&self, base: &str) -> bool {
        self.models.iter().any(|m| same_model(&m.base_name, base))
    }

    /// Retire les modèles dérivés absents d'Ollama (`installed` : noms de `/api/tags`, lus à
    /// `listed_at`). Une entrée ajoutée après la lecture de la liste est gardée : elle n'y
    /// figurait pas encore. Renvoie le nombre d'entrées retirées.
    pub fn retain_installed(&mut self, installed: &[&str], listed_at: DateTime<Utc>) -> usize {
        let before = self.models.len();
        self.models.retain(|m| {
            m.added_at > listed_at || installed.iter().any(|n| same_model(n, &m.ollama_name))
        });
        before - self.models.len()
    }

    /// Reporte une mesure réelle sur les entrées dont la base est le modèle mesuré. Renvoie le
    /// nombre d'entrées mises à jour.
    pub fn record_measurement(&mut self, measured_model: &str, tok_s: f32) -> usize {
        let mut n = 0;
        for m in &mut self.models {
            if same_model(&m.base_name, measured_model) {
                m.measured_tok_s = Some(tok_s);
                n += 1;
            }
        }
        n
    }
}

/// Lecture du fichier, avant toute tolérance.
enum Stored {
    Absent,
    Invalid(String),
    Future(u32),
    Valid(SharedManifest),
}

fn load(dir: &Path) -> Stored {
    let path = dir.join(MANIFEST_FILE);
    let bytes = match json::read(&path) {
        Ok(Some(b)) => b,
        Ok(None) => return Stored::Absent,
        Err(e) => return Stored::Invalid(e.to_string()),
    };
    #[derive(Deserialize)]
    struct Version {
        #[serde(default)]
        version: u32,
    }
    match serde_json::from_slice::<Version>(&bytes) {
        Ok(v) if v.version > MANIFEST_VERSION => Stored::Future(v.version),
        Ok(_) => match serde_json::from_slice(&bytes) {
            Ok(m) => Stored::Valid(m),
            Err(e) => Stored::Invalid(e.to_string()),
        },
        Err(e) => Stored::Invalid(e.to_string()),
    }
}

/// Manifeste du dossier partagé. Absent : vide. Illisible, invalide ou d'une version
/// inconnue : vide aussi, avec un avertissement dans le journal.
pub fn read_manifest(dir: &Path) -> SharedManifest {
    match load(dir) {
        Stored::Valid(m) => m,
        Stored::Absent => SharedManifest::default(),
        Stored::Invalid(e) => {
            tracing::warn!(dir = %dir.display(), "{MANIFEST_FILE} illisible, ignoré : {e}");
            SharedManifest::default()
        }
        Stored::Future(v) => {
            tracing::warn!(
                dir = %dir.display(),
                version = v,
                "{MANIFEST_FILE} d'une version plus récente de GRAM, ignoré"
            );
            SharedManifest::default()
        }
    }
}

/// Une seule modification du manifeste à la fois dans le processus.
static UPDATE_LOCK: Mutex<()> = Mutex::new(());

/// Lit le manifeste, le modifie avec `f` puis l'écrit de façon atomique s'il a changé (avec
/// la date de modification), sous un verrou du processus. Un fichier invalide repart d'un
/// manifeste vide ; un manifeste d'une version plus récente n'est jamais écrasé (erreur).
pub fn update_manifest<R>(dir: &Path, f: impl FnOnce(&mut SharedManifest) -> R) -> io::Result<R> {
    let _guard = UPDATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let current = match load(dir) {
        Stored::Valid(m) => m,
        Stored::Absent => SharedManifest::default(),
        Stored::Invalid(e) => {
            tracing::warn!(dir = %dir.display(), "{MANIFEST_FILE} illisible, remplacé : {e}");
            SharedManifest::default()
        }
        Stored::Future(v) => {
            return Err(io::Error::other(format!(
                "{MANIFEST_FILE} vient d'une version plus récente de GRAM (format {v}) : mettre \
                 GRAM à jour"
            )));
        }
    };
    let mut next = current.clone();
    let result = f(&mut next);
    if next != current {
        next.version = MANIFEST_VERSION;
        next.updated_at = Utc::now();
        json::save(&dir.join(MANIFEST_FILE), &next)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::tests::TempDir;
    use chrono::TimeZone;

    fn at(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 12, 10, minute, 0).unwrap()
    }

    fn entry(alias: &str, base: &str, pulled: bool) -> SharedModel {
        SharedModel {
            ollama_name: alias.into(),
            base_name: base.into(),
            base_installed_by_gram: pulled,
            display_name: "Qwen3 8B".into(),
            repo_id: "Qwen/Qwen3-8B-GGUF".into(),
            quant: "Q4_K_M".into(),
            size: 5_027_797_508,
            ctx: 16_384,
            kv_cache: KvCacheType::F16,
            use_case: UseCase::Code,
            vision: false,
            estimated_tok_s: 138.2,
            measured_tok_s: None,
            added_at: at(0),
        }
    }

    const ALIAS: &str = "gram/qwen3-8b:q4_k_m";
    const BASE: &str = "hf.co/Qwen/Qwen3-8B-GGUF:Q4_K_M";
    const OTHER_BASE: &str = "hf.co/unsloth/Qwen3-8B-GGUF:Q4_K_M";

    #[test]
    fn format_en_camel_case() {
        let m = SharedManifest {
            updated_at: at(1),
            models: vec![entry(ALIAS, BASE, true)],
            ..SharedManifest::default()
        };
        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["version"], 1);
        assert_eq!(json["updatedAt"], "2026-11-12T10:01:00Z");
        let e = &json["models"][0];
        assert_eq!(e["ollamaName"], ALIAS);
        assert_eq!(e["baseInstalledByGram"], true);
        assert_eq!(e["kvCache"], "f16");
        assert_eq!(e["useCase"], "code");
        assert_eq!(e["measuredTokS"], serde_json::Value::Null);
        assert_eq!(serde_json::from_value::<SharedManifest>(json).unwrap(), m);

        // Champs inconnus ignorés ; sans `models` : vide.
        let m: SharedManifest =
            serde_json::from_str(r#"{ "version": 1, "plusTard": true }"#).unwrap();
        assert!(m.models.is_empty());
    }

    #[test]
    fn remplacement() {
        let mut m = SharedManifest::default();
        assert_eq!(m.upsert(entry(ALIAS, BASE, true)), None);
        assert_eq!(
            m.upsert(entry("gram/autre:q8_0", "hf.co/a/b:Q8_0", false)),
            None
        );

        // Même base, autre contexte : rien à supprimer, et GRAM reste celui qui l'a
        // téléchargée (le nouvel ajout l'a trouvée déjà là).
        let mut again = entry("GRAM/Qwen3-8B:Q4_K_M", BASE, false);
        again.ctx = 32_768;
        assert_eq!(m.upsert(again), None);
        assert_eq!(m.models.len(), 2);
        let e = m.get(ALIAS).unwrap();
        assert_eq!(e.ctx, 32_768);
        assert!(e.base_installed_by_gram);

        // Autre dépôt du même modèle : l'ancienne base, téléchargée par GRAM et plus utilisée,
        // est à supprimer.
        assert_eq!(
            m.upsert(entry(ALIAS, OTHER_BASE, true)),
            Some(BASE.to_string())
        );
        assert_eq!(m.get(ALIAS).unwrap().base_name, OTHER_BASE);

        // Ancienne base qu'un autre modèle dérivé utilise encore : gardée.
        m.upsert(entry("gram/qwen3-8b-copie:q4_k_m", OTHER_BASE, false));
        assert_eq!(m.upsert(entry(ALIAS, BASE, true)), None);
        // Ancienne base que GRAM n'a pas téléchargée : gardée.
        let mut m = SharedManifest::default();
        m.upsert(entry(ALIAS, BASE, false));
        assert_eq!(m.upsert(entry(ALIAS, OTHER_BASE, true)), None);
    }

    #[test]
    fn retrait_et_base_supprimable() {
        let mut m = SharedManifest::default();
        m.upsert(entry(ALIAS, BASE, true));
        assert!(m.base_removable(m.get(ALIAS).unwrap()));

        // Base partagée avec un autre modèle dérivé : gardée.
        m.upsert(entry(
            "gram/qwen3-8b-copie:q4_k_m",
            "HF.CO/qwen/qwen3-8b-gguf:q4_k_m",
            false,
        ));
        assert!(!m.base_removable(m.get(ALIAS).unwrap()));
        assert!(m.remove("gram/qwen3-8b-copie:q4_k_m").is_some());
        assert!(m.base_removable(m.get(ALIAS).unwrap()));

        // Base que GRAM n'a pas téléchargée : gardée.
        let mut m = SharedManifest::default();
        m.upsert(entry(ALIAS, BASE, false));
        assert!(!m.base_removable(m.get(ALIAS).unwrap()));
        assert_eq!(m.remove(ALIAS).unwrap().ollama_name, ALIAS);
        assert_eq!(m.remove(ALIAS), None);
        assert!(m.models.is_empty());
    }

    #[test]
    fn modeles_disparus_et_mesures() {
        let mut m = SharedManifest::default();
        m.upsert(entry(ALIAS, BASE, true));
        let mut late = entry("gram/recent:q4_k_m", OTHER_BASE, true);
        late.added_at = at(10);
        m.upsert(late);
        m.upsert(entry("gram/supprime:q4_k_m", OTHER_BASE, false));

        // Liste lue à 10 h 05 : l'entrée de 10 h 10 n'y figurait pas encore.
        let installed = ["GRAM/QWEN3-8B:Q4_K_M", BASE];
        assert_eq!(m.retain_installed(&installed, at(5)), 1);
        let names: Vec<&str> = m.models.iter().map(|e| e.ollama_name.as_str()).collect();
        assert_eq!(names, [ALIAS, "gram/recent:q4_k_m"]);

        assert_eq!(
            m.record_measurement("hf.co/qwen/qwen3-8b-gguf:q4_k_m", 135.3),
            1
        );
        assert_eq!(m.get(ALIAS).unwrap().measured_tok_s, Some(135.3));
        assert_eq!(m.record_measurement("qwen3:8b", 50.0), 0);
    }

    #[test]
    fn lecture_et_ecriture() {
        let dir = TempDir::new("manifeste");
        let path = dir.0.join(MANIFEST_FILE);
        assert_eq!(read_manifest(&dir.0), SharedManifest::default());

        // Rien de changé : rien d'écrit.
        update_manifest(&dir.0, |m| m.get(ALIAS).is_some()).unwrap();
        assert!(!path.exists());

        let removed = update_manifest(&dir.0, |m| m.upsert(entry(ALIAS, BASE, true))).unwrap();
        assert_eq!(removed, None);
        let m = read_manifest(&dir.0);
        assert_eq!(m.models, [entry(ALIAS, BASE, true)]);
        assert!(m.updated_at > DateTime::<Utc>::default());

        // Fichier illisible : manifeste vide à la lecture, remplacé à l'écriture.
        std::fs::write(&path, b"{ pas du json").unwrap();
        assert_eq!(read_manifest(&dir.0), SharedManifest::default());
        update_manifest(&dir.0, |m| m.upsert(entry(ALIAS, OTHER_BASE, false))).unwrap();
        assert_eq!(read_manifest(&dir.0).models[0].base_name, OTHER_BASE);

        // Version future : ignorée à la lecture, jamais écrasée.
        let future = r#"{ "version": 2, "updatedAt": "2027-01-01T00:00:00Z", "models": [] }"#;
        std::fs::write(&path, future).unwrap();
        assert_eq!(read_manifest(&dir.0), SharedManifest::default());
        let err = update_manifest(&dir.0, |m| m.upsert(entry(ALIAS, BASE, true))).unwrap_err();
        assert!(err.to_string().contains("version plus récente"), "{err}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), future);
    }
}
