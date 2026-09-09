use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct GraphCollection<T> {
    pub value: Vec<T>,
    #[serde(rename = "@odata.nextLink")]
    pub next_link: Option<String>,
}

#[derive(Deserialize)]
pub struct GraphTeam {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
}

#[derive(Deserialize)]
pub struct GraphChannel {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
}

#[derive(Deserialize)]
pub struct GraphUser {
    pub id: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub mail: Option<String>,
    #[serde(rename = "userPrincipalName")]
    pub user_principal_name: Option<String>,
    #[serde(rename = "jobTitle")]
    pub job_title: Option<String>,
}

#[derive(Serialize)]
pub struct GraphPresenceRequest<'a> {
    pub ids: &'a [String],
}

#[derive(Deserialize)]
pub struct GraphPresence {
    pub id: String,
    pub availability: String,
    pub activity: String,
    #[serde(rename = "statusMessage")]
    pub status_message: Option<GraphPresenceStatusMessage>,
}

#[derive(Deserialize)]
pub struct GraphPresenceStatusMessage {
    pub message: Option<GraphItemBody>,
}

#[derive(Deserialize)]
pub struct GraphItemBody {
    pub content: Option<String>,
}

#[derive(Serialize)]
pub struct NativePresenceRequest {
    pub mri: String,
}

#[derive(Deserialize)]
pub struct NativePresenceResponse {
    pub mri: String,
    pub presence: Option<NativePresence>,
}

#[derive(Deserialize)]
pub struct NativePresence {
    pub availability: String,
    pub activity: String,
}

#[derive(Deserialize)]
pub struct GraphChat {
    pub id: String,
    pub members: Option<Vec<GraphChatMember>>,
    pub topic: Option<String>,
    #[serde(rename = "chatType")]
    pub chat_type: Option<String>,
    pub viewpoint: Option<GraphChatViewpoint>,
    #[serde(rename = "lastMessagePreview")]
    pub last_message_preview: Option<GraphMessage>,
}

#[derive(Deserialize)]
pub struct GraphChatViewpoint {
    #[serde(rename = "isHidden")]
    pub is_hidden: Option<bool>,
}

#[derive(Deserialize)]
pub struct GraphChatMember {
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "userId")]
    pub user_id: Option<String>,
}

#[derive(Deserialize)]
pub struct GraphMessage {
    pub id: Option<String>,
    #[serde(rename = "createdDateTime")]
    pub created_date_time: Option<String>,
    pub body: Option<GraphMessageBody>,
    pub from: Option<GraphMessageFrom>,
    pub reactions: Option<Vec<GraphReaction>>,
    pub attachments: Option<Vec<GraphMessageAttachment>>,
}

#[derive(Deserialize)]
pub struct GraphMessageAttachment {
    pub content: Option<String>,
    #[serde(rename = "contentType")]
    pub content_type: Option<String>,
    #[serde(rename = "contentUrl")]
    pub content_url: Option<String>,
    #[serde(rename = "thumbnailUrl")]
    pub thumbnail_url: Option<String>,
    pub name: Option<String>,
}

#[derive(Deserialize)]
pub struct GraphMessageBody {
    pub content: Option<String>,
}

#[derive(Deserialize)]
pub struct GraphMessageFrom {
    pub user: Option<GraphMessageUser>,
    pub application: Option<GraphMessageUser>,
}

#[derive(Deserialize)]
pub struct GraphMessageUser {
    pub id: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
}

#[derive(Deserialize)]
pub struct ConversationsResponse {
    pub conversations: Option<Vec<Conversation>>,
    #[serde(rename = "_metadata")]
    pub metadata: Option<ConversationsMetadata>,
}

#[derive(Deserialize)]
pub struct ConversationsMetadata {
    #[serde(rename = "backwardLink")]
    pub backward_link: Option<String>,
}

#[derive(Deserialize)]
pub struct Conversation {
    pub id: Option<String>,
    #[serde(rename = "threadProperties")]
    pub thread_properties: Option<ThreadProperties>,
    #[serde(rename = "lastMessage")]
    pub last_message: Option<NativeMessage>,
}

#[derive(Deserialize)]
pub struct ThreadProperties {
    pub topic: Option<String>,
    #[serde(rename = "lastjoinat")]
    pub last_join_at: Option<String>,
    pub members: Option<String>,
}

#[derive(Deserialize)]
pub struct NativeMessage {
    pub id: Option<String>,
    #[serde(rename = "composetime")]
    pub compose_time: Option<String>,
    #[serde(rename = "originalarrivaltime")]
    pub original_arrival_time: Option<String>,
    #[serde(rename = "imdisplayname")]
    pub im_display_name: Option<String>,
    pub from: Option<String>,
    pub content: Option<String>,
    pub messagetype: Option<String>,
    pub properties: Option<NativeMessageProperties>,
}

#[derive(Deserialize)]
pub struct NativeMessageProperties {
    pub emotions: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct NativeMessagesResponse {
    pub messages: Vec<NativeMessage>,
    #[serde(rename = "_metadata")]
    pub metadata: Option<ConversationsMetadata>,
}

#[derive(Deserialize)]
pub struct LegacyMessagesResponse {
    pub messages: Option<Vec<NativeMessage>>,
}

#[derive(Deserialize)]
pub struct NativeMembersResponse {
    pub members: Vec<NativeMember>,
}

#[derive(Deserialize)]
pub struct NativeMember {
    pub id: String,
    #[serde(rename = "userDisplayName")]
    pub user_display_name: Option<String>,
}

#[derive(Deserialize)]
pub struct GraphReaction {
    #[serde(rename = "reactionType")]
    pub reaction_type: Option<String>,
    pub user: Option<GraphReactionIdentitySet>,
}

#[derive(Deserialize)]
pub struct GraphReactionIdentitySet {
    pub user: Option<GraphMessageUser>,
}

#[derive(Deserialize)]
pub struct GraphChatListResponse {
    pub value: Vec<GraphChatListItem>,
    #[serde(rename = "@odata.nextLink")]
    pub next_link: Option<String>,
}

#[derive(Deserialize)]
pub struct GraphChatListItem {
    pub id: String,
    pub topic: Option<String>,
    #[serde(rename = "chatType")]
    pub chat_type: String,
    #[serde(rename = "lastUpdatedDateTime")]
    pub last_updated: Option<String>,
    pub members: Option<Vec<GraphChatListMember>>,
    #[serde(rename = "lastMessagePreview")]
    pub last_message_preview: Option<GraphMessagePreview>,
}

#[derive(Deserialize)]
pub struct GraphChatListMember {
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
}

#[derive(Deserialize)]
pub struct GraphMessagePreview {
    pub body: Option<GraphPreviewBody>,
}

#[derive(Deserialize)]
pub struct GraphPreviewBody {
    pub content: Option<String>,
}

#[derive(Deserialize)]
pub struct GraphMe {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub mail: Option<String>,
    #[serde(rename = "userPrincipalName")]
    pub user_principal_name: Option<String>,
}
