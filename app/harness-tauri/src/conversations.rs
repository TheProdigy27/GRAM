//! Conversations enregistrées : un fichier JSON par conversation, `conversations/<id>.json`
//! dans le dossier de données de GRAM Harness (`fr.gram.harness/`), écrit de façon atomique
//! (`gram_link::json::save`).
//!
//! Chaque modification relit le fichier, le change et le réécrit sous un verrou du processus :
//! renommer une conversation pendant une réponse ne perd ni le titre ni la réponse.

use chrono::{DateTime, Utc};
use gram_ollama::ChatDone;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use ts_rs::TS;

/// Version du format des fichiers. Un fichier d'une version supérieure (GRAM Harness plus
/// récent, puis revenu en arrière) n'est ni listé ni modifié : ses champs inconnus seraient
/// perdus à la réécriture.
pub const CONVERSATION_VERSION: u32 = 1;
/// Longueur maximale du titre tiré du premier message, en caractères.
pub const AUTO_TITLE_CHARS: usize = 60;
/// Longueur maximale d'un titre choisi, en caractères.
pub const TITLE_MAX_CHARS: usize = 120;
/// Titre d'une conversation dont le premier message n'a aucun mot.
const UNTITLED: &str = "Nouvelle conversation";

/// Auteur d'un message. Les instructions ne sont pas un message : elles vont dans
/// `Conversation::instructions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ChatRole {
    User,
    Assistant,
}

/// Statistiques d'une réponse, d'après le bilan d'Ollama (durées en nanosecondes).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ChatStats {
    /// `stop` (fin naturelle), `length` (limite de tokens atteinte)…
    pub done_reason: Option<String>,
    /// Tokens de la conversation envoyée (cache compris).
    pub prompt_eval_count: u32,
    /// Tokens générés, réflexion comprise.
    pub eval_count: u32,
    pub eval_duration: u64,
    /// Chargement du modèle, s'il n'était pas déjà en mémoire.
    pub load_duration: u64,
    pub prompt_eval_duration: u64,
    pub total_duration: u64,
}

impl From<ChatDone> for ChatStats {
    fn from(d: ChatDone) -> Self {
        Self {
            done_reason: d.done_reason,
            prompt_eval_count: d.prompt_eval_count,
            eval_count: d.eval_count,
            eval_duration: d.eval_duration,
            load_duration: d.load_duration,
            prompt_eval_duration: d.prompt_eval_duration,
            total_duration: d.total_duration,
        }
    }
}

/// Un message de la conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChatMessage {
    pub role: ChatRole,
    /// Texte du message (Markdown pour une réponse) ; vide pour une réponse en échec avant son
    /// premier mot.
    pub content: String,
    /// Réflexion du modèle avant sa réponse.
    #[serde(default)]
    pub thinking: Option<String>,
    /// Modèle qui a répondu (nom dans Ollama) : on peut en changer en cours de conversation.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub stats: Option<ChatStats>,
    /// Réponse arrêtée par l'utilisateur ; le début est gardé.
    #[serde(default)]
    pub interrupted: bool,
    /// Échec de la réponse (Ollama arrêté, modèle disparu…), en français ; le début reçu est
    /// gardé.
    #[serde(default)]
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>, at: DateTime<Utc>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
            thinking: None,
            model: None,
            stats: None,
            interrupted: false,
            error: None,
            created_at: at,
        }
    }

    pub fn assistant(content: String, model: &str, at: DateTime<Utc>) -> Self {
        Self {
            role: ChatRole::Assistant,
            model: Some(model.to_string()),
            ..Self::user(content, at)
        }
    }
}

/// Une conversation enregistrée.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Conversation {
    /// Nom du fichier, sans `.json` : chiffres, lettres et `-`.
    pub id: String,
    /// Début du premier message, ou titre choisi.
    pub title: String,
    /// Dernier modèle utilisé (nom dans Ollama).
    pub model: String,
    /// Instructions, envoyées en message `system` avant la conversation.
    #[serde(default)]
    pub instructions: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Dernier message ; un changement de titre ne la modifie pas.
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<ChatMessage>,
}

impl Conversation {
    pub fn summary(&self) -> ConversationSummary {
        ConversationSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            model: self.model.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// Une conversation dans la liste de la barre latérale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub model: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Contenu d'un fichier : la conversation et la version du format.
#[derive(Serialize, Deserialize)]
struct ConversationFile {
    version: u32,
    #[serde(flatten)]
    conversation: Conversation,
}

/// Début d'un fichier, pour la liste (les messages sont sautés).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HeadFile {
    version: u32,
    #[serde(flatten)]
    summary: ConversationSummary,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Conversation introuvable : elle a peut-être été supprimée")]
    NotFound,
    #[error("Conversation illisible ({0}) : le fichier est peut-être abîmé")]
    Invalid(String),
    #[error(
        "Conversation enregistrée par une version plus récente de GRAM Harness : mettre GRAM \
         Harness à jour pour l'ouvrir"
    )]
    Newer,
    #[error("Conversation non enregistrée ({0}) : vérifier la place sur le disque")]
    Io(String),
}

/// Les conversations d'un dossier.
#[derive(Debug)]
pub struct ConversationStore {
    dir: PathBuf,
    /// Une modification à la fois (lecture, changement, écriture).
    lock: Mutex<()>,
}

impl ConversationStore {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            lock: Mutex::new(()),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Toutes les conversations, la plus récente d'abord. Dossier absent : aucune ; fichier
    /// illisible ou d'une version future : écarté (journalisé).
    pub fn list(&self) -> Vec<ConversationSummary> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut list: Vec<ConversationSummary> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let id = path.file_name()?.to_str()?.strip_suffix(".json")?;
                valid_id(id).then_some(())?;
                read_head(&path)
                    .inspect_err(|e| {
                        tracing::warn!(path = %path.display(), "conversation écartée : {e}");
                    })
                    .ok()
            })
            .collect();
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(b.id.cmp(&a.id)));
        list
    }

    pub fn get(&self, id: &str) -> Result<Conversation, StoreError> {
        read_file(&self.path(id)?)
    }

    /// Enregistre une nouvelle conversation sous un identifiant libre, tiré de `created_at`.
    pub fn create(&self, mut conversation: Conversation) -> Result<Conversation, StoreError> {
        let _guard = self.lock();
        let base = conversation
            .created_at
            .format("%Y%m%d-%H%M%S%3f")
            .to_string();
        let id = (0..)
            .map(|n| {
                if n == 0 {
                    base.clone()
                } else {
                    format!("{base}-{n}")
                }
            })
            .find(|id| !self.dir.join(format!("{id}.json")).exists())
            .expect("identifiant libre");
        conversation.id = id;
        self.write(&conversation)?;
        Ok(conversation)
    }

    /// Relit la conversation, la change avec `f` et la réécrit ; rien n'est écrit si `f`
    /// échoue.
    pub fn update<R, E: From<StoreError>>(
        &self,
        id: &str,
        f: impl FnOnce(&mut Conversation) -> Result<R, E>,
    ) -> Result<R, E> {
        let path = self.path(id)?;
        let _guard = self.lock();
        let mut conversation = read_file(&path)?;
        let result = f(&mut conversation)?;
        conversation.id = id.to_string();
        self.write(&conversation)?;
        Ok(result)
    }

    /// Supprime la conversation ; `false` si elle n'existait pas.
    pub fn delete(&self, id: &str) -> Result<bool, StoreError> {
        let path = self.path(id)?;
        let _guard = self.lock();
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(StoreError::Io(e.to_string())),
        }
    }

    /// Chemin du fichier ; un identifiant qui n'en est pas un (`../x`…) est introuvable.
    fn path(&self, id: &str) -> Result<PathBuf, StoreError> {
        if valid_id(id) {
            Ok(self.dir.join(format!("{id}.json")))
        } else {
            Err(StoreError::NotFound)
        }
    }

    fn write(&self, conversation: &Conversation) -> Result<(), StoreError> {
        let path = self.dir.join(format!("{}.json", conversation.id));
        let file = ConversationFile {
            version: CONVERSATION_VERSION,
            conversation: conversation.clone(),
        };
        gram_link::json::save(&path, &file).map_err(|e| StoreError::Io(e.to_string()))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, StoreError> {
    match gram_link::json::read(path) {
        Ok(Some(bytes)) => Ok(bytes),
        Ok(None) => Err(StoreError::NotFound),
        Err(e) => Err(StoreError::Invalid(e.to_string())),
    }
}

fn read_file(path: &Path) -> Result<Conversation, StoreError> {
    let bytes = read_bytes(path)?;
    check_version(&bytes)?;
    let file: ConversationFile =
        serde_json::from_slice(&bytes).map_err(|e| StoreError::Invalid(e.to_string()))?;
    Ok(file.conversation)
}

fn read_head(path: &Path) -> Result<ConversationSummary, StoreError> {
    let bytes = read_bytes(path)?;
    let head: HeadFile =
        serde_json::from_slice(&bytes).map_err(|e| StoreError::Invalid(e.to_string()))?;
    if head.version > CONVERSATION_VERSION {
        return Err(StoreError::Newer);
    }
    Ok(head.summary)
}

/// Version lue avant le reste : un fichier d'une version future peut avoir changé de forme.
fn check_version(bytes: &[u8]) -> Result<(), StoreError> {
    #[derive(Deserialize)]
    struct Version {
        version: u32,
    }
    match serde_json::from_slice::<Version>(bytes) {
        Ok(v) if v.version > CONVERSATION_VERSION => Err(StoreError::Newer),
        _ => Ok(()),
    }
}

/// Identifiant de conversation : 1 à 64 caractères parmi `[A-Za-z0-9-]`, donc un nom de
/// fichier sûr.
pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Titre tiré du premier message : espaces réunis, 60 caractères au plus (« … » à la coupe).
pub fn auto_title(content: &str) -> String {
    let text = collapse(content);
    if text.is_empty() {
        UNTITLED.to_string()
    } else {
        clip(&text, AUTO_TITLE_CHARS)
    }
}

/// Titre choisi : espaces réunis, 120 caractères au plus ; `None` s'il est vide.
pub fn clean_title(title: &str) -> Option<String> {
    let text = collapse(title);
    (!text.is_empty()).then(|| clip(&text, TITLE_MAX_CHARS))
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max - 1).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Dossier temporaire supprimé à la fin du test.
    pub(crate) struct TempDir(pub PathBuf);

    impl TempDir {
        pub(crate) fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("gram-harness-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    pub(crate) fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 13, h, m, 0).unwrap()
    }

    pub(crate) fn conversation(content: &str, created: DateTime<Utc>) -> Conversation {
        Conversation {
            id: String::new(),
            title: auto_title(content),
            model: "gram/qwen3-1.7b:q4_k_m".into(),
            instructions: None,
            created_at: created,
            updated_at: created,
            messages: vec![ChatMessage::user(content, created)],
        }
    }

    #[test]
    fn aller_retour() {
        let dir = TempDir::new("conv-aller-retour");
        let store = ConversationStore::new(dir.0.join("conversations"));
        assert!(store.list().is_empty(), "dossier absent");

        let first = store.create(conversation("Bonjour", at(9, 0))).unwrap();
        assert_eq!(first.id, "20261113-090000000");
        let second = store.create(conversation("Salut", at(9, 0))).unwrap();
        assert_eq!(second.id, "20261113-090000000-1", "même instant : suffixe");

        // Réponse complète, puis relue telle quelle.
        let mut answer = ChatMessage::assistant("**Bonjour !**".into(), "qwen3:8b", at(9, 1));
        answer.thinking = Some("L'utilisateur salue.".into());
        answer.stats = Some(ChatStats {
            done_reason: Some("stop".into()),
            prompt_eval_count: 18,
            eval_count: 104,
            eval_duration: 252_300_000,
            load_duration: 1_467_873_600,
            prompt_eval_duration: 62_248_000,
            total_duration: 1_784_520_700,
        });
        let pushed = answer.clone();
        store
            .update(&first.id, |c| {
                c.messages.push(pushed);
                c.updated_at = at(9, 1);
                Ok::<_, StoreError>(())
            })
            .unwrap();
        let read = store.get(&first.id).unwrap();
        assert_eq!(read.messages, [first.messages[0].clone(), answer]);
        assert_eq!(read.updated_at, at(9, 1));

        // Format du fichier : version, camelCase, dates RFC 3339.
        let raw: serde_json::Value = serde_json::from_slice(
            &std::fs::read(store.dir().join(format!("{}.json", first.id))).unwrap(),
        )
        .unwrap();
        assert_eq!(raw["version"], 1);
        assert_eq!(raw["createdAt"], "2026-11-13T09:00:00Z");
        assert_eq!(raw["messages"][1]["stats"]["evalCount"], 104);
        assert_eq!(raw["messages"][1]["interrupted"], false);

        // Liste : la plus récente d'abord.
        let ids: Vec<String> = store.list().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, [first.id.clone(), second.id.clone()]);

        assert!(store.delete(&second.id).unwrap());
        assert!(!store.delete(&second.id).unwrap(), "déjà supprimée");
        assert!(matches!(store.get(&second.id), Err(StoreError::NotFound)));
        let missing = store.update(&second.id, |_| Ok::<_, StoreError>(()));
        assert!(matches!(missing, Err(StoreError::NotFound)), "pas recréée");
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn modification_refusee_rien_n_est_ecrit() {
        let dir = TempDir::new("conv-refus");
        let store = ConversationStore::new(dir.0.clone());
        let c = store.create(conversation("Bonjour", at(9, 0))).unwrap();
        let result: Result<(), String> = store.update(&c.id, |c| {
            c.title = "Changé".into();
            Err("refusé".to_string())
        });
        assert_eq!(result, Err("refusé".to_string()));
        assert_eq!(store.get(&c.id).unwrap().title, "Bonjour");
    }

    impl From<StoreError> for String {
        fn from(e: StoreError) -> Self {
            e.to_string()
        }
    }

    #[test]
    fn fichiers_invalides_ou_futurs() {
        let dir = TempDir::new("conv-invalides");
        let store = ConversationStore::new(dir.0.clone());
        let good = store.create(conversation("Bonjour", at(9, 0))).unwrap();
        std::fs::write(dir.0.join("abime.json"), "{ pas du JSON").unwrap();
        std::fs::write(dir.0.join("vide.json"), "{}").unwrap();
        // Version future : écartée de la liste, ni lue ni modifiée.
        let mut future: serde_json::Value = serde_json::from_slice(
            &std::fs::read(dir.0.join(format!("{}.json", good.id))).unwrap(),
        )
        .unwrap();
        future["version"] = 2.into();
        future["id"] = "futur".into();
        future["messages"] = "autre forme".into();
        std::fs::write(dir.0.join("futur.json"), future.to_string()).unwrap();
        // Ni conversation, ni fichier temporaire d'une écriture.
        std::fs::write(dir.0.join("notes.txt"), "x").unwrap();
        std::fs::write(dir.0.join("x.json.123.tmp"), "{}").unwrap();

        let ids: Vec<String> = store.list().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, [good.id]);
        assert!(matches!(store.get("abime"), Err(StoreError::Invalid(_))));
        assert!(matches!(store.get("futur"), Err(StoreError::Newer)));
        let refused = store.update("futur", |c| {
            c.title = "x".into();
            Ok::<_, StoreError>(())
        });
        assert!(matches!(refused, Err(StoreError::Newer)));
        assert!(
            std::fs::read_to_string(dir.0.join("futur.json"))
                .unwrap()
                .contains("autre forme"),
            "pas réécrit"
        );
        assert!(
            StoreError::Newer
                .to_string()
                .contains("mettre GRAM Harness à jour")
        );
    }

    #[test]
    fn identifiants_surs() {
        assert!(valid_id("20261113-090000000-1"));
        for id in [
            "",
            "../x",
            "a/b",
            "a\\b",
            "a.json",
            "a b",
            "é",
            &"a".repeat(65),
        ] {
            assert!(!valid_id(id), "{id}");
        }
        let dir = TempDir::new("conv-ids");
        let store = ConversationStore::new(dir.0.join("conversations"));
        std::fs::write(dir.0.join("secret.json"), "{}").unwrap();
        assert!(matches!(store.get("../secret"), Err(StoreError::NotFound)));
        assert!(matches!(
            store.delete("../secret"),
            Err(StoreError::NotFound)
        ));
        assert!(dir.0.join("secret.json").exists());
    }

    #[test]
    fn titres() {
        assert_eq!(auto_title("  Bonjour\n\n  toi  "), "Bonjour toi");
        assert_eq!(auto_title(" \n "), "Nouvelle conversation");
        let long =
            "Explique-moi la différence entre une quantification Q4_K_M et Q8_0, avec des exemples";
        let title = auto_title(long);
        assert!(title.chars().count() <= AUTO_TITLE_CHARS);
        assert_eq!(
            title,
            "Explique-moi la différence entre une quantification Q4_K_M…"
        );
        // Coupe après un espace : pas d'espace avant « … ».
        assert_eq!(clip("abc def", 5), "abc…");
        assert_eq!(auto_title(&"é".repeat(60)), "é".repeat(60));
        assert_eq!(auto_title(&"🦀".repeat(61)).chars().count(), 60);

        assert_eq!(clean_title("  Mon   titre "), Some("Mon titre".into()));
        assert_eq!(clean_title(" \t "), None);
        assert_eq!(
            clean_title(&"x".repeat(200)).unwrap().chars().count(),
            TITLE_MAX_CHARS
        );
    }
}
