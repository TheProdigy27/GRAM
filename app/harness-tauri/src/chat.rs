//! Discussion : envoyer un message (ou régénérer la dernière réponse), recevoir la réponse en
//! flux, l'arrêter.
//!
//! L'API d'Ollama est sans état : toute la conversation lui est envoyée à chaque fois. Le
//! message de l'utilisateur est enregistré avant d'interroger Ollama, la réponse une fois
//! finie, arrêtée (début gardé, marqué « interrompue ») ou en échec (début gardé, avec le
//! message d'erreur) : l'interface la montre avec « Réessayer ».

use crate::conversations::{
    ChatMessage, ChatRole, Conversation, ConversationStore, ConversationSummary, StoreError,
    auto_title,
};
use crate::error::{CmdResult, CommandError};
use chrono::Utc;
use gram_ollama::api::is_model_not_found;
use gram_ollama::{ChatOptions, ChatRequest, OllamaClient, OllamaError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::Notify;
use ts_rs::TS;

/// Requête de `chat_send`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
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
}

/// Événements d'une réponse, dans l'ordre : `started`, des `delta`, puis `done`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export)]
pub enum ChatEvent {
    /// Message de l'utilisateur enregistré : la conversation existe (son identifiant, pour une
    /// nouvelle).
    Started { conversation: ConversationSummary },
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
///
/// Rejeté avant tout événement : requête invalide, conversation introuvable ou illisible,
/// réponse déjà en cours, enregistrement impossible. Une erreur d'Ollama n'est pas un rejet :
/// elle arrive dans `done`, avec la réponse partielle. Un échec de l'enregistrement final
/// rejette après `done`, sauf si la conversation a été supprimée entre-temps.
pub async fn send(
    client: &OllamaClient,
    store: &ConversationStore,
    generations: &Generations,
    request: ChatSend,
    mut emit: impl FnMut(ChatEvent),
) -> CmdResult<()> {
    let model = request.model.trim().to_string();
    if model.is_empty() {
        return Err(CommandError::msg(
            "Aucun modèle choisi : en choisir un dans la liste",
        ));
    }
    if request
        .content
        .as_deref()
        .is_some_and(|c| c.trim().is_empty())
    {
        return Err(CommandError::msg(
            "Message vide : écrire quelque chose avant d'envoyer",
        ));
    }
    let now = Utc::now();
    let (conversation, running) = match request.conversation_id {
        None => {
            let content = request.content.ok_or_else(nothing_to_regenerate)?;
            let conversation = store.create(Conversation {
                id: String::new(),
                title: auto_title(&content),
                model: model.clone(),
                instructions: None,
                created_at: now,
                updated_at: now,
                messages: vec![ChatMessage::user(content, now)],
            })?;
            let running = generations.start(&conversation.id).ok_or_else(busy)?;
            (conversation, running)
        }
        Some(id) => {
            let running = generations.start(&id).ok_or_else(busy)?;
            let conversation = store.update(&id, |c| {
                match request.content {
                    Some(content) => c.messages.push(ChatMessage::user(content, now)),
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
                    }
                }
                c.model = model.clone();
                c.updated_at = now;
                Ok(c.clone())
            })?;
            (conversation, running)
        }
    };
    emit(ChatEvent::Started {
        conversation: conversation.summary(),
    });

    let messages = ollama_messages(&conversation);
    let chat_request = ChatRequest {
        model: &model,
        messages: &messages,
        stream: true,
        think: None,
        keep_alive: None,
        options: ChatOptions::default(),
    };
    let (mut content, mut thinking) = (String::new(), String::new());
    let outcome = {
        let chat = client.chat(&chat_request, |chunk| {
            if chunk.content.is_empty() && chunk.thinking.is_empty() {
                return;
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
    match outcome {
        Some(Ok(done)) => message.stats = Some(done.into()),
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

/// Conversation envoyée à Ollama : les instructions, puis les messages. Les réponses en échec
/// avant leur premier mot sont sautées ; la réflexion n'est pas renvoyée.
fn ollama_messages(c: &Conversation) -> Vec<gram_ollama::ChatMessage> {
    use gram_ollama::ChatRole as Role;
    let instructions = c
        .instructions
        .as_deref()
        .map(str::trim)
        .filter(|i| !i.is_empty())
        .map(|i| gram_ollama::ChatMessage::new(Role::System, i));
    let messages = c.messages.iter().filter_map(|m| match m.role {
        ChatRole::User => Some(gram_ollama::ChatMessage::new(Role::User, &m.content)),
        ChatRole::Assistant if !m.content.is_empty() => {
            Some(gram_ollama::ChatMessage::new(Role::Assistant, &m.content))
        }
        ChatRole::Assistant => None,
    });
    instructions.into_iter().chain(messages).collect()
}

/// Message d'une erreur d'Ollama dans le fil, qui dit quoi faire.
fn chat_error(e: &OllamaError, model: &str) -> String {
    match e {
        OllamaError::Api { status, message } if is_model_not_found(*status, message) => format!(
            "Le modèle {model} n'est plus dans Ollama : en choisir un autre, puis Réessayer"
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
            let mut events = Vec::new();
            let result = send(&self.client, &self.store, &self.generations, request, |e| {
                events.push(e)
            })
            .await;
            (events, result)
        }
    }

    fn request(id: Option<&str>, content: Option<&str>) -> ChatSend {
        ChatSend {
            conversation_id: id.map(Into::into),
            model: "gram/qwen3-1.7b:q4_k_m".into(),
            content: content.map(Into::into),
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
            Some(ChatEvent::Started { conversation }) => conversation,
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
        let body = s.fake.requests().last().unwrap().json();
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
        let body = s.fake.requests().last().unwrap().json();
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
        let body = s.fake.requests().last().unwrap().json();
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
        assert_eq!(ollama_messages(&saved).len(), 1);

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
}
