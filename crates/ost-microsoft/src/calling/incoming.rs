use base64::Engine;
use serde::Deserialize;
use std::io::Read as _;

#[derive(Debug, Clone, Deserialize)]
pub struct CallLinks {
    pub acceptance: Option<String>,
    pub end: Option<String>,
    #[serde(rename = "mediaAnswer")]
    pub media_answer: Option<String>,
    #[serde(rename = "p2pForkNotification")]
    pub p2p_fork_notification: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MediaContent {
    pub blob: Option<String>,
    #[serde(rename = "contentType")]
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Participant {
    pub id: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "endpointId")]
    pub endpoint_id: Option<String>,
    #[serde(rename = "languageId")]
    pub language_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Participants {
    pub from: Option<Participant>,
    pub to: Option<Vec<Participant>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConversationLinks {
    #[serde(rename = "conversationEnd")]
    pub conversation_end: Option<String>,
    #[serde(rename = "conversationUpdate")]
    pub conversation_update: Option<String>,
    #[serde(rename = "localParticipantUpdate")]
    pub local_participant_update: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConversationRequest {
    pub links: Option<ConversationLinks>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DebugContent {
    #[serde(rename = "callId")]
    pub call_id: Option<String>,
    #[serde(rename = "endpointId")]
    pub endpoint_id: Option<String>,
    #[serde(rename = "operationId")]
    pub operation_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallInvitation {
    #[serde(rename = "callModalities")]
    pub call_modalities: Option<Vec<String>>,
    pub links: Option<CallLinks>,
    #[serde(rename = "mediaContent")]
    pub media_content: Option<MediaContent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupChat {
    #[serde(rename = "threadId")]
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallEnd {
    pub phrase: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallNotification {
    #[serde(rename = "callEnd")]
    pub call_end: Option<CallEnd>,
    #[serde(rename = "conversationEnd")]
    pub conversation_end: Option<CallEnd>,
    #[serde(rename = "groupChat")]
    pub group_chat: Option<GroupChat>,
    #[serde(rename = "callInvitation")]
    pub call_invitation: Option<CallInvitation>,
    pub participants: Option<Participants>,
    #[serde(rename = "conversationRequest")]
    pub conversation_request: Option<ConversationRequest>,
    #[serde(rename = "debugContent")]
    pub debug_content: Option<DebugContent>,
}

pub fn parse_call_notification(json_str: &str) -> Option<CallNotification> {
    parse_call_event(json_str).filter(|event| event.call_invitation.is_some())
}

pub fn parse_call_event(json_str: &str) -> Option<CallNotification> {
    let value: serde_json::Value = serde_json::from_str(json_str).ok()?;
    find_call_notification(&value)
}

fn find_call_notification(value: &serde_json::Value) -> Option<CallNotification> {
    if let Some(body) = value.get("body").and_then(|body| body.as_str()) {
        let gzip = value
            .get("headers")
            .and_then(|headers| headers.as_object())
            .is_some_and(|headers| {
                headers.iter().any(|(name, value)| {
                    name.eq_ignore_ascii_case("X-Microsoft-Skype-Content-Encoding")
                        && value
                            .as_str()
                            .is_some_and(|value| value.eq_ignore_ascii_case("gzip"))
                })
            });
        if gzip {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(body)
                .ok()?;
            let mut decoded = String::new();
            flate2::read::GzDecoder::new(&bytes[..])
                .read_to_string(&mut decoded)
                .ok()?;
            return parse_call_event(&decoded);
        }
    }
    if value
        .get("callInvitation")
        .is_some_and(|value| value.is_object())
        || value.get("callEnd").is_some_and(|value| value.is_object())
        || value
            .get("conversationEnd")
            .is_some_and(|value| value.is_object())
    {
        return serde_json::from_value(value.clone()).ok();
    }
    match value {
        serde_json::Value::Object(object) => object.iter().find_map(|(key, child)| {
            if let Some(encoded) = child.as_str() {
                if key == "cp" {
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) {
                        let mut decoder = flate2::read::GzDecoder::new(&bytes[..]);
                        let mut decoded = String::new();
                        if decoder.read_to_string(&mut decoded).is_ok() {
                            if let Ok(decoded) = serde_json::from_str::<serde_json::Value>(&decoded)
                            {
                                if let Some(notification) = find_call_notification(&decoded) {
                                    return Some(notification);
                                }
                            }
                        }
                    }
                } else if key == "gp" {
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) {
                        if let Ok(decoded) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                            if let Some(notification) = find_call_notification(&decoded) {
                                return Some(notification);
                            }
                        }
                    }
                } else if encoded.trim_start().starts_with('{')
                    || encoded.trim_start().starts_with('[')
                {
                    if let Ok(decoded) = serde_json::from_str::<serde_json::Value>(encoded) {
                        if let Some(notification) = find_call_notification(&decoded) {
                            return Some(notification);
                        }
                    }
                }
            }
            find_call_notification(child)
        }),
        serde_json::Value::Array(items) => items.iter().find_map(find_call_notification),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_call_notification;
    use base64::Engine;
    use std::io::Write as _;

    #[test]
    fn terminal_events_are_not_invitations() {
        assert!(parse_call_notification(r#"{"callEnd":{"phrase":"Remote hangup"}}"#).is_none());
        assert!(parse_call_notification(r#"{"conversationEnd":{}}"#).is_none());
        assert!(parse_call_notification(r#"{"callInvitation":null}"#).is_none());
    }

    #[test]
    fn parses_gzip_incoming_audio_and_video_notifications() {
        for modalities in [
            serde_json::json!(["Audio"]),
            serde_json::json!(["Audio", "Video"]),
        ] {
            let inner = serde_json::json!({
                "callInvitation": {"callModalities": modalities},
                "debugContent": {"callId": "incoming-call"}
            });
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(inner.to_string().as_bytes()).unwrap();
            let body = base64::engine::general_purpose::STANDARD.encode(encoder.finish().unwrap());
            let payload = serde_json::json!({"args": [{"headers": {
                "X-Microsoft-Skype-Content-Encoding": "gzip"
            }, "body": body}]})
            .to_string();
            let notification = parse_call_notification(&payload).expect("gzip invitation");
            assert_eq!(
                notification
                    .call_invitation
                    .unwrap()
                    .call_modalities
                    .unwrap(),
                serde_json::from_value::<Vec<String>>(modalities).unwrap()
            );
        }
    }

    #[test]
    fn parses_call_notification_from_compressed_cp() {
        let inner = r#"{"callInvitation":{"callModalities":["Audio"]},"debugContent":{"callId":"call-cp"}}"#;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(inner.as_bytes()).unwrap();
        let cp = base64::engine::general_purpose::STANDARD.encode(encoder.finish().unwrap());
        let payload = serde_json::json!({"cp": cp}).to_string();

        let notification = parse_call_notification(&payload).unwrap();
        assert_eq!(
            notification
                .debug_content
                .and_then(|debug| debug.call_id)
                .as_deref(),
            Some("call-cp")
        );
    }

    #[test]
    fn parses_call_notification_from_base64_gp() {
        let inner = r#"{"callInvitation":{"callModalities":["Audio"]},"debugContent":{"callId":"call-gp"}}"#;
        let gp = base64::engine::general_purpose::STANDARD.encode(inner);
        let payload = serde_json::json!({"gp": gp}).to_string();

        let notification = parse_call_notification(&payload).unwrap();
        assert_eq!(
            notification
                .debug_content
                .and_then(|debug| debug.call_id)
                .as_deref(),
            Some("call-gp")
        );
    }

    #[test]
    fn parses_call_notification_nested_in_socketio_args_and_string_body() {
        let payload = r#"{"name":"event","args":[{"body":"{\"callInvitation\":{\"callModalities\":[\"Audio\"]},\"participants\":{\"from\":{\"displayName\":\"Ada\"}},\"debugContent\":{\"callId\":\"call-1\"}}"}]}"#;
        let notification = parse_call_notification(payload).expect("nested call should parse");
        assert_eq!(
            notification
                .debug_content
                .and_then(|debug| debug.call_id)
                .as_deref(),
            Some("call-1")
        );
    }
}
