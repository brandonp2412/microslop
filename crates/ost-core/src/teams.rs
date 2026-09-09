use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures::StreamExt;
use ost_microsoft::teams::{self as microsoft_teams, models::*};
use ost_platform::PlatformCacheStorage;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(all(target_os = "android", debug_assertions))]
use std::ffi::CString;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        OnceLock, RwLock,
        atomic::{AtomicBool, Ordering},
    },
};

macro_rules! chat_diag {
    ($message:expr) => {
        #[cfg(debug_assertions)]
        chat_diag($message)
    };
}

mod parsing;
mod service_chats;
mod service_directory;
mod service_messages;
mod service_profiles;
mod service_sending;
mod service_transport;

use parsing::*;

use crate::auth::{AuthService, is_unauthorized};

const GRAPH_BASE: &str = microsoft_teams::GRAPH_BASE;
const DEFAULT_CHAT_SERVICE: &str = microsoft_teams::DEFAULT_CHAT_SERVICE;
const AMS_USER_AGENT: &str = microsoft_teams::AMS_USER_AGENT;
const DEFAULT_AMS_SERVICE: &str = microsoft_teams::DEFAULT_AMS_SERVICE;

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
static CHAT_CACHE: OnceLock<RwLock<Option<Vec<Chat>>>> = OnceLock::new();
static TEAM_CACHE: OnceLock<RwLock<Option<Vec<Team>>>> = OnceLock::new();
static GRAPH_CHATS_FORBIDDEN: AtomicBool = AtomicBool::new(false);

#[cfg(debug_assertions)]
fn chat_diag(message: impl AsRef<str>) {
    #[cfg(all(target_os = "android", debug_assertions))]
    {
        const ANDROID_LOG_INFO: std::os::raw::c_int = 4;
        unsafe extern "C" {
            fn __android_log_write(
                priority: std::os::raw::c_int,
                tag: *const std::os::raw::c_char,
                text: *const std::os::raw::c_char,
            ) -> std::os::raw::c_int;
        }
        let Ok(tag) = CString::new("OST_CHAT_DIAG") else {
            return;
        };
        let Ok(text) = CString::new(message.as_ref()) else {
            return;
        };
        unsafe {
            __android_log_write(ANDROID_LOG_INFO, tag.as_ptr(), text.as_ptr());
        }
    }
    #[cfg(all(not(target_os = "android"), debug_assertions))]
    {
        use std::io::Write;
        let _ = writeln!(
            std::io::stderr().lock(),
            "OST_CHAT_DIAG {}",
            message.as_ref()
        );
    }
}

fn message_cache_key(chat_id: &str) -> String {
    format!("message-cache-{:x}", Sha256::digest(chat_id.as_bytes()))
}

fn http_client() -> reqwest::Client {
    HTTP_CLIENT.get_or_init(reqwest::Client::new).clone()
}

fn chat_cache() -> &'static RwLock<Option<Vec<Chat>>> {
    CHAT_CACHE.get_or_init(|| RwLock::new(None))
}

fn team_cache() -> &'static RwLock<Option<Vec<Team>>> {
    TEAM_CACHE.get_or_init(|| RwLock::new(None))
}

pub(crate) async fn clear_session_cache() -> Result<()> {
    if let Ok(mut cache) = chat_cache().write() {
        *cache = None;
    }
    if let Ok(mut cache) = team_cache().write() {
        *cache = None;
    }
    GRAPH_CHATS_FORBIDDEN.store(false, Ordering::Relaxed);

    tokio::task::spawn_blocking(move || {
        let cache = PlatformCacheStorage::new()?;
        cache.delete("chat-cache")?;
        cache.delete_prefix("message-cache-")
    })
    .await
    .context("Could not join the Teams cache cleanup")?
}

fn cached_chat_name(chat_id: &str) -> Option<String> {
    chat_cache()
        .read()
        .ok()?
        .as_ref()?
        .iter()
        .find(|chat| chat.id == chat_id)
        .map(|chat| chat.name.trim())
        .filter(|name| is_displayable_chat_name(name))
        .filter(|name| *name != "Chat" && *name != "Group chat")
        .map(ToOwned::to_owned)
}

fn is_forbidden(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<reqwest::Error>()
        .and_then(reqwest::Error::status)
        == Some(reqwest::StatusCode::FORBIDDEN)
}

async fn cached_chats(network_error: anyhow::Error) -> Result<Vec<Chat>> {
    if let Ok(cache) = chat_cache().read()
        && let Some(cached) = cache.as_ref()
    {
        return Ok(cached.clone());
    }
    let cached = tokio::task::spawn_blocking(|| PlatformCacheStorage::new()?.get("chat-cache"))
        .await
        .context("Could not join the Teams chat-cache read")??;
    cached
        .map(|cached| serde_json::from_str(&cached).context("Could not read the cached chats"))
        .transpose()?
        .context(network_error)
}

async fn load_cached_messages(chat_id: &str) -> Result<Option<Vec<ChatMessage>>> {
    let key = message_cache_key(chat_id);
    tokio::task::spawn_blocking(move || PlatformCacheStorage::new()?.get(&key))
        .await
        .context("Could not join the Teams message-cache read")??
        .map(|cached| serde_json::from_str(&cached).context("Could not read cached messages"))
        .transpose()
}

fn persist_messages(chat_id: &str, messages: &[ChatMessage]) {
    let key = message_cache_key(chat_id);
    if let Ok(serialized) = serde_json::to_string(messages) {
        tokio::task::spawn_blocking(move || {
            if let Err(error) =
                PlatformCacheStorage::new().and_then(|cache| cache.set(&key, &serialized))
            {
                tracing::warn!("Could not persist the Teams message cache: {error:#}");
            }
        });
    }
}

#[derive(Clone, Debug)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub channels: Vec<Channel>,
}

#[derive(Clone, Debug)]
pub struct Channel {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Chat {
    pub id: String,
    pub name: String,
    pub is_group: bool,
    pub profile_photo_user_id: Option<String>,
    pub member_user_ids: Vec<String>,
    pub team_id: Option<String>,
    pub last_message_id: Option<String>,
    pub last_message_preview: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub sender: String,
    pub sender_id: Option<String>,
    pub is_from_current_user: bool,
    pub timestamp: String,
    pub content: String,
    #[serde(default)]
    pub quotes: Vec<MessageQuote>,
    pub images: Vec<MessageImage>,
    #[serde(default)]
    pub image_urls: Vec<String>,
    pub reactions: Vec<MessageReaction>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageQuote {
    pub message_id: Option<String>,
    pub sender: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageImage {
    #[serde(default)]
    pub source_url: Option<String>,
    pub content_type: String,
    pub data_base64: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReactionUser {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageReaction {
    pub reaction_type: String,
    pub count: usize,
    pub selected: bool,
    #[serde(default)]
    pub users: Vec<ReactionUser>,
}

#[derive(Clone, Debug)]
pub struct UserProfile {
    pub id: Option<String>,
    pub display_name: String,
    pub email: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserPresence {
    pub user_id: String,
    pub availability: String,
    pub activity: String,
}

#[derive(Clone, Debug)]
pub struct UserDetails {
    pub user_id: String,
    pub display_name: String,
    pub email: Option<String>,
    pub job_title: Option<String>,
    pub availability: Option<String>,
    pub activity: Option<String>,
    pub status_message: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CustomReaction {
    pub reaction_type: String,
    pub shortcut: String,
    pub document_id: String,
    pub content_type: String,
    pub content_base64: String,
}

pub struct TeamsService<'a> {
    auth: &'a AuthService,
    client: reqwest::Client,
}

#[derive(Clone, Copy)]
enum GraphConversation<'a> {
    Chat(&'a str),
    Channel {
        team_id: &'a str,
        channel_id: &'a str,
    },
}

impl GraphConversation<'_> {
    fn messages_path(self) -> String {
        match self {
            Self::Chat(chat_id) => format!("/chats/{chat_id}/messages"),
            Self::Channel {
                team_id,
                channel_id,
            } => {
                format!("/teams/{team_id}/channels/{channel_id}/messages")
            }
        }
    }
}

struct ChatMembers {
    names: Vec<String>,
    photo_user_id: Option<String>,
    user_ids: Vec<String>,
}

#[cfg(test)]
mod tests;
