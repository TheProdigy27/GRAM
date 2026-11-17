//! Commandes Tauri de GRAM Harness (`#[tauri::command]`).
//!
//! Depuis l'interface (ou la console du WebView en développement) :
//! `window.__TAURI_INTERNALS__.invoke('list_models')`. Les arguments sont en camelCase ; une
//! erreur rejette la promesse avec un message en français.

use crate::chat::{self, ChatEvent, ChatSend, clean_instructions};
use crate::conversations::{Conversation, ConversationSummary, StoreError, clean_title};
use crate::error::{CmdResult, CommandError};
use crate::images;
use crate::models::{self, ModelList, OllamaState};
use crate::state::HarnessState;
use gram_ollama::OllamaError;
use tauri::ipc::{Channel, Invoke};
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_opener::OpenerExt;

/// Page de téléchargement d'Ollama (seule adresse que le harness ouvre en phase 4).
pub const OLLAMA_DOWNLOAD_URL: &str = "https://ollama.com/download";

/// Toutes les commandes, pour le runtime réel comme pour le runtime simulé des tests.
pub fn handler<R: Runtime>() -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        list_models,
        open_ollama_download,
        chat_send,
        chat_stop,
        list_conversations,
        get_conversation,
        rename_conversation,
        set_instructions,
        set_think,
        delete_conversation,
        get_image,
        take_opened_model,
    ]
}

/// Modèles avec lesquels discuter et état d'Ollama. Ollama arrêté ou absent n'est pas une
/// erreur : la liste est vide et `ollama` dit quoi faire.
#[tauri::command]
pub async fn list_models(state: State<'_, HarnessState>) -> CmdResult<ModelList> {
    let client = state.require_ollama()?;
    let manifest = state.manifest();
    match models::list_models(client, &manifest, state.shows()).await {
        Ok(models) => Ok(ModelList {
            ollama: OllamaState::Running,
            models,
        }),
        Err(OllamaError::Unreachable(_)) => {
            let ollama = if state.ollama_installed().await {
                OllamaState::Stopped
            } else {
                OllamaState::Absent
            };
            Ok(ModelList {
                ollama,
                models: Vec::new(),
            })
        }
        Err(e) => Err(e.into()),
    }
}

/// Ouvre la page de téléchargement d'Ollama dans le navigateur du système. L'adresse est fixe :
/// la WebView n'a aucune permission d'ouvrir une adresse.
#[tauri::command]
pub fn open_ollama_download<R: Runtime>(app: AppHandle<R>) -> CmdResult<()> {
    app.opener()
        .open_url(OLLAMA_DOWNLOAD_URL, None::<&str>)
        .map_err(|e| {
            CommandError::msg(format!(
                "Navigateur non ouvert ({e}) : aller sur {OLLAMA_DOWNLOAD_URL}"
            ))
        })
}

/// Envoie un message (ou, sans `content`, régénère la dernière réponse) ; la réponse arrive
/// par `onEvent` (`started`, `delta`…, `done`), propre à cette requête. La promesse se résout
/// après `done` ; elle est rejetée avant tout événement si la requête est refusée (images
/// refusées, réponse déjà en cours, conversation introuvable, enregistrement impossible). Une
/// erreur d'Ollama arrive dans `done`, avec la réponse partielle.
#[tauri::command]
pub async fn chat_send(
    state: State<'_, HarnessState>,
    request: ChatSend,
    on_event: Channel<ChatEvent>,
) -> CmdResult<()> {
    let client = state.require_ollama()?;
    let store = state.conversations()?;
    let vision = state.model_vision(client, request.model.trim()).await;
    chat::send(
        client,
        store,
        state.generations(),
        vision,
        request,
        |event| {
            // Fenêtre fermée ou rechargée : la réponse est quand même enregistrée.
            if let Err(e) = on_event.send(event) {
                tracing::debug!("événement de discussion non transmis : {e}");
            }
        },
    )
    .await
}

/// Arrête la réponse en cours dans cette conversation (le début est gardé, marqué
/// « interrompue ») ; `false` s'il n'y en a pas.
#[tauri::command]
pub fn chat_stop(state: State<'_, HarnessState>, conversation_id: String) -> bool {
    state.generations().stop(&conversation_id)
}

/// Conversations enregistrées, la plus récente d'abord.
#[tauri::command]
pub async fn list_conversations(
    state: State<'_, HarnessState>,
) -> CmdResult<Vec<ConversationSummary>> {
    Ok(state.conversations()?.list())
}

#[tauri::command]
pub async fn get_conversation(
    state: State<'_, HarnessState>,
    id: String,
) -> CmdResult<Conversation> {
    Ok(state.conversations()?.get(&id)?)
}

/// Change le titre (espaces réunis, 120 caractères au plus).
#[tauri::command]
pub async fn rename_conversation(
    state: State<'_, HarnessState>,
    id: String,
    title: String,
) -> CmdResult<ConversationSummary> {
    let title = clean_title(&title)
        .ok_or_else(|| CommandError::msg("Titre vide : écrire un titre, ou annuler"))?;
    state
        .conversations()?
        .update(&id, |c| {
            c.title = title;
            Ok::<_, StoreError>(c.summary())
        })
        .map_err(Into::into)
}

/// Change les instructions de la conversation (vides : aucune), envoyées en message `system`
/// à partir du prochain message ; sans effet sur la date.
#[tauri::command]
pub async fn set_instructions(
    state: State<'_, HarnessState>,
    id: String,
    instructions: Option<String>,
) -> CmdResult<()> {
    let instructions = clean_instructions(instructions.as_deref());
    state
        .conversations()?
        .update(&id, |c| {
            c.instructions = instructions;
            Ok::<_, StoreError>(())
        })
        .map_err(Into::into)
}

/// Retient la bascule « Réflexion » de la conversation ; sans effet sur la date.
#[tauri::command]
pub async fn set_think(state: State<'_, HarnessState>, id: String, think: bool) -> CmdResult<()> {
    state
        .conversations()?
        .update(&id, |c| {
            c.think = Some(think);
            Ok::<_, StoreError>(())
        })
        .map_err(Into::into)
}

/// Supprime la conversation et ses images, après avoir arrêté sa réponse en cours ; `false`
/// si elle n'existait plus.
#[tauri::command]
pub async fn delete_conversation(state: State<'_, HarnessState>, id: String) -> CmdResult<bool> {
    state.generations().stop(&id);
    Ok(state.conversations()?.delete(&id)?)
}

/// Image jointe à un message, en adresse `data:` (la CSP de la WebView n'autorise que
/// `'self'` et `data:` pour les images).
#[tauri::command]
pub async fn get_image(
    state: State<'_, HarnessState>,
    conversation_id: String,
    name: String,
) -> CmdResult<String> {
    let image = state.conversations()?.read_image(&conversation_id, &name)?;
    Ok(images::data_url(image.kind, &image.bytes))
}

/// Modèle demandé par le dernier lien `gram-harness://open?model=…` (au lancement ou depuis,
/// annoncé par l'événement `open-model`), une seule fois ; `null` s'il n'y en a pas.
#[tauri::command]
pub fn take_opened_model(state: State<'_, HarnessState>) -> Option<String> {
    state.open_requests().take()
}
