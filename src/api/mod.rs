//! API client module for Microsoft Teams

mod chat;
pub mod client;
mod me;
mod teams;

use anyhow::Result;

pub use chat::{ChatInfo, MessageInfo};
pub use me::UserInfo;
pub use teams::TeamInfo;

pub use chat::{list_chats_data, read_messages_data, send_message_with_client};
pub use me::whoami_data;
pub use teams::list_teams_data;

pub async fn list_chats(limit: usize) -> Result<()> {
    chat::list_chats(limit).await
}

pub async fn read_messages(chat_id: &str, limit: usize) -> Result<()> {
    chat::read_messages(chat_id, limit).await
}

pub async fn send_message(to: &str, message: &str) -> Result<()> {
    chat::send_message(to, message).await
}

pub async fn whoami() -> Result<()> {
    me::whoami().await
}

pub async fn list_teams() -> Result<()> {
    teams::list_teams().await
}
