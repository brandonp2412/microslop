use anyhow::{Context, Result};
use ost_core::{auth::AuthService, teams::TeamsService};

use super::auth;

#[derive(Clone)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub channels: Vec<Channel>,
}

#[derive(Clone)]
pub struct Channel {
    pub id: String,
    pub name: String,
}

#[derive(Clone)]
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

#[derive(Clone)]
pub struct SavedAccountConversation {
    pub account_id: String,
    pub account_name: String,
    pub conversation_id: String,
    pub conversation_name: String,
    pub is_group: bool,
    pub is_channel: bool,
    pub last_message_id: Option<String>,
    pub last_message_preview: Option<String>,
}

#[derive(Clone)]
pub struct SavedAccountNotification {
    pub account_id: String,
    pub account_name: String,
    pub conversation_id: String,
    pub conversation_name: String,
    pub is_group: bool,
    pub is_channel: bool,
    pub message: Message,
    pub messages: Vec<Message>,
}

#[derive(Clone)]
pub struct Message {
    pub id: String,
    pub sender: String,
    pub sender_id: Option<String>,
    pub is_from_current_user: bool,
    pub timestamp: String,
    pub content: String,
    pub quotes: Vec<MessageQuote>,
    pub images: Vec<MessageImage>,
    pub image_urls: Vec<String>,
    pub reactions: Vec<Reaction>,
}

#[derive(Clone)]
pub struct MessageQuote {
    pub message_id: Option<String>,
    pub sender: String,
    pub content: String,
}

#[derive(Clone)]
pub struct MessageImage {
    pub source_url: Option<String>,
    pub content_type: String,
    pub data_base64: String,
}

#[derive(Clone)]
pub struct ReactionUser {
    pub id: String,
    pub name: String,
}

#[derive(Clone)]
pub struct Reaction {
    pub reaction_type: String,
    pub count: usize,
    pub selected: bool,
    pub users: Vec<ReactionUser>,
}

#[derive(Clone)]
pub struct UserProfile {
    pub id: Option<String>,
    pub display_name: String,
    pub email: Option<String>,
}

#[derive(Clone)]
pub struct UserPresence {
    pub user_id: String,
    pub availability: String,
    pub activity: String,
}

#[derive(Clone)]
pub struct UserDetails {
    pub user_id: String,
    pub display_name: String,
    pub email: Option<String>,
    pub job_title: Option<String>,
    pub availability: Option<String>,
    pub activity: Option<String>,
    pub status_message: Option<String>,
}

#[derive(Clone)]
pub struct CustomReaction {
    pub reaction_type: String,
    pub shortcut: String,
    pub document_id: String,
    pub content_type: String,
    pub content_base64: String,
}

pub async fn get_custom_reaction(reaction_type: String) -> Result<Option<CustomReaction>> {
    Ok(TeamsService::new(auth::service())
        .custom_reaction(&reaction_type)
        .await?
        .map(|reaction| CustomReaction {
            reaction_type: reaction.reaction_type,
            shortcut: reaction.shortcut,
            document_id: reaction.document_id,
            content_type: reaction.content_type,
            content_base64: reaction.content_base64,
        }))
}

pub async fn list_teams() -> Result<Vec<Team>> {
    Ok(TeamsService::new(auth::service())
        .list_teams()
        .await?
        .into_iter()
        .map(|team| Team {
            id: team.id,
            name: team.name,
            channels: team
                .channels
                .into_iter()
                .map(|channel| Channel {
                    id: channel.id,
                    name: channel.name,
                })
                .collect(),
        })
        .collect())
}

pub async fn list_chats(limit: usize) -> Result<Vec<Chat>> {
    Ok(TeamsService::new(auth::service())
        .list_chats(limit)
        .await?
        .into_iter()
        .map(|chat| Chat {
            id: chat.id,
            name: chat.name,
            is_group: chat.is_group,
            profile_photo_user_id: chat.profile_photo_user_id,
            member_user_ids: chat.member_user_ids,
            team_id: chat.team_id,
            last_message_id: chat.last_message_id,
            last_message_preview: chat.last_message_preview,
        })
        .collect())
}

pub async fn get_presences(user_ids: Vec<String>) -> Result<Vec<UserPresence>> {
    Ok(TeamsService::new(auth::service())
        .presences(&user_ids)
        .await?
        .into_iter()
        .map(|presence| UserPresence {
            user_id: presence.user_id,
            availability: presence.availability,
            activity: presence.activity,
        })
        .collect())
}

pub async fn get_user_details(user_id: String) -> Result<UserDetails> {
    let details = TeamsService::new(auth::service())
        .user_details(&user_id)
        .await?;
    Ok(UserDetails {
        user_id: details.user_id,
        display_name: details.display_name,
        email: details.email,
        job_title: details.job_title,
        availability: details.availability,
        activity: details.activity,
        status_message: details.status_message,
    })
}

pub async fn list_saved_account_conversations(
    account_id: String,
    limit: usize,
) -> Result<Vec<SavedAccountConversation>> {
    let account = auth::service()
        .saved_accounts()
        .await?
        .into_iter()
        .find(|account| account.id == account_id)
        .context("Saved Microsoft account was not found")?;
    let service = AuthService::ephemeral();
    service.restore_saved_account(&account.id).await?;
    let teams = TeamsService::new(&service);
    let mut conversations = teams
        .list_chats_uncached(limit)
        .await?
        .into_iter()
        .map(|chat| SavedAccountConversation {
            account_id: account.id.clone(),
            account_name: account.display_name.clone(),
            conversation_id: chat.id,
            conversation_name: chat.name,
            is_group: chat.is_group,
            is_channel: false,
            last_message_id: chat.last_message_id,
            last_message_preview: chat.last_message_preview,
        })
        .collect::<Vec<_>>();
    for team in teams.list_teams_uncached().await.unwrap_or_default() {
        conversations.extend(
            team.channels
                .into_iter()
                .map(|channel| SavedAccountConversation {
                    account_id: account.id.clone(),
                    account_name: account.display_name.clone(),
                    conversation_id: channel.id,
                    conversation_name: format!("{} · {}", team.name, channel.name),
                    is_group: true,
                    is_channel: true,
                    last_message_id: None,
                    last_message_preview: None,
                }),
        );
    }
    Ok(conversations)
}

pub async fn read_saved_account_notification(
    account_id: String,
    conversation_id: String,
    message_limit: usize,
) -> Result<Option<SavedAccountNotification>> {
    let account = auth::service()
        .saved_accounts()
        .await?
        .into_iter()
        .find(|account| account.id == account_id)
        .context("Saved Microsoft account was not found")?;
    let service = AuthService::ephemeral();
    service.restore_saved_account(&account.id).await?;
    let teams = TeamsService::new(&service);
    let message_limit = message_limit.clamp(1, 25);
    if let Some(chat) = teams
        .list_chats_uncached(100)
        .await?
        .into_iter()
        .find(|chat| chat.id == conversation_id)
    {
        let messages = teams
            .read_messages(&conversation_id, message_limit, false)
            .await?
            .into_iter()
            .map(Message::from)
            .collect::<Vec<_>>();
        let Some(message) = messages.last().cloned() else {
            return Ok(None);
        };
        return Ok(Some(SavedAccountNotification {
            account_id: account.id,
            account_name: account.display_name,
            conversation_id,
            conversation_name: chat.name,
            is_group: chat.is_group,
            is_channel: false,
            message,
            messages,
        }));
    }
    for team in teams.list_teams_uncached().await.unwrap_or_default() {
        if let Some(channel) = team
            .channels
            .into_iter()
            .find(|channel| channel.id == conversation_id)
        {
            let messages = teams
                .read_channel_messages(&team.id, &conversation_id, message_limit, false)
                .await?
                .into_iter()
                .map(Message::from)
                .collect::<Vec<_>>();
            let Some(message) = messages.last().cloned() else {
                return Ok(None);
            };
            return Ok(Some(SavedAccountNotification {
                account_id: account.id,
                account_name: account.display_name,
                conversation_id,
                conversation_name: format!("{} · {}", team.name, channel.name),
                is_group: true,
                is_channel: true,
                message,
                messages,
            }));
        }
    }
    Ok(None)
}

impl From<ost_core::teams::ChatMessage> for Message {
    fn from(message: ost_core::teams::ChatMessage) -> Self {
        Self {
            id: message.id,
            sender: message.sender,
            sender_id: message.sender_id,
            is_from_current_user: message.is_from_current_user,
            timestamp: message.timestamp,
            content: message.content,
            quotes: message
                .quotes
                .into_iter()
                .map(|quote| MessageQuote {
                    message_id: quote.message_id,
                    sender: quote.sender,
                    content: quote.content,
                })
                .collect(),
            image_urls: message.image_urls,
            images: message
                .images
                .into_iter()
                .map(|image| MessageImage {
                    source_url: image.source_url,
                    content_type: image.content_type,
                    data_base64: image.data_base64,
                })
                .collect(),
            reactions: message
                .reactions
                .into_iter()
                .map(|reaction| Reaction {
                    reaction_type: reaction.reaction_type,
                    count: reaction.count,
                    selected: reaction.selected,
                    users: reaction
                        .users
                        .into_iter()
                        .map(|user| ReactionUser {
                            id: user.id,
                            name: user.name,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

pub async fn read_messages(
    conversation_id: String,
    limit: usize,
    include_images: bool,
) -> Result<Vec<Message>> {
    Ok(TeamsService::new(auth::service())
        .read_messages(&conversation_id, limit, include_images)
        .await?
        .into_iter()
        .map(Into::into)
        .collect())
}

pub async fn send_message(conversation_id: String, content: String) -> Result<()> {
    TeamsService::new(auth::service())
        .send_message(&conversation_id, &content)
        .await
}

pub async fn set_reaction(
    conversation_id: String,
    team_id: Option<String>,
    message_id: String,
    reaction_type: String,
    remove: bool,
) -> Result<()> {
    let channel_id = team_id.as_ref().map(|_| conversation_id.as_str());
    TeamsService::new(auth::service())
        .set_reaction(
            team_id.is_none().then_some(conversation_id.as_str()),
            team_id.as_deref(),
            channel_id,
            &message_id,
            &reaction_type,
            remove,
        )
        .await
}

pub async fn get_user_profile() -> Result<UserProfile> {
    let profile = TeamsService::new(auth::service()).user_profile().await?;
    Ok(UserProfile {
        id: profile.id,
        display_name: profile.display_name,
        email: profile.email,
    })
}

pub async fn get_profile_photo(user_id: String) -> Result<Option<String>> {
    TeamsService::new(auth::service())
        .profile_photo(&user_id)
        .await
}

pub async fn get_chat_photo(chat_id: String) -> Result<Option<String>> {
    TeamsService::new(auth::service())
        .chat_photo(&chat_id)
        .await
}

pub async fn get_team_photo(team_id: String) -> Result<Option<String>> {
    TeamsService::new(auth::service())
        .team_photo(&team_id)
        .await
}

pub async fn send_image_message(
    conversation_id: String,
    team_id: Option<String>,
    caption: String,
    content_type: String,
    data_base64: String,
) -> Result<()> {
    let channel_id = team_id.as_ref().map(|_| conversation_id.as_str());
    TeamsService::new(auth::service())
        .send_image_message(
            team_id.is_none().then_some(conversation_id.as_str()),
            team_id.as_deref(),
            channel_id,
            &caption,
            &content_type,
            &data_base64,
        )
        .await
}

pub async fn send_file_message(
    conversation_id: String,
    team_id: Option<String>,
    file_name: String,
    content_type: String,
    data_base64: String,
) -> Result<()> {
    let service = TeamsService::new(auth::service());
    if file_name.starts_with("voice-") && content_type.starts_with("audio/") {
        return service
            .send_audio_message(&conversation_id, &content_type, &data_base64)
            .await;
    }
    let channel_id = team_id.as_ref().map(|_| conversation_id.as_str());
    service
        .send_file_message(
            team_id.is_none().then_some(conversation_id.as_str()),
            team_id.as_deref(),
            channel_id,
            &file_name,
            &content_type,
            &data_base64,
        )
        .await
}

pub async fn read_channel_messages(
    team_id: String,
    channel_id: String,
    limit: usize,
    include_images: bool,
) -> Result<Vec<Message>> {
    Ok(TeamsService::new(auth::service())
        .read_channel_messages(&team_id, &channel_id, limit, include_images)
        .await?
        .into_iter()
        .map(|message| Message {
            id: message.id,
            sender: message.sender,
            sender_id: message.sender_id,
            is_from_current_user: message.is_from_current_user,
            timestamp: message.timestamp,
            content: message.content,
            quotes: message
                .quotes
                .into_iter()
                .map(|quote| MessageQuote {
                    message_id: quote.message_id,
                    sender: quote.sender,
                    content: quote.content,
                })
                .collect(),
            image_urls: message.image_urls,
            images: message
                .images
                .into_iter()
                .map(|image| MessageImage {
                    source_url: image.source_url,
                    content_type: image.content_type,
                    data_base64: image.data_base64,
                })
                .collect(),
            reactions: message
                .reactions
                .into_iter()
                .map(|reaction| Reaction {
                    reaction_type: reaction.reaction_type,
                    count: reaction.count,
                    selected: reaction.selected,
                    users: reaction
                        .users
                        .into_iter()
                        .map(|user| ReactionUser {
                            id: user.id,
                            name: user.name,
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect())
}

pub async fn send_channel_message(
    team_id: String,
    channel_id: String,
    content: String,
) -> Result<()> {
    TeamsService::new(auth::service())
        .send_channel_message(&team_id, &channel_id, &content)
        .await
}

pub async fn load_message_image(url: String) -> Result<Option<MessageImage>> {
    Ok(TeamsService::new(auth::service())
        .download_message_image(&url)
        .await?
        .map(|image| MessageImage {
            source_url: image.source_url,
            content_type: image.content_type,
            data_base64: image.data_base64,
        }))
}

pub async fn reaction_user_name(user_id: String) -> Result<String> {
    TeamsService::new(auth::service())
        .user_display_name(&user_id)
        .await
}
