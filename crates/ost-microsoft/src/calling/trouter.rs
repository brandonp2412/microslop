use serde::{Deserialize, Serialize};

use super::TROUTER_CLIENT_CONTEXT;

pub const DEFAULT_REGISTRAR_URL: &str =
    "https://teams.microsoft.com/registrar/prod/V2/registrations";
pub const MESSAGE_REGISTRATION_TTL_SECONDS: u64 = 86_400;
pub const ACTIVE_FRAME: &str = r#"5:1+::{"name":"user.activity","args":[{"state":"active"}]}"#;

pub fn socketio_acknowledgement(frame: &str) -> Option<String> {
    let rest = frame.strip_prefix("5:")?;
    let (raw_id, _) = rest.split_once(':')?;
    let id = raw_id.strip_suffix('+').unwrap_or(raw_id);
    (!id.is_empty() && id.chars().all(|character| character.is_ascii_digit()))
        .then(|| format!("6:::{id}+[]"))
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TrouterConnectParams {
    pub sr: String,
    pub issuer: String,
    pub sp: String,
    pub se: String,
    pub st: String,
    pub sig: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrouterSession {
    pub socketio: String,
    pub surl: String,
    pub connectparams: TrouterConnectParams,
    pub ccid: Option<String>,
    #[serde(default)]
    pub registrar_url: Option<String>,
}

impl TrouterSession {
    pub fn session_url(&self, endpoint_id: &str) -> String {
        self.socket_url("socket.io/1/", endpoint_id)
    }

    pub fn ws_url(&self, session_id: &str, endpoint_id: &str) -> String {
        self.websocket_url(session_id, endpoint_id)
    }

    pub fn websocket_url(&self, session_id: &str, endpoint_id: &str) -> String {
        self.socket_url(&format!("socket.io/1/websocket/{session_id}"), endpoint_id)
            .replacen("https://", "wss://", 1)
    }

    fn socket_url(&self, path: &str, endpoint_id: &str) -> String {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in [
            ("v", "v4"),
            ("sr", self.connectparams.sr.as_str()),
            ("issuer", self.connectparams.issuer.as_str()),
            ("sp", self.connectparams.sp.as_str()),
            ("se", self.connectparams.se.as_str()),
            ("st", self.connectparams.st.as_str()),
            ("sig", self.connectparams.sig.as_str()),
            ("tc", TROUTER_CLIENT_CONTEXT),
            ("con_num", "0_1"),
            ("auth", "true"),
            ("timeout", "40"),
            ("epid", endpoint_id),
        ] {
            query.append_pair(key, value);
        }
        if let Some(ccid) = &self.ccid {
            query.append_pair("ccid", ccid);
        }
        format!(
            "{}/{path}?{}",
            self.socketio.trim_end_matches('/'),
            query.finish()
        )
    }
}

pub fn authentication_payload<T: Serialize>(
    access_token: &str,
    connectparams: &T,
) -> serde_json::Value {
    serde_json::json!({
        "name": "user.authenticate",
        "args": [{
            "headers": {
                "X-Ms-Test-User": "False",
                "Authorization": format!("Bearer {access_token}"),
                "X-MS-Migration": "True"
            },
            "connectparams": connectparams
        }]
    })
}

pub fn message_registration_payload(registration_id: &str, path: &str) -> serde_json::Value {
    serde_json::json!({
        "clientDescription": {
            "appId": "TeamsCDLWebWorker",
            "aesKey": "",
            "languageId": "en-US",
            "platform": "edge",
            "templateKey": "TeamsCDLWebWorker_2.6",
            "platformUIVersion": "49/1.0.0",
            "productContext": "TFL"
        },
        "registrationId": registration_id,
        "nodeId": "",
        "transports": {
            "TROUTER": [{
                "context": "TFL",
                "path": path,
                "ttl": MESSAGE_REGISTRATION_TTL_SECONDS
            }]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::socketio_acknowledgement;

    #[test]
    fn builds_socketio_v1_acknowledgements_with_or_without_plus_suffix() {
        assert_eq!(
            socketio_acknowledgement(r#"5:8+::{"name":"trouter.message_loss"}"#).as_deref(),
            Some("6:::8+[]")
        );
        assert_eq!(
            socketio_acknowledgement(r#"5:42::{"name":"call.acceptance"}"#).as_deref(),
            Some("6:::42+[]")
        );
        assert_eq!(socketio_acknowledgement(r#"5:::{"name":"event"}"#), None);
    }
}
