//! Microsoft Graph API operations

use anyhow::{Context, Result};
use ost_microsoft::teams::models::{
    GraphChatListItem as Chat, GraphChatListResponse as ChatsResponse,
};

use super::client::TeamsClient;

/// Build a display name for a chat.
/// Uses topic if set, otherwise joins member names, otherwise falls back to chat type.
fn chat_display_name(chat: &Chat) -> String {
    if let Some(ref topic) = chat.topic {
        if !topic.is_empty() {
            return topic.clone();
        }
    }
    if let Some(ref members) = chat.members {
        let names: Vec<&str> = members
            .iter()
            .filter_map(|m| m.display_name.as_deref())
            .collect();
        if !names.is_empty() {
            return names.join(", ");
        }
    }
    format!("[{}]", chat.chat_type)
}

pub async fn list_chats(limit: usize) -> Result<()> {
    let client = TeamsClient::new().await?;

    let path = format!(
        "/me/chats?$top={}&$orderby=lastUpdatedDateTime desc&$expand=members,lastMessagePreview",
        limit
    );
    let resp = client.graph_get(&path).await?;
    let chats: ChatsResponse = resp
        .json()
        .await
        .context("Failed to parse chats response")?;

    println!("\nRecent Chats:");
    println!("{:-<60}", "");

    for chat in &chats.value {
        let name = chat_display_name(chat);
        let updated = chat.last_updated.as_deref().unwrap_or("unknown");
        println!("{}", name);
        println!("  ID:      {}", chat.id);
        println!("  Updated: {}", updated);

        if let Some(ref preview) = chat.last_message_preview {
            if let Some(ref body) = preview.body {
                if let Some(ref content) = body.content {
                    let text = if content.len() > 80 {
                        let end = content
                            .char_indices()
                            .map(|(i, _)| i)
                            .take_while(|&i| i <= 77)
                            .last()
                            .unwrap_or(0);
                        format!("{}...", &content[..end])
                    } else {
                        content.clone()
                    };
                    println!("  Preview: {}", text);
                }
            }
        }

        println!();
    }

    if chats.value.is_empty() {
        println!("  (no chats found)");
    }

    if chats.next_link.is_some() {
        println!("(showing first {} -- more available)", limit);
    }

    Ok(())
}

/// Send a message to a chat
pub async fn send_message(chat_id: &str, message: &str) -> Result<()> {
    let client = TeamsClient::new().await?;

    let body = serde_json::json!({
        "body": {
            "content": message
        }
    });

    let path = format!("/me/chats/{}/messages", chat_id);
    client.graph_post(&path, &body).await?;

    println!("Message sent successfully.");
    Ok(())
}
