pub mod models;

pub const GRAPH_BASE: &str = "https://graph.microsoft.com/v1.0";
pub const GRAPH_BULK_PRESENCE_PATH: &str = "/communications/getPresencesByUserId";
pub const PRESENCE_BASE: &str = "https://presence.teams.microsoft.com";
pub const PRESENCE_GET_PATH: &str = "/v1/presence/getpresence/";
pub const DEFAULT_CHAT_SERVICE: &str = "https://amer.ng.msg.teams.microsoft.com";
pub const CHATSVCAGG_BASE: &str = "https://chatsvcagg.teams.microsoft.com";
pub const CSA_CONVERSATIONS_BASE: &str =
    "https://teams.microsoft.com/api/csa/api/v1/teams/users/ME/conversations";
pub const DEFAULT_AMS_SERVICE: &str = "https://api.asm.skype.com";
pub const AMS_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.0.0 Safari/537.36 Edg/144.0.0.0";
pub const SKYPE_TOKEN_HEADER: &str = "X-Skypetoken";
pub const NATIVE_AUTH_HEADER: &str = "Authentication";
pub const CSA_CLIENT_VERSION_HEADER: &str = "x-ms-client-version";
pub const CSA_CLIENT_VERSION: &str = "1416/1.0.0.2024050301";
pub const REPLY_SCHEMA: &str = "http://schema.skype.com/reply";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageImageAuth {
    Graph,
    Skype,
    Public,
}

pub fn native_auth_value(token: &str) -> String {
    format!("skypetoken={token}")
}

pub fn ams_auth_value(token: &str) -> String {
    format!("skype_token {token}")
}

pub fn asm_cookie_value(token: &str) -> String {
    format!("skypetoken_asm={token}")
}

pub fn csa_conversations_url(limit: usize) -> String {
    format!("{CSA_CONVERSATIONS_BASE}?view=mychats&pageSize={limit}")
}

pub fn graph_user_path(user_id: &str) -> String {
    format!("/users/{user_id}")
}

pub fn graph_user_presence_path(user_id: &str) -> String {
    format!("/users/{user_id}/presence")
}

pub fn message_image_auth(host: &str) -> Option<MessageImageAuth> {
    let host = host.to_ascii_lowercase();
    if host == "graph.microsoft.com" {
        return Some(MessageImageAuth::Graph);
    }
    if host.ends_with(".teams.microsoft.com")
        || host.ends_with(".skype.com")
        || host == "smba.trafficmanager.net"
        || host == "botapi.skype.com"
    {
        return Some(MessageImageAuth::Skype);
    }
    if host == "giphy.com"
        || host.ends_with(".giphy.com")
        || host == "tenor.com"
        || host.ends_with(".tenor.com")
    {
        return Some(MessageImageAuth::Public);
    }
    None
}

pub fn audio_message_content(objects_url: &str, object_id: &str) -> String {
    let object_url = format!("{objects_url}/{object_id}");
    format!(
        "<URIObject type=\"Audio.1/Message.1\" url_thumbnail=\"{object_url}/views/thumbnail\" uri=\"{object_url}\"><Title>Audio Message</Title><a href=\"https://login.skype.com/login/sso?go=webclient.xmm&amp;am={object_id}\">Play</a><OriginalName v=\"voice-message.m4a\"/></URIObject>"
    )
}

pub fn contains_reply_schema(value: &str) -> bool {
    value.to_ascii_lowercase().contains(REPLY_SCHEMA)
}

pub fn reaction_user_name(user: &serde_json::Value) -> Option<&str> {
    user.get("displayName")
        .or_else(|| user.get("name"))
        .and_then(serde_json::Value::as_str)
}

pub fn hosted_image_url(messages_path: &str, message_id: &str, content_id: &str) -> String {
    format!("{GRAPH_BASE}{messages_path}/{message_id}/hostedContents/{content_id}/$value")
}

#[cfg(test)]
mod tests {
    use super::{message_image_auth, MessageImageAuth};

    #[test]
    fn teams_gif_hosts_are_public_image_sources() {
        assert_eq!(
            message_image_auth("media.giphy.com"),
            Some(MessageImageAuth::Public)
        );
        assert_eq!(
            message_image_auth("media.tenor.com"),
            Some(MessageImageAuth::Public)
        );
        assert_eq!(message_image_auth("example.com"), None);
    }
}
