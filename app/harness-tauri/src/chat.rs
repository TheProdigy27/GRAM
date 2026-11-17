//! Discussion : envoyer un message (ou régénérer la dernière réponse), recevoir la réponse en
//! flux, l'arrêter.
//!
//! L'API d'Ollama est sans état : toute la conversation lui est envoyée à chaque fois (les
//! instructions, puis les messages et leurs images). Le message de l'utilisateur est enregistré
//! avant d'interroger Ollama, la réponse une fois finie, arrêtée (début gardé, marqué
//! « interrompue ») ou en échec (début gardé, avec le message d'erreur) : l'interface la montre
//! avec « Réessayer ».
//!
//! Ollama refuse toute la requête si une image s'y trouve, même dans l'historique, et que le
//! modèle n'a pas la vision (constaté avec Ollama 0.40.16) : pour un tel modèle, les images de
//! l'historique ne sont pas envoyées, et un nouveau message avec des images est refusé.

use crate::conversations::{
    ChatMessage, ChatRole, ChatStats, Conversation, ConversationStore, ConversationSummary,
    StoreError, auto_title,
};
use crate::error::{CmdResult, CommandError};
use crate::images;
use chrono::Utc;
use gram_ollama::api::{is_image_unsupported, is_model_not_found};
use gram_ollama::{ChatOptions, ChatRequest, OllamaClient, OllamaError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use ts_rs::TS;

/// Attente maximale de `/api/ps` après une réponse, pour le contexte du modèle chargé.
const PS_TIMEOUT: Duration = Duration::from_secs(2);

/// Garder le modèle en mémoire après la réponse (paramètres de GRAM Harness).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum KeepAlive {
    /// Réglage d'Ollama : 5 min, sauf si `OLLAMA_KEEP_ALIVE` dit autre chose.
    #[default]
    Ollama,
    /// 30 minutes.
    HalfHour,
    /// Jusqu'à l'arrêt d'Ollama.
    Forever,
}

impl KeepAlive {
    /// Valeur de `keep_alive` envoyée à Ollama ; `None` : son réglage.
    pub fn as_ollama(self) -> Option<&'static str> {
        match self {
            Self::Ollama => None,
            Self::HalfHour => Some("30m"),
            // « -1 » sans unité est refusé par Ollama.
            Self::Forever => Some("-1m"),
        }
    }
}

/// Requête de `chat_send`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChatSend {
    /// Conversation à poursuivre ; absente : nouvelle conversation (avec `content`).
    pub conversation_id: Option<String>,
    /// Modèle qui répond (nom dans Ollama) ; il peut changer en cours de conversation.
    pub model: String,
    /// Nouveau message. Absent : la dernière réponse est retirée et redemandée (« Régénérer »,
    /// « Réessayer »).
    pub content: Option<String>,
    /// Images jointes au nouveau message, en base64 sans préfixe `data:` : PNG, JPEG ou WebP,
    /// 10 Mo au plus, 4 au plus. Le message peut alors être sans texte.
    #[serde(default)]
    pub images: Vec<String>,
    /// Réflexion, pour un modèle qui sait réfléchir ; absente : le modèle décide (Qwen3
    /// réfléchit). Retenue par la conversation.
    #[serde(default)]
    pub think: Option<bool>,
    /// Garder le modèle en mémoire après la réponse.
    #[serde(default)]
    pub keep_alive: KeepAlive,
    /// Instructions d'une nouvelle conversation (celles par défaut des paramètres, ou changées
    /// avant le premier message). Ignorées pour une conversation existante (`set_instructions`).
    #[serde(default)]
    pub instructions: Option<String>,
}

/// Événements d'une réponse, dans l'ordre : `started`, des `delta`, puis `done`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export)]
pub enum ChatEvent {
    /// Message de l'utilisateur enregistré : la conversation existe (son identifiant, pour une
    /// nouvelle) ; `message` est ce message tel qu'enregistré (noms des images compris), absent
    /// pour « Régénérer ».
    Started {
        conversation: ConversationSummary,
        message: Option<ChatMessage>,
    },
    /// Morceau de la réponse ou de la réflexion (l'un des deux peut être vide).
    Delta { content: String, thinking: String },
    /// Réponse enregistrée : finie (avec ses statistiques), arrêtée (`interrupted`) ou en échec
    /// (`error`).
    Done { message: ChatMessage },
}

/// Réponses en cours, par conversation, et moyen de les arrêter.
#[derive(Debug, Default)]
pub struct Generations {
    running: Mutex<HashMap<String, Arc<Notify>>>,
}

impl Generations {
    /// Enregistre une réponse en cours ; `None` s'il y en a déjà une dans cette conversation.
    fn start(&self, id: &str) -> Option<Running<'_>> {
        let mut running = self.lock();
        if running.contains_key(id) {
            return None;
        }
        let cancel = Arc::new(Notify::new());
        running.insert(id.to_string(), Arc::clone(&cancel));
        Some(Running {
            generations: self,
            id: id.to_string(),
            cancel,
        })
    }

    /// Arrête la réponse en cours dans cette conversation ; `false` s'il n'y en a pas.
    pub fn stop(&self, id: &str) -> bool {
        match self.lock().get(id) {
            Some(cancel) => {
                // Gardé si la réponse n'attend pas encore : elle s'arrêtera dès qu'elle attendra.
                cancel.notify_one();
                true
            }
            None => false,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<Notify>>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Réponse en cours ; retirée des réponses en cours à la destruction.
struct Running<'a> {
    generations: &'a Generations,
    id: String,
    cancel: Arc<Notify>,
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.generations.lock().remove(&self.id);
    }
}

/// Envoie un message (ou régénère la dernière réponse) et transmet la réponse à `emit`.
/// `vision` : le modèle accepte-t-il des images (`None` : inconnu, elles sont envoyées) ?
///
/// Rejeté avant tout événement : requête invalide, images refusées, conversation introuvable
/// ou illisible, réponse déjà en cours, enregistrement impossible. Une erreur d'Ollama n'est
/// pas un rejet : elle arrive dans `done`, avec la réponse partielle. Un échec de
/// l'enregistrement final rejette après `done`, sauf si la conversation a été supprimée
/// entre-temps.
pub async fn send(
    client: &OllamaClient,
    store: &ConversationStore,
    generations: &Generations,
    vision: Option<bool>,
    request: ChatSend,
    mut emit: impl FnMut(ChatEvent),
) -> CmdResult<()> {
    let model = request.model.trim().to_string();
    if model.is_empty() {
        return Err(CommandError::msg(
            "Aucun modèle choisi : en choisir un dans la liste",
        ));
    }
    let images =
        images::decode_all(&request.images).map_err(|e| CommandError::msg(e.to_string()))?;
    if !images.is_empty() {
        if request.content.is_none() {
            return Err(CommandError::msg(
                "Images jointes sans message : les joindre à un nouveau message",
            ));
        }
        if vision == Some(false) {
            return Err(CommandError::msg(format!(
                "Le modèle {model} n'accepte pas d'images : retirer les images, ou choisir un \
                 modèle avec la vision"
            )));
        }
    }
    if images.is_empty()
        && request
            .content
            .as_deref()
            .is_some_and(|c| c.trim().is_empty())
    {
        return Err(CommandError::msg(
            "Message vide : écrire quelque chose avant d'envoyer",
        ));
    }
    let now = Utc::now();
    let (conversation, user, running) = match request.conversation_id {
        None => {
            let content = request.content.ok_or_else(nothing_to_regenerate)?;
            let conversation = store.create_with_images(
                Conversation {
                    id: String::new(),
                    title: auto_title(&content),
                    model: model.clone(),
                    instructions: clean_instructions(request.instructions.as_deref()),
                    think: request.think,
                    created_at: now,
                    updated_at: now,
                    messages: vec![ChatMessage::user(content, now)],
                },
                &images,
            )?;
            let running = generations.start(&conversation.id).ok_or_else(busy)?;
            let user = conversation.messages.last().cloned();
            (conversation, user, running)
        }
        Some(id) => {
            let running = generations.start(&id).ok_or_else(busy)?;
            let names = if images.is_empty() {
                Vec::new()
            } else {
                store.save_images(&id, &images, now)?
            };
            let updated = store.update(&id, |c| {
                let user = match request.content {
                    Some(content) => {
                        let mut user = ChatMessage::user(content, now);
                        user.images = names.clone();
                        c.messages.push(user.clone());
                        Some(user)
                    }
                    None => {
                        if c.messages
                            .last()
                            .is_some_and(|m| m.role == ChatRole::Assistant)
                        {
                            c.messages.pop();
                        }
                        if c.messages.last().is_none_or(|m| m.role != ChatRole::User) {
                            return Err(nothing_to_regenerate());
                        }
                        None
                    }
                };
                c.model = model.clone();
                if request.think.is_some() {
                    c.think = request.think;
                }
                c.updated_at = now;
                Ok((c.clone(), user))
            });
            let (conversation, user) = updated.inspect_err(|_| store.remove_images(&id, &names))?;
            (conversation, user, running)
        }
    };
    emit(ChatEvent::Started {
        conversation: conversation.summary(),
        message: user,
    });

    let messages = ollama_messages(store, &conversation, vision != Some(false));
    let chat_request = ChatRequest {
        model: &model,
        messages: &messages,
        stream: true,
        think: request.think,
        keep_alive: request.keep_alive.as_ollama(),
        options: ChatOptions::default(),
    };
    let (mut content, mut thinking) = (String::new(), String::new());
    // Réflexion : de son premier morceau au premier mot de la réponse.
    let (mut thinking_from, mut thinking_to) = (None::<Instant>, None::<Instant>);
    let outcome = {
        let chat = client.chat(&chat_request, |chunk| {
            if chunk.content.is_empty() && chunk.thinking.is_empty() {
                return;
            }
            if !chunk.thinking.is_empty() && thinking_from.is_none() {
                thinking_from = Some(Instant::now());
            }
            if !chunk.content.is_empty() && thinking_from.is_some() && thinking_to.is_none() {
                thinking_to = Some(Instant::now());
            }
            content.push_str(&chunk.content);
            thinking.push_str(&chunk.thinking);
            emit(ChatEvent::Delta {
                content: chunk.content.clone(),
                thinking: chunk.thinking.clone(),
            });
        });
        // Arrêter abandonne la requête, ce qui ferme la connexion : Ollama arrête de générer.
        tokio::select! {
            result = chat => Some(result),
            () = running.cancel.notified() => None,
        }
    };

    let mut message = ChatMessage::assistant(content, &model, Utc::now());
    message.thinking = (!thinking.is_empty()).then_some(thinking);
    message.thinking_ms = thinking_from.map(|from| {
        let to = thinking_to.unwrap_or_else(Instant::now);
        u64::try_from(to.duration_since(from).as_millis()).unwrap_or(u64::MAX)
    });
    match outcome {
        Some(Ok(done)) => {
            let mut stats = ChatStats::from(done);
            stats.context_length = loaded_context(client, &model).await;
            message.stats = Some(stats);
        }
        Some(Err(e)) => message.error = Some(chat_error(&e, &model)),
        None => message.interrupted = true,
    }
    let saved = store.update(&conversation.id, |c| {
        c.messages.push(message.clone());
        c.updated_at = message.created_at;
        Ok::<_, StoreError>(())
    });
    // Plus de réponse en cours avant `done` : l'interface peut renvoyer aussitôt.
    drop(running);
    emit(ChatEvent::Done { message });
    match saved {
        // Supprimée pendant la réponse : rien à enregistrer.
        Ok(()) | Err(StoreError::NotFound) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Conversation envoyée à Ollama : les instructions, puis les messages, avec leurs images en
/// base64 si `with_images`. Les réponses en échec avant leur premier mot sont sautées ; la
/// réflexion n'est pas renvoyée ; une image illisible est sautée (journalisé).
fn ollama_messages(
    store: &ConversationStore,
    c: &Conversation,
    with_images: bool,
) -> Vec<gram_ollama::ChatMessage> {
    use gram_ollama::ChatRole as Role;
    let instructions = c
        .instructions
        .as_deref()
        .map(str::trim)
        .filter(|i| !i.is_empty())
        .map(|i| gram_ollama::ChatMessage::new(Role::System, i));
    let messages = c.messages.iter().filter_map(|m| match m.role {
        ChatRole::User => {
            let mut message = gram_ollama::ChatMessage::new(Role::User, &m.content);
            if with_images {
                message.images = m
                    .images
                    .iter()
                    .filter_map(|name| match store.read_image(&c.id, name) {
                        Ok(image) => Some(images::to_base64(&image.bytes)),
                        Err(e) => {
                            tracing::warn!(conversation = %c.id, name, "image non envoyée : {e}");
                            None
                        }
                    })
                    .collect();
            }
            Some(message)
        }
        ChatRole::Assistant if !m.content.is_empty() => {
            Some(gram_ollama::ChatMessage::new(Role::Assistant, &m.content))
        }
        ChatRole::Assistant => None,
    });
    instructions.into_iter().chain(messages).collect()
}

/// Instructions saisies : espaces des bords retirés ; vides, aucune.
pub fn clean_instructions(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

/// Contexte du modèle chargé, d'après `/api/ps` ; `None` si Ollama ne le donne pas ou tarde
/// (la réponse est déjà là : la jauge prendra le contexte du modèle).
async fn loaded_context(client: &OllamaClient, model: &str) -> Option<u32> {
    let ps = tokio::time::timeout(PS_TIMEOUT, client.ps())
        .await
        .ok()?
        .inspect_err(|e| tracing::debug!("/api/ps après la réponse : {e}"))
        .ok()?;
    ps.iter().find(|e| e.is(model))?.loaded().context_length
}

/// Message d'une erreur d'Ollama dans le fil, qui dit quoi faire.
fn chat_error(e: &OllamaError, model: &str) -> String {
    match e {
        OllamaError::Api { status, message } if is_model_not_found(*status, message) => format!(
            "Le modèle {model} n'est plus dans Ollama : en choisir un autre, puis Réessayer"
        ),
        OllamaError::Api { message, .. } if is_image_unsupported(message) => format!(
            "Le modèle {model} n'accepte pas d'images : en choisir un avec la vision, puis \
             Réessayer"
        ),
        _ => capitalize(&e.to_string()),
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn busy() -> CommandError {
    CommandError::msg("Une réponse est déjà en cours dans cette conversation : l'arrêter d'abord")
}

fn nothing_to_regenerate() -> CommandError {
    CommandError::msg("Rien à régénérer : la conversation n'a pas de message à qui répondre")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversations::tests::{TempDir, at, conversation};
    use gram_ollama::testing::{
        CHAT_DEFAULT, CHAT_NO_THINK_CONTENT, FakeOllama, Reply, standard_response,
    };
    use serde_json::json;
    use std::time::Duration;

    struct Setup {
        _dir: TempDir,
        fake: FakeOllama,
        client: OllamaClient,
        store: ConversationStore,
        generations: Generations,
    }

    fn setup(name: &str, fake: FakeOllama) -> Setup {
        let dir = TempDir::new(name);
        let store = ConversationStore::new(dir.0.join("conversations"));
        let client = OllamaClient::new(fake.url()).unwrap();
        Setup {
            _dir: dir,
            fake,
            client,
            store,
            generations: Generations::default(),
        }
    }

    impl Setup {
        async fn send(&self, request: ChatSend) -> (Vec<ChatEvent>, CmdResult<()>) {
            self.send_with(None, request).await
        }

        /// `vision` : le modèle accepte-t-il des images ?
        async fn send_with(
            &self,
            vision: Option<bool>,
            request: ChatSend,
        ) -> (Vec<ChatEvent>, CmdResult<()>) {
            let mut events = Vec::new();
            let result = send(
                &self.client,
                &self.store,
                &self.generations,
                vision,
                request,
                |e| events.push(e),
            )
            .await;
            (events, result)
        }

        /// Corps de la dernière requête `/api/chat` (une requête `/api/ps` suit chaque réponse).
        fn last_chat(&self) -> serde_json::Value {
            self.fake
                .requests()
                .iter()
                .rfind(|r| r.path == "/api/chat")
                .expect("requête /api/chat")
                .json()
        }
    }

    fn request(id: Option<&str>, content: Option<&str>) -> ChatSend {
        ChatSend {
            conversation_id: id.map(Into::into),
            model: "gram/qwen3-1.7b:q4_k_m".into(),
            content: content.map(Into::into),
            ..ChatSend::default()
        }
    }

    fn done(events: &[ChatEvent]) -> &ChatMessage {
        match events.last() {
            Some(ChatEvent::Done { message }) => message,
            other => panic!("{other:?}"),
        }
    }

    fn started(events: &[ChatEvent]) -> &ConversationSummary {
        match events.first() {
            Some(ChatEvent::Started { conversation, .. }) => conversation,
            other => panic!("{other:?}"),
        }
    }

    /// Réponse et réflexion transmises par les `delta`.
    fn streamed(events: &[ChatEvent]) -> (String, String) {
        events
            .iter()
            .fold((String::new(), String::new()), |(c, t), e| match e {
                ChatEvent::Delta { content, thinking } => (c + content, t + thinking),
                _ => (c, t),
            })
    }

    #[tokio::test]
    async fn nouvelle_conversation_puis_suite() {
        let s = setup("chat-suite", FakeOllama::standard());
        let (events, result) = s.send(request(None, Some("Combien font 12 × 3 ?"))).await;
        result.unwrap();
        let summary = started(&events).clone();
        assert_eq!(summary.title, "Combien font 12 × 3 ?");
        assert_eq!(streamed(&events).0, CHAT_NO_THINK_CONTENT);
        let answer = done(&events);
        assert_eq!(answer.content, CHAT_NO_THINK_CONTENT);
        assert_eq!(answer.model.as_deref(), Some("gram/qwen3-1.7b:q4_k_m"));
        assert_eq!(answer.stats.as_ref().unwrap().eval_count, 11);
        assert!(!answer.interrupted && answer.error.is_none() && answer.thinking.is_none());

        // Enregistrée : la question et la réponse.
        let saved = s.store.get(&summary.id).unwrap();
        assert_eq!(saved.messages.len(), 2);
        assert_eq!(&saved.messages[1], answer);
        assert_eq!(saved.updated_at, answer.created_at);

        // Suite, avec un autre modèle et des instructions : tout l'historique est renvoyé.
        s.store
            .update(&summary.id, |c| {
                c.instructions = Some("  Réponds en français.  ".into());
                Ok::<_, StoreError>(())
            })
            .unwrap();
        let (events, result) = s
            .send(ChatSend {
                model: "qwen3:8b".into(),
                ..request(Some(&summary.id), Some("Et 12 × 4 ?"))
            })
            .await;
        result.unwrap();
        assert_eq!(started(&events).model, "qwen3:8b");
        assert_eq!(done(&events).model.as_deref(), Some("qwen3:8b"));
        let body = s.last_chat();
        assert_eq!(body["model"], "qwen3:8b");
        assert_eq!(body["stream"], true);
        assert!(body.get("think").is_none() && body.get("options").is_none());
        assert_eq!(
            body["messages"],
            json!([
                { "role": "system", "content": "Réponds en français." },
                { "role": "user", "content": "Combien font 12 × 3 ?" },
                { "role": "assistant", "content": CHAT_NO_THINK_CONTENT },
                { "role": "user", "content": "Et 12 × 4 ?" },
            ])
        );
        let saved = s.store.get(&summary.id).unwrap();
        assert_eq!(saved.messages.len(), 4);
        assert_eq!(saved.model, "qwen3:8b");
    }

    #[tokio::test]
    async fn reflexion_gardee_a_part() {
        let fake = FakeOllama::streaming(|_| Reply::ndjson(CHAT_DEFAULT));
        let s = setup("chat-reflexion", fake);
        let (events, result) = s.send(request(None, Some("Bonjour !"))).await;
        result.unwrap();
        let (content, thinking) = streamed(&events);
        let answer = done(&events);
        assert_eq!(answer.content, "Bonjour ! Comment puis-je vous aider ?");
        assert_eq!(answer.content, content);
        assert_eq!(answer.thinking.as_deref(), Some(thinking.as_str()));
        assert!(thinking.starts_with("Okay, the user said"));
        // La dernière ligne (bilan, sans texte) ne donne pas de `delta`.
        let deltas = events
            .iter()
            .filter(|e| matches!(e, ChatEvent::Delta { .. }))
            .count();
        assert_eq!(deltas, 99);
        // La réflexion n'est pas renvoyée à Ollama.
        let (_, result) = s
            .send(request(Some(&started(&events).id), Some("Merci")))
            .await;
        result.unwrap();
        let body = s.last_chat();
        assert!(body["messages"][1].get("thinking").is_none(), "{body}");
    }

    #[tokio::test]
    async fn regenerer_et_reessayer() {
        let s = setup("chat-regenerer", FakeOllama::standard());
        let (events, _) = s.send(request(None, Some("Bonjour"))).await;
        let id = started(&events).id.clone();
        // Régénérer : la dernière réponse est remplacée.
        let (events, result) = s.send(request(Some(&id), None)).await;
        result.unwrap();
        assert_eq!(done(&events).content, CHAT_NO_THINK_CONTENT);
        let saved = s.store.get(&id).unwrap();
        assert_eq!(saved.messages.len(), 2);
        let body = s.last_chat();
        assert_eq!(
            body["messages"],
            json!([{ "role": "user", "content": "Bonjour" }])
        );

        // Réessayer après un échec : la réponse en échec est retirée de même.
        s.store
            .update(&id, |c| {
                c.messages.pop();
                let mut failed = ChatMessage::assistant(String::new(), "x", at(10, 0));
                failed.error = Some("Ollama ne répond pas".into());
                c.messages.push(failed);
                Ok::<_, StoreError>(())
            })
            .unwrap();
        let (_, result) = s.send(request(Some(&id), None)).await;
        result.unwrap();
        let saved = s.store.get(&id).unwrap();
        assert_eq!(saved.messages.len(), 2);
        assert!(saved.messages[1].error.is_none());

        // Rien à régénérer.
        let (events, result) = s.send(request(None, None)).await;
        assert!(events.is_empty());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .starts_with("Rien à régénérer")
        );
        let empty = s
            .store
            .create(Conversation {
                messages: Vec::new(),
                ..conversation("x", at(11, 0))
            })
            .unwrap();
        let (events, result) = s.send(request(Some(&empty.id), None)).await;
        assert!(events.is_empty() && result.is_err());
    }

    #[tokio::test]
    async fn arret_en_cours_de_reponse() {
        let lines: Vec<String> = CHAT_DEFAULT.lines().map(|l| format!("{l}\n")).collect();
        let (first, rest) = (lines[..3].concat(), lines[3..].concat());
        let fake = FakeOllama::streaming(move |_| {
            Reply::ndjson(&first)
                .pause(Duration::from_secs(5))
                .lines(&rest)
        });
        let s = setup("chat-arret", fake);
        let events = Mutex::new(Vec::new());
        let start = std::time::Instant::now();
        let sending = send(
            &s.client,
            &s.store,
            &s.generations,
            None,
            request(None, Some("Bonjour")),
            |e| events.lock().unwrap().push(e),
        );
        let stopping = async {
            // Arrêt dès le premier morceau.
            loop {
                let id = {
                    let events = events.lock().unwrap();
                    events
                        .iter()
                        .any(|e| matches!(e, ChatEvent::Delta { .. }))
                        .then(|| started(&events).id.clone())
                };
                if let Some(id) = id {
                    assert!(s.generations.stop(&id));
                    return id;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        let (result, id) = tokio::join!(sending, stopping);
        result.unwrap();
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "arrêtée sans attendre la fin"
        );
        let events = events.into_inner().unwrap();
        let answer = done(&events);
        assert!(answer.interrupted);
        assert_eq!(answer.thinking.as_deref(), Some("Okay, the"));
        assert!(answer.stats.is_none() && answer.error.is_none());
        assert_eq!(&s.store.get(&id).unwrap().messages[1], answer);
        assert!(!s.generations.stop(&id), "plus rien en cours");
    }

    #[tokio::test]
    async fn une_reponse_a_la_fois_par_conversation() {
        let s = setup("chat-occupee", FakeOllama::standard());
        let c = s.store.create(conversation("Bonjour", at(9, 0))).unwrap();
        let running = s.generations.start(&c.id).unwrap();
        let (events, result) = s.send(request(Some(&c.id), Some("Encore"))).await;
        assert!(events.is_empty());
        assert!(result.unwrap_err().to_string().contains("déjà en cours"));
        assert_eq!(s.store.get(&c.id).unwrap().messages.len(), 1, "rien ajouté");
        drop(running);
        assert!(s.send(request(Some(&c.id), None)).await.1.is_ok());
    }

    #[tokio::test]
    async fn erreurs_dans_le_fil() {
        // Erreur au milieu du flux : le début est gardé, avec le message.
        let head: String = CHAT_DEFAULT
            .lines()
            .take(3)
            .map(|l| format!("{l}\n"))
            .collect();
        let fake = FakeOllama::streaming(move |_| {
            Reply::ndjson(&head).lines(r#"{"error":"llama runner process has terminated"}"#)
        });
        let s = setup("chat-erreur", fake);
        let (events, result) = s.send(request(None, Some("Bonjour"))).await;
        result.unwrap();
        let answer = done(&events);
        assert_eq!(answer.thinking.as_deref(), Some("Okay, the"));
        assert_eq!(
            answer.error.as_deref(),
            Some("Ollama a interrompu l'opération : llama runner process has terminated")
        );
        assert_eq!(s.store.list().len(), 1);

        // Ollama arrêté au milieu de la réponse : connexion coupée après quelques lignes.
        let head: String = CHAT_DEFAULT
            .lines()
            .take(5)
            .map(|l| format!("{l}\n"))
            .collect();
        let fake = FakeOllama::streaming(move |_| Reply::ndjson(&head).abort());
        let s = setup("chat-coupee", fake);
        let (events, _) = s.send(request(None, Some("Bonjour"))).await;
        assert_eq!(
            done(&events).error.as_deref(),
            Some("Connexion avec Ollama coupée (réponse) : le relancer, puis réessayer")
        );

        // Modèle disparu d'Ollama.
        let fake = FakeOllama::start(|r| match r.path.as_str() {
            "/api/chat" => (
                404,
                r#"{"error":"model 'gram/qwen3-1.7b:q4_k_m' not found"}"#.into(),
            ),
            _ => standard_response(r),
        });
        let s = setup("chat-disparu", fake);
        let (events, result) = s.send(request(None, Some("Bonjour"))).await;
        result.unwrap();
        let answer = done(&events);
        assert_eq!(
            answer.error.as_deref(),
            Some(
                "Le modèle gram/qwen3-1.7b:q4_k_m n'est plus dans Ollama : en choisir un autre, \
                 puis Réessayer"
            )
        );
        assert_eq!(answer.content, "");
        // La réponse vide en échec n'est pas renvoyée ensuite.
        let id = started(&events).id.clone();
        let saved = s.store.get(&id).unwrap();
        assert_eq!(ollama_messages(&s.store, &saved, true).len(), 1);

        // Ollama arrêté : le message de l'utilisateur est gardé.
        let closed = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let s = Setup {
            client: OllamaClient::for_addr(closed).unwrap(),
            ..setup("chat-arrete", FakeOllama::standard())
        };
        let (events, result) = s.send(request(None, Some("Bonjour"))).await;
        result.unwrap();
        assert!(
            done(&events)
                .error
                .as_deref()
                .unwrap()
                .starts_with("Ollama ne répond pas sur"),
        );
        assert_eq!(s.store.get(&started(&events).id).unwrap().messages.len(), 2);
    }

    #[tokio::test]
    async fn requetes_invalides() {
        let s = setup("chat-invalides", FakeOllama::standard());
        let (events, result) = s
            .send(ChatSend {
                model: " ".into(),
                ..request(None, Some("Bonjour"))
            })
            .await;
        assert!(events.is_empty());
        assert!(result.unwrap_err().to_string().starts_with("Aucun modèle"));
        let (_, result) = s.send(request(None, Some(" \n "))).await;
        assert!(result.unwrap_err().to_string().starts_with("Message vide"));
        let (_, result) = s.send(request(Some("../x"), Some("Bonjour"))).await;
        assert!(
            result
                .unwrap_err()
                .to_string()
                .starts_with("Conversation introuvable")
        );
        assert!(s.store.list().is_empty());
        assert!(s.fake.requests().is_empty(), "Ollama pas interrogé");
    }

    #[tokio::test]
    async fn supprimee_pendant_la_reponse() {
        let lines: Vec<String> = CHAT_DEFAULT.lines().map(|l| format!("{l}\n")).collect();
        let (first, rest) = (lines[..3].concat(), lines[3..].concat());
        let fake = FakeOllama::streaming(move |_| {
            Reply::ndjson(&first)
                .pause(Duration::from_millis(300))
                .lines(&rest)
        });
        let s = setup("chat-supprimee", fake);
        let events = Mutex::new(Vec::new());
        let sending = send(
            &s.client,
            &s.store,
            &s.generations,
            None,
            request(None, Some("Bonjour")),
            |e| events.lock().unwrap().push(e),
        );
        let deleting = async {
            loop {
                let id = {
                    let events = events.lock().unwrap();
                    (!events.is_empty()).then(|| started(&events).id.clone())
                };
                if let Some(id) = id {
                    assert!(s.store.delete(&id).unwrap());
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        let (result, ()) = tokio::join!(sending, deleting);
        result.unwrap();
        assert!(s.store.list().is_empty(), "pas recréée");
        assert!(!done(&events.into_inner().unwrap()).content.is_empty());
    }

    fn with_images(request: ChatSend, images: &[&[u8]]) -> ChatSend {
        ChatSend {
            images: images.iter().map(|b| images::to_base64(b)).collect(),
            ..request
        }
    }

    /// Fichiers du dossier des images d'une conversation.
    fn image_files(s: &Setup, id: &str) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(s.store.dir().join(id))
            .map(|d| {
                d.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    #[tokio::test]
    async fn images_enregistrees_puis_envoyees() {
        use crate::images::tests::{JPEG, PNG};
        let s = setup("chat-images", FakeOllama::standard());
        // Image seule, sans texte : acceptée.
        let (events, result) = s
            .send_with(
                Some(true),
                with_images(request(None, Some("")), &[PNG, JPEG]),
            )
            .await;
        result.unwrap();
        let ChatEvent::Started {
            conversation,
            message: Some(user),
        } = &events[0]
        else {
            panic!("{:?}", events[0]);
        };
        let id = conversation.id.clone();
        assert_eq!(conversation.title, "Nouvelle conversation");
        assert_eq!(user.images.len(), 2);
        assert!(user.images[0].ends_with("-1.png") && user.images[1].ends_with("-2.jpg"));
        assert_eq!(image_files(&s, &id), user.images);
        assert_eq!(s.store.get(&id).unwrap().messages[0].images, user.images);
        // Envoyées en base64, dans l'ordre.
        let body = s.last_chat();
        assert_eq!(
            body["messages"][0]["images"],
            json!([images::to_base64(PNG), images::to_base64(JPEG)])
        );

        // Suite avec un modèle sans vision : les images de l'historique ne sont pas envoyées.
        let (_, result) = s
            .send_with(Some(false), request(Some(&id), Some("Et maintenant ?")))
            .await;
        result.unwrap();
        let body = s.last_chat();
        assert!(body["messages"][0].get("images").is_none(), "{body}");
        // Une nouvelle image pour ce modèle : refusée avant tout, rien d'enregistré.
        let chats = s.fake.requests().len();
        let (events, result) = s
            .send_with(
                Some(false),
                with_images(request(Some(&id), Some("Et ça ?")), &[PNG]),
            )
            .await;
        assert!(events.is_empty());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Le modèle gram/qwen3-1.7b:q4_k_m n'accepte pas d'images : retirer les images, ou \
             choisir un modèle avec la vision"
        );
        assert_eq!(s.fake.requests().len(), chats, "Ollama pas interrogé");
        assert_eq!(image_files(&s, &id).len(), 2);
        assert_eq!(s.store.get(&id).unwrap().messages.len(), 4);

        // Vision inconnue (capacités illisibles) : les images partent, Ollama tranchera.
        let (_, result) = s
            .send_with(
                None,
                with_images(request(Some(&id), Some("Et ça ?")), &[PNG]),
            )
            .await;
        result.unwrap();
        let body = s.last_chat();
        let sent: Vec<usize> = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.get("images").map_or(0, |i| i.as_array().unwrap().len()))
            .collect();
        assert_eq!(sent, [2, 0, 0, 0, 1]);
        assert_eq!(image_files(&s, &id).len(), 3);

        // Image disparue du dossier : sautée, la conversation part quand même.
        let first = s.store.get(&id).unwrap().messages[0].images[0].clone();
        std::fs::remove_file(s.store.dir().join(&id).join(first)).unwrap();
        let saved = s.store.get(&id).unwrap();
        assert_eq!(ollama_messages(&s.store, &saved, true)[0].images.len(), 1);

        // Supprimer la conversation supprime ses images.
        assert!(s.store.delete(&id).unwrap());
        assert!(image_files(&s, &id).is_empty());
    }

    #[tokio::test]
    async fn images_refusees() {
        use crate::images::tests::PNG;
        let s = setup("chat-images-refus", FakeOllama::standard());
        let refused = |result: CmdResult<()>| result.unwrap_err().to_string();
        let (_, result) = s
            .send(with_images(request(None, Some("x")), &[b"GIF89a"]))
            .await;
        assert!(refused(result).contains("format non pris en charge"));
        let (_, result) = s
            .send(with_images(request(None, Some("x")), &[PNG; 5]))
            .await;
        assert!(refused(result).contains("4 au plus"));
        let (_, result) = s.send(with_images(request(None, None), &[PNG])).await;
        assert!(refused(result).starts_with("Images jointes sans message"));
        assert!(s.store.list().is_empty());
        assert!(s.fake.requests().is_empty());

        // Ollama refuse quand même (capacités inconnues, modèle sans vision).
        let fake = FakeOllama::start(|r| match r.path.as_str() {
            "/api/chat" => (
                500,
                include_str!("../../../tests/fixtures/ollama/chat-image-unsupported.json").into(),
            ),
            _ => standard_response(r),
        });
        let s = setup("chat-images-ollama", fake);
        let (events, result) = s.send(with_images(request(None, Some("x")), &[PNG])).await;
        result.unwrap();
        assert_eq!(
            done(&events).error.as_deref(),
            Some(
                "Le modèle gram/qwen3-1.7b:q4_k_m n'accepte pas d'images : en choisir un avec la \
                 vision, puis Réessayer"
            )
        );
    }

    #[tokio::test]
    async fn reglages_envoyes_et_retenus() {
        let s = setup("chat-reglages", FakeOllama::standard());
        // Nouvelle conversation : instructions (bords retirés), réflexion, mémoire.
        let (events, result) = s
            .send(ChatSend {
                think: Some(false),
                keep_alive: KeepAlive::Forever,
                instructions: Some("  Réponds en français.\n".into()),
                ..request(None, Some("Bonjour"))
            })
            .await;
        result.unwrap();
        let id = started(&events).id.clone();
        let body = s.last_chat();
        assert_eq!(body["think"], false);
        assert_eq!(body["keep_alive"], "-1m");
        assert_eq!(
            body["messages"],
            json!([
                { "role": "system", "content": "Réponds en français." },
                { "role": "user", "content": "Bonjour" },
            ])
        );
        let saved = s.store.get(&id).unwrap();
        assert_eq!(saved.instructions.as_deref(), Some("Réponds en français."));
        assert_eq!(saved.think, Some(false));

        // Conversation existante : les instructions de la requête sont ignorées ; la réflexion
        // n'est changée que si la requête la donne (modèle qui sait réfléchir).
        let (_, result) = s
            .send(ChatSend {
                keep_alive: KeepAlive::HalfHour,
                instructions: Some("Autre chose".into()),
                ..request(Some(&id), Some("Encore"))
            })
            .await;
        result.unwrap();
        let body = s.last_chat();
        assert!(body.get("think").is_none());
        assert_eq!(body["keep_alive"], "30m");
        assert_eq!(body["messages"][0]["content"], "Réponds en français.");
        assert_eq!(s.store.get(&id).unwrap().think, Some(false));
        let (_, result) = s
            .send(ChatSend {
                think: Some(true),
                ..request(Some(&id), Some("Réfléchis"))
            })
            .await;
        result.unwrap();
        assert!(
            s.last_chat().get("keep_alive").is_none(),
            "réglage d'Ollama"
        );
        assert_eq!(s.store.get(&id).unwrap().think, Some(true));
        // Instructions vides : aucune.
        assert_eq!(clean_instructions(Some(" \n ")), None);
        assert_eq!(clean_instructions(None), None);
    }

    #[tokio::test]
    async fn duree_de_la_reflexion() {
        // Réflexion, pause, puis réponse.
        let lines: Vec<String> = CHAT_DEFAULT.lines().map(|l| format!("{l}\n")).collect();
        let first_content = lines
            .iter()
            .position(|l| l.contains(r#""content":"Bon"#))
            .unwrap();
        let (thinking, rest) = (
            lines[..first_content].concat(),
            lines[first_content..].concat(),
        );
        let fake = FakeOllama::streaming(move |r| match r.path.as_str() {
            "/api/chat" => Reply::ndjson(&thinking)
                .pause(Duration::from_millis(300))
                .lines(&rest),
            _ => standard_response(r).into(),
        });
        let s = setup("chat-duree", fake);
        let (events, result) = s.send(request(None, Some("Bonjour"))).await;
        result.unwrap();
        let ms = done(&events).thinking_ms.unwrap();
        assert!((250..3000).contains(&ms), "{ms} ms");

        // Sans réflexion : pas de durée.
        let s = setup("chat-sans-duree", FakeOllama::standard());
        let (events, _) = s.send(request(None, Some("Bonjour"))).await;
        assert_eq!(done(&events).thinking_ms, None);
    }

    #[tokio::test]
    async fn contexte_du_modele_charge() {
        let ps = |name: &str, ctx: u32| {
            json!({ "models": [{ "name": name, "model": name, "size": 1,
                "size_vram": 1, "context_length": ctx }] })
            .to_string()
        };
        let loaded = ps("gram/qwen3-1.7b:q4_k_m", 16384);
        let fake = FakeOllama::start(move |r| match r.path.as_str() {
            "/api/ps" => (200, loaded.clone()),
            _ => standard_response(r),
        });
        let s = setup("chat-contexte", fake);
        let (events, _) = s.send(request(None, Some("Bonjour"))).await;
        let stats = done(&events).stats.clone().unwrap();
        assert_eq!(stats.context_length, Some(16384));
        assert_eq!((stats.prompt_eval_count, stats.eval_count), (29, 11));

        // Autre modèle chargé, ou `/api/ps` en échec : inconnu.
        let s = setup("chat-contexte-autre", FakeOllama::standard());
        let (events, _) = s.send(request(None, Some("Bonjour"))).await;
        assert_eq!(done(&events).stats.as_ref().unwrap().context_length, None);
        let fake = FakeOllama::start(|r| match r.path.as_str() {
            "/api/ps" => (500, r#"{"error":"x"}"#.into()),
            _ => standard_response(r),
        });
        let s = setup("chat-contexte-echec", fake);
        let (events, result) = s.send(request(None, Some("Bonjour"))).await;
        result.unwrap();
        assert!(done(&events).error.is_none());
        assert_eq!(done(&events).stats.as_ref().unwrap().context_length, None);
    }

    #[test]
    fn memoire_du_modele() {
        assert_eq!(KeepAlive::default(), KeepAlive::Ollama);
        assert_eq!(KeepAlive::Ollama.as_ollama(), None);
        assert_eq!(KeepAlive::HalfHour.as_ollama(), Some("30m"));
        assert_eq!(KeepAlive::Forever.as_ollama(), Some("-1m"));
        let request: ChatSend = serde_json::from_value(
            json!({ "conversationId": null, "model": "m", "content": "x",
                "images": [], "think": true, "keepAlive": "halfHour", "instructions": null }),
        )
        .unwrap();
        assert_eq!(request.keep_alive, KeepAlive::HalfHour);
        // Champs récents absents : valeurs par défaut.
        let request: ChatSend =
            serde_json::from_value(json!({ "conversationId": null, "model": "m", "content": "x" }))
                .unwrap();
        assert_eq!(request.keep_alive, KeepAlive::Ollama);
        assert!(request.images.is_empty() && request.think.is_none());
    }
}
