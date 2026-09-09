use serde_json::{json, Value};

use super::{call_modalities, ECHO_BOT_MRI, PERSONAL_CLIENT_ENDPOINT_CAPABILITIES};

pub struct PayloadContext<'a> {
    pub caller_mri: &'a str,
    pub caller_display_name: &'a str,
    pub endpoint_id: &'a str,
    pub participant_id: &'a str,
    pub thread_id: &'a str,
    pub caller_oid: &'a str,
    pub tenant_id: &'a str,
    pub message_id: &'a str,
}

fn cause_id<'a>(context: &'a PayloadContext<'a>) -> &'a str {
    &context.message_id[..8.min(context.message_id.len())]
}

fn from_participant(context: &PayloadContext<'_>) -> Value {
    json!({
        "id": context.caller_mri,
        "displayName": context.caller_display_name,
        "endpointId": context.endpoint_id,
        "participantId": context.participant_id,
        "languageId": "en-US"
    })
}

fn conversation_request(
    callback: &dyn Fn(&str) -> String,
    subject: Value,
    suppress_dialout: Option<bool>,
    enable_meetup_generation: bool,
    extended_links: bool,
) -> Value {
    let mut links = json!({
        "conversationEnd": callback("conversation/conversationEnd/"),
        "conversationUpdate": callback("conversation/conversationUpdate/"),
        "localParticipantUpdate": callback("conversation/localParticipantUpdate/"),
        "addParticipantSuccess": callback("conversation/addParticipantSuccess/"),
        "addParticipantFailure": callback("conversation/addParticipantFailure/"),
        "receiveMessage": callback("conversation/receiveMessage/")
    });
    if extended_links {
        links["addModalitySuccess"] = json!(callback("conversation/addModalitySuccess/"));
        links["addModalityFailure"] = json!(callback("conversation/addModalityFailure/"));
        links["confirmUnmute"] = json!(callback("conversation/confirmUnmute/"));
    }
    let mut request = json!({
        "subject": subject,
        "roster": {
            "type": "Delta",
            "rosterUpdate": callback("conversation/rosterUpdate/")
        },
        "properties": {
            "allowConversationWithoutHost": true,
            "enableGroupCallEventMessages": true,
            "enableGroupCallUpgradeMessage": false,
            "enableGroupCallMeetupGeneration": enable_meetup_generation
        },
        "links": links
    });
    if let Some(suppress_dialout) = suppress_dialout {
        request["conversationType"] = Value::Null;
        request["suppressDialout"] = json!(suppress_dialout);
    }
    request
}

fn endpoint_state(preheat: bool) -> Value {
    let mut properties = json!({
        "additionalEndpointProperties": {
            "infoShownInReportMode": "FullInformation"
        }
    });
    if preheat {
        properties["preheatProperties"] = json!(1);
    }
    json!({
        "endpointStateSequenceNumber": 0,
        "endpointProperties": properties
    })
}

fn call_invitation(
    callback: &dyn Fn(&str) -> String,
    modalities: &[&str],
    sdp_offer: &str,
) -> Value {
    json!({
        "callModalities": modalities,
        "replaces": null,
        "transferor": null,
        "links": {
            "progress": callback("call/progress/"),
            "mediaAnswer": callback("call/mediaAnswer/"),
            "acceptance": callback("call/acceptance/"),
            "redirection": callback("call/redirection/"),
            "end": callback("call/end/")
        },
        "clientContentForMediaController": {
            "controlVideoStreaming": callback("call/controlVideoStreaming/"),
            "csrcInfo": callback("call/csrcInfo/"),
            "dominantSpeakerInfo": callback("call/dominantSpeakerInfo/")
        },
        "pstnContent": {
            "emergencyCallCountry": "",
            "platformName": "teams-cli",
            "publicApiCall": false
        },
        "mediaContent": {
            "contentType": "application/sdp",
            "blob": sdp_offer
        }
    })
}

pub fn create_conversation_payload(
    context: &PayloadContext<'_>,
    callback: &dyn Fn(&str) -> String,
) -> Value {
    json!({
        "conversationRequest": conversation_request(callback, Value::Null, None, true, false),
        "groupContext": null,
        "groupChat": {
            "threadId": context.thread_id,
            "messageId": null
        },
        "participants": {
            "from": from_participant(context)
        },
        "capabilities": null,
        "endpointCapabilities": 73463,
        "clientEndpointCapabilities": 9336554,
        "endpointMetadata": { "holographicCapabilities": 3 },
        "meetingInfo": null,
        "endpointState": endpoint_state(false),
        "debugContent": {
            "ecsEtag": "\"0\"",
            "causeId": cause_id(context)
        }
    })
}

pub fn join_conversation_payload(
    context: &PayloadContext<'_>,
    callback: &dyn Fn(&str) -> String,
    sdp_offer: &str,
) -> Value {
    json!({
        "conversationRequest": conversation_request(callback, json!(""), Some(true), true, true),
        "groupContext": null,
        "groupChat": {
            "threadId": context.thread_id,
            "messageId": null
        },
        "participants": {
            "from": from_participant(context),
            "to": []
        },
        "capabilities": null,
        "endpointCapabilities": 73463,
        "clientEndpointCapabilities": 9336554,
        "endpointMetadata": { "holographicCapabilities": 3 },
        "meetingInfo": {
            "organizerId": context.caller_oid,
            "tenantId": context.tenant_id
        },
        "endpointState": endpoint_state(true),
        "callInvitation": call_invitation(callback, call_modalities(false), sdp_offer),
        "debugContent": {
            "ecsEtag": "\"0\"",
            "causeId": cause_id(context)
        }
    })
}

pub fn acceptance_payload(include_video: bool) -> Value {
    json!({
        "acceptedCallModalities": call_modalities(include_video),
        "endpointMetadata": {
            "isCallMediaCaptured": false,
            "isMicrophoneOn": false
        }
    })
}

pub fn media_answer_payload(sdp_answer: &str) -> Value {
    json!({
        "mediaContent": {
            "blob": sdp_answer,
            "contentType": "application/sdp"
        }
    })
}

pub fn media_renegotiation_answer_payload(sdp_answer: &str, media_leg_id: &str) -> Value {
    json!({
        "mediaAnswer": {
            "callModalities": ["Audio", "Video"],
            "mediaContent": {
                "blob": sdp_answer,
                "contentType": "application/sdp",
                "mediaLegId": media_leg_id,
                "escalationOccurring": false,
                "newOffer": false
            }
        }
    })
}

pub fn end_call_payload() -> Value {
    json!({})
}

fn cc_call_links(callback: &dyn Fn(&str) -> String) -> Value {
    json!({
        "links": {
            "mediaAcknowledgement": callback("call/mediaAcknowledgement/"),
            "rejection": callback("call/rejection/"),
            "acknowledgement": callback("call/acknowledgement/"),
            "mediaRenegotiation": callback("call/mediaRenegotiation/"),
            "replacement": callback("call/replacement/"),
            "progress": callback("call/progress/"),
            "mediaAnswer": callback("call/mediaAnswer/"),
            "newMediaOffer": callback("call/newMediaOffer/"),
            "redirection": callback("call/redirection/"),
            "balanceUpdate": callback("call/balanceUpdate/"),
            "acceptance": callback("call/acceptance/"),
            "controlVideoStreaming": callback("call/controlVideoStreaming/"),
            "dominantSpeakerInfo": callback("call/dominantSpeakerInfo/"),
            "csrcInfo": callback("call/csrcInfo/"),
            "end": callback("call/end/"),
            "retargetCompletion": callback("call/retargetCompletion/"),
            "transfer": callback("call/transfer/"),
            "transferAcceptance": callback("call/transferAcceptance/"),
            "transferCompletion": callback("call/transferCompletion/"),
            "holdCompletion": callback("call/holdCompletion/"),
            "resumeCompletion": callback("call/resumeCompletion/"),
            "call": callback("call/updateMediaDescriptions"),
            "monitorCompletion": callback("call/monitorCompletion/")
        },
        "clientContentForMediaController": {
            "controlVideoStreaming": callback("call/controlVideoStreaming/"),
            "csrcInfo": callback("call/csrcInfo/")
        }
    })
}

pub fn call_acceptance_acknowledgement_payload(callback: &dyn Fn(&str) -> String) -> Value {
    json!({
        "callAcceptanceAcknowledgement": cc_call_links(callback)
    })
}

pub fn cc_callback_registration_payload(callback: &dyn Fn(&str) -> String) -> Value {
    json!({
        "callAcceptanceAcknowledgement": cc_call_links(callback),
        "callParticipantUpdate": cc_call_links(callback)
    })
}

pub fn echo_call_payload(
    context: &PayloadContext<'_>,
    callback: &dyn Fn(&str) -> String,
    sdp_offer: &str,
) -> Value {
    json!({
        "conversationRequest": conversation_request(callback, json!(""), Some(true), false, true),
        "scenario": "UserInitiatedTestCall",
        "groupContext": null,
        "groupChat": {
            "threadId": context.thread_id,
            "messageId": null
        },
        "participants": {
            "from": from_participant(context),
            "to": []
        },
        "capabilities": null,
        "endpointCapabilities": 73463,
        "clientEndpointCapabilities": 9336554,
        "endpointMetadata": { "holographicCapabilities": 3 },
        "meetingInfo": null,
        "endpointState": endpoint_state(false),
        "callInvitation": call_invitation(
            callback,
            &["Audio", "Video", "ScreenViewer"],
            sdp_offer
        ),
        "debugContent": {
            "ecsEtag": "\"0\"",
            "causeId": cause_id(context)
        }
    })
}

pub fn echo_bot_invite_payload(
    context: &PayloadContext<'_>,
    callback: &dyn Fn(&str) -> String,
    participant_id: &str,
) -> Value {
    json!({
        "disableUnmute": false,
        "participants": {
            "from": from_participant(context),
            "to": [{
                "id": ECHO_BOT_MRI,
                "participantId": participant_id
            }]
        },
        "participantInvitationData": {},
        "replacementDetails": null,
        "groupContext": null,
        "groupChat": {
            "threadId": context.thread_id,
            "messageId": null
        },
        "links": {
            "addParticipantSuccess": callback("conversation/addParticipantSuccess/"),
            "addParticipantFailure": callback("conversation/addParticipantFailure/")
        }
    })
}

pub fn one_to_one_call_payload(
    context: &PayloadContext<'_>,
    callback: &dyn Fn(&str) -> String,
    sdp_offer: &str,
    callee_mri: Option<&str>,
    include_video: bool,
) -> Value {
    let recipients = callee_mri
        .map(|id| vec![json!({ "id": id })])
        .unwrap_or_default();
    json!({
        "conversationRequest": conversation_request(callback, json!(""), Some(false), false, true),
        "groupContext": null,
        "groupChat": {
            "threadId": context.thread_id,
            "messageId": null
        },
        "participants": {
            "from": from_participant(context),
            "to": recipients
        },
        "capabilities": null,
        "endpointCapabilities": 73463,
        "clientEndpointCapabilities": 9336554,
        "endpointMetadata": { "holographicCapabilities": 3 },
        "meetingInfo": null,
        "endpointState": endpoint_state(false),
        "callInvitation": call_invitation(callback, call_modalities(include_video), sdp_offer),
        "debugContent": {
            "ecsEtag": "\"0\"",
            "causeId": cause_id(context)
        }
    })
}

pub fn personal_one_to_one_call_payload(
    context: &PayloadContext<'_>,
    callback: &dyn Fn(&str) -> String,
    sdp_offer: &str,
    callee_mri: &str,
    callee_participant_id: &str,
    media_leg_id: &str,
    include_video: bool,
) -> Value {
    let mut links = json!({
        "conversationEnd": callback("conversation/conversationEnd/"),
        "conversationUpdate": callback("conversation/conversationUpdate/"),
        "localParticipantUpdate": callback("conversation/localParticipantUpdate/"),
        "addParticipantSuccess": callback("conversation/addParticipantSuccess/"),
        "addParticipantFailure": callback("conversation/addParticipantFailure/"),
        "addModalitySuccess": callback("conversation/addModalitySuccess/"),
        "addModalityFailure": callback("conversation/addModalityFailure/"),
        "confirmUnmute": callback("conversation/confirmUnmute/"),
        "receiveMessage": callback("conversation/receiveMessage/")
    });
    links
        .as_object_mut()
        .unwrap()
        .retain(|_, value| !value.is_null());
    json!({
        "conversationRequest": {
            "conversationType": null,
            "subject": null,
            "suppressDialout": false,
            "applicationType": "TFL",
            "roster": {
                "type": "Delta",
                "rosterUpdate": callback("conversation/rosterUpdate/")
            },
            "properties": {
                "allowConversationWithoutHost": true,
                "enableGroupCallEventMessages": true,
                "enableGroupCallUpgradeMessage": false,
                "enableGroupCallMeetupGeneration": false
            },
            "links": links
        },
        "contentSharing": null,
        "participants": {
            "from": from_participant(context),
            "to": [{
                "id": callee_mri,
                "participantId": callee_participant_id
            }]
        },
        "capabilities": null,
        "endpointCapabilities": 73463,
        "clientEndpointCapabilities": PERSONAL_CLIENT_ENDPOINT_CAPABILITIES,
        "endpointMetadata": { "holographicCapabilities": 3 },
        "groupContext": null,
        "groupChat": null,
        "meetingInfo": null,
        "meetingData": null,
        "endpointState": {
            "endpointStateSequenceNumber": 1,
            "endpointProperties": {
                "additionalEndpointProperties": {
                    "infoShownInReportMode": "FullInformation"
                }
            }
        },
        "callInvitation": {
            "callModalities": call_modalities(include_video),
            "replaces": null,
            "transferor": null,
            "clientTransferContext": null,
            "customContext": null,
            "links": {
                "progress": callback("call/progress/"),
                "mediaAnswer": callback("call/mediaAnswer/"),
                "acceptance": callback("call/acceptance/"),
                "redirection": callback("call/redirection/"),
                "end": callback("call/end/")
            },
            "clientContentForMediaController": {
                "controlVideoStreaming": callback("call/controlVideoStreaming/"),
                "csrcInfo": callback("call/csrcInfo/")
            },
            "pstnContent": {
                "emergencyCallCountry": "",
                "platformName": "SkypeSpaces/1415/teams-cli/TsCallingVersion=2026.31.01.16",
                "publicApiCall": false
            },
            "emergencyContent": null,
            "mediaContent": {
                "blob": sdp_offer,
                "contentType": "application/sdp-ngc-1.0",
                "requiredFeatures": "nonByPass",
                "mediaLegId": media_leg_id
            },
            "voicemailSettings": {},
            "locationContent": null,
            "networkContent": null,
            "areaContent": null
        },
        "debugContent": {
            "ecsEtag": "\"0\"",
            "causeId": cause_id(context)
        },
        "participantPropertyBag": {
            "aiVoiceConsent": {
                "value": { "aiVoiceConsentValue": "0" },
                "sequenceNumber": 0
            }
        }
    })
}

pub fn user_invite_payload(
    context: &PayloadContext<'_>,
    callback: &dyn Fn(&str) -> String,
    callee_mri: &str,
    participant_id: &str,
    include_video: bool,
) -> Value {
    let modalities = call_modalities(include_video);
    json!({
        "disableUnmute": false,
        "participants": {
            "from": from_participant(context),
            "to": [{
                "id": callee_mri,
                "participantId": participant_id
            }]
        },
        "participantInvitationData": {
            "callModalities": modalities,
            "callDirection": "Outgoing"
        },
        "callInvitation": {
            "callModalities": modalities,
            "replaces": null,
            "transferor": null
        },
        "replacementDetails": null,
        "groupContext": null,
        "groupChat": {
            "threadId": context.thread_id,
            "messageId": null
        },
        "links": {
            "addParticipantSuccess": callback("conversation/addParticipantSuccess/"),
            "addParticipantFailure": callback("conversation/addParticipantFailure/")
        }
    })
}

pub fn echo_bot_invite_url(conversation_controller: &str) -> String {
    user_invite_url(conversation_controller)
}

pub fn user_invite_url(conversation_controller: &str) -> String {
    if let Some(index) = conversation_controller.find('?') {
        let (path, query) = conversation_controller.split_at(index);
        format!("{}/add{}", path.trim_end_matches('/'), query)
    } else {
        format!("{}/add", conversation_controller.trim_end_matches('/'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context<'a>() -> PayloadContext<'a> {
        PayloadContext {
            caller_mri: "8:orgid:caller",
            caller_display_name: "Caller",
            endpoint_id: "endpoint",
            participant_id: "participant",
            thread_id: "19:thread",
            caller_oid: "caller",
            tenant_id: "tenant",
            message_id: "12345678-rest",
        }
    }

    fn callback(path: &str) -> String {
        format!("https://callback/{path}")
    }

    #[test]
    fn one_to_one_audio_payload_does_not_advertise_video() {
        let payload =
            one_to_one_call_payload(&context(), &callback, "sdp", Some("8:orgid:callee"), false);
        assert_eq!(
            payload["callInvitation"]["callModalities"],
            json!(["Audio"])
        );
    }

    #[test]
    fn one_to_one_video_payload_advertises_audio_and_video() {
        let payload =
            one_to_one_call_payload(&context(), &callback, "sdp", Some("8:orgid:callee"), true);
        assert_eq!(
            payload["callInvitation"]["callModalities"],
            json!(["Audio", "Video"])
        );
    }

    #[test]
    fn one_to_one_payload_targets_peer_in_initial_request() {
        let payload =
            one_to_one_call_payload(&context(), &callback, "sdp", Some("8:orgid:callee"), false);
        assert_eq!(
            payload["participants"]["to"],
            json!([{ "id": "8:orgid:callee" }])
        );
    }

    #[test]
    fn one_to_one_payload_can_leave_recipient_empty_for_test_bot_invite() {
        let payload = one_to_one_call_payload(&context(), &callback, "sdp", None, false);
        assert_eq!(payload["participants"]["to"], json!([]));
    }

    #[test]
    fn user_invite_payload_targets_canonical_peer() {
        let payload = user_invite_payload(
            &context(),
            &callback,
            "8:orgid:callee",
            "participant-callee",
            true,
        );
        assert_eq!(
            payload["participants"]["to"],
            json!([{
                "id": "8:orgid:callee",
                "participantId": "participant-callee"
            }])
        );
        assert_eq!(
            payload["participantInvitationData"]["callModalities"],
            json!(["Audio", "Video"])
        );
    }

    #[test]
    fn media_renegotiation_answer_keeps_media_leg_and_video_sdp() {
        let payload =
            media_renegotiation_answer_payload("v=0\r\nm=video 3480 RTP/SAVP 122\r\n", "leg-1");
        assert_eq!(
            payload["mediaAnswer"]["callModalities"],
            json!(["Audio", "Video"])
        );
        assert_eq!(
            payload["mediaAnswer"]["mediaContent"]["mediaLegId"],
            "leg-1"
        );
        assert_eq!(
            payload["mediaAnswer"]["mediaContent"]["contentType"],
            "application/sdp"
        );
        assert!(payload["mediaAnswer"]["mediaContent"]["blob"]
            .as_str()
            .is_some_and(|sdp| sdp.contains("m=video")));
    }

    #[test]
    fn echo_invite_fallback_uses_add_endpoint() {
        assert_eq!(
            echo_bot_invite_url("https://example.test/conv/abc?view=caller"),
            "https://example.test/conv/abc/add?view=caller"
        );
    }
}
