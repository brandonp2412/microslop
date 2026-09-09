//! Async backend: bridges the sync TUI event loop with async API calls.
//!
//! Uses an mpsc channel pair. The TUI sends `BackendCommand` values, and a
//! background tokio task executes them and sends `BackendResponse` values back.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::mpsc;

use crate::api;
use crate::api::client::TeamsClient;

const PREFETCH_LIMIT: usize = 12;

/// Commands sent from the TUI event loop to the async backend.
pub enum BackendCommand {
    LoadTeams,
    LoadChats { limit: usize },
    LoadMessages { chat_id: String, limit: usize },
    PrefetchMessages { chat_ids: Vec<String>, limit: usize },
    InvalidateMessages { chat_id: String },
    SendMessage { chat_id: String, message: String },
    LoadUserInfo,
}

pub enum BackendResponse {
    Teams(Result<Vec<api::TeamInfo>>),
    Chats(Result<Vec<api::ChatInfo>>),
    Messages {
        chat_id: String,
        result: Result<Vec<api::MessageInfo>>,
    },
    MessageSent(Result<()>),
    UserInfo(Result<api::UserInfo>),
    ClientError(String),
}

pub struct Backend {
    cmd_tx: mpsc::UnboundedSender<BackendCommand>,
    resp_rx: mpsc::UnboundedReceiver<BackendResponse>,
}

pub(super) fn prefetch_ids<'a, I, J>(channels: I, chats: J) -> Vec<String>
where
    I: IntoIterator<Item = &'a str>,
    J: IntoIterator<Item = &'a str>,
{
    let mut seen = HashSet::new();
    channels
        .into_iter()
        .chain(chats)
        .filter(|id| !id.is_empty() && seen.insert(*id))
        .take(PREFETCH_LIMIT)
        .map(str::to_owned)
        .collect()
}

impl Backend {
    ///
    /// Returns the Backend handle for sending commands and receiving responses.
    pub fn start() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (resp_tx, resp_rx) = mpsc::unbounded_channel();

        tokio::spawn(backend_loop(cmd_rx, resp_tx));

        Self { cmd_tx, resp_rx }
    }

    pub fn send(&self, cmd: BackendCommand) {
        if self.cmd_tx.send(cmd).is_err() {
            tracing::error!("Backend channel closed -- command dropped");
        }
    }

    ///
    /// Suspends until a response is available. Returns `None` only when the
    pub async fn recv(&mut self) -> Option<BackendResponse> {
        self.resp_rx.recv().await
    }
}

async fn backend_loop(
    mut cmd_rx: mpsc::UnboundedReceiver<BackendCommand>,
    resp_tx: mpsc::UnboundedSender<BackendResponse>,
) {
    let client = match TeamsClient::new().await {
        Ok(c) => Arc::new(c),
        Err(e) => {
            let _ = resp_tx.send(BackendResponse::ClientError(format!("{:#}", e)));
            return;
        }
    };
    let cache = Arc::new(Mutex::new(std::collections::HashMap::<
        String,
        Vec<api::MessageInfo>,
    >::new()));

    while let Some(cmd) = cmd_rx.recv().await {
        let client = Arc::clone(&client);
        let resp_tx = resp_tx.clone();
        let cache = Arc::clone(&cache);

        tokio::spawn(async move {
            match cmd {
                BackendCommand::LoadTeams => {
                    let result = api::list_teams_data(&client).await;
                    let _ = resp_tx.send(BackendResponse::Teams(result));
                }
                BackendCommand::LoadChats { limit } => {
                    let result = api::list_chats_data(&client, limit).await;
                    let _ = resp_tx.send(BackendResponse::Chats(result));
                }
                BackendCommand::LoadMessages { chat_id, limit } => {
                    let result = match cache.lock().ok().and_then(|c| c.get(&chat_id).cloned()) {
                        Some(messages) => Ok(messages),
                        None => {
                            let result = api::read_messages_data(&client, &chat_id, limit).await;
                            if let Ok(ref messages) = result {
                                if let Ok(mut c) = cache.lock() {
                                    c.insert(chat_id.clone(), messages.clone());
                                }
                            }
                            result
                        }
                    };
                    let _ = resp_tx.send(BackendResponse::Messages { chat_id, result });
                }
                BackendCommand::PrefetchMessages { chat_ids, limit } => {
                    for chat_id in chat_ids {
                        if cache
                            .lock()
                            .map(|c| c.contains_key(&chat_id))
                            .unwrap_or(false)
                        {
                            continue;
                        }
                        let result = api::read_messages_data(&client, &chat_id, limit).await;
                        if let Ok(messages) = result {
                            if let Ok(mut c) = cache.lock() {
                                c.insert(chat_id, messages);
                            }
                        }
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                }
                BackendCommand::InvalidateMessages { chat_id } => {
                    if let Ok(mut c) = cache.lock() {
                        c.remove(&chat_id);
                    }
                }
                BackendCommand::SendMessage { chat_id, message } => {
                    let result = api::send_message_with_client(&client, &chat_id, &message).await;
                    let _ = resp_tx.send(BackendResponse::MessageSent(result));
                }
                BackendCommand::LoadUserInfo => {
                    let result = api::whoami_data(&client).await;
                    let _ = resp_tx.send(BackendResponse::UserInfo(result));
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::prefetch_ids;

    #[test]
    fn prefetch_ids_are_bounded_and_deduplicated_in_order() {
        let ids = prefetch_ids(
            ["channel-a", "channel-b", "channel-a", "channel-c"],
            ["chat-a", "channel-b", "chat-b"],
        );

        assert_eq!(
            ids,
            vec!["channel-a", "channel-b", "channel-c", "chat-a", "chat-b"]
        );
    }
}
