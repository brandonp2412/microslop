use serde_json::{json, Value};

use super::RECORDER_BOT_MRI;

pub const CACHED_RESPONSE_HEADER: &str = "x-microsoft-skype-cached-response";
pub const RECORDER_HOST_SUFFIX: &str = "callrecorder.teams.cloud.microsoft";
pub const RECORDER_HOST_PREFIX: &str = "aks-prod-";
pub const FLIGHTPROXY_RECORDER_PREFIX: &str =
    "https://api.flightproxy.teams.microsoft.com/api/v2/ep/";

pub struct RecorderPayloadContext<'a> {
    pub caller_mri: &'a str,
    pub display_name: &'a str,
    pub endpoint_id: &'a str,
    pub participant_id: &'a str,
    pub chain_id: &'a str,
    pub thread_id: &'a str,
    pub recorder_token: &'a str,
}

fn recorder_features() -> Value {
    json!({
        "enablePPTSharing": true,
        "intermediateLiveCaptions": false,
        "actionItemsEnabled": false,
        "enableEmailAndMeetingLanguageModel": true,
        "ceoSummit": false,
        "useUnmixedAudio": true,
        "enableTranscriptMeetingChaptering": false
    })
}

fn recording_features() -> Value {
    let mut features = recorder_features();
    features["recordingMode"] = json!("Normal");
    features
}

pub fn add_recorder_payload(
    context: &RecorderPayloadContext<'_>,
    bot_participant_id: &str,
    callback: &dyn Fn(&str) -> String,
) -> Value {
    json!({
        "disableUnmute": false,
        "participants": {
            "from": {
                "id": context.caller_mri,
                "displayName": context.display_name,
                "endpointId": context.endpoint_id,
                "participantId": context.participant_id,
                "languageId": "en-US"
            },
            "to": [{
                "id": RECORDER_BOT_MRI,
                "participantId": bot_participant_id
            }]
        },
        "participantInvitationData": {
            "botData": {
                "meetingTitle": "",
                "clientInfo": "Teams-R4",
                "callId": context.chain_id,
                "threadId": context.thread_id,
                "recorderFeatures": recorder_features(),
                "mode": "RecordingAndTranscription",
                "iCalUid": null,
                "consumerType": "Teams",
                "spokenLanguage": "en-us",
                "initiatorUserToken": context.recorder_token,
                "exchangeId": null,
                "meetingOrganizer": context.display_name
            }
        },
        "replacementDetails": null,
        "links": {
            "addParticipantSuccess": callback("conversation/addParticipantSuccess/"),
            "addParticipantFailure": callback("conversation/addParticipantFailure/")
        },
        "debugContent": {}
    })
}

pub fn recorder_command_url(base: &str, conversation_id: &str) -> String {
    format!("{base}/v2/oncommand/{conversation_id}")
}

pub fn start_transcription_payload(
    timestamp: &str,
    caller_mri: &str,
    participant_id: &str,
) -> Value {
    json!({
        "timestamp": timestamp,
        "participantMri": caller_mri,
        "participantLegId": participant_id,
        "action": "start",
        "mode": "transcription",
        "processingModes": ["closedCaptions"],
        "participantSkypeToken": ""
    })
}

pub fn start_recording_payload(
    timestamp: &str,
    caller_mri: &str,
    participant_id: &str,
    file_name: &str,
    correlation_id: &str,
) -> Value {
    json!({
        "timestamp": timestamp,
        "participantMri": caller_mri,
        "participantLegId": participant_id,
        "action": "start",
        "processingModes": ["recording", "realTimeTranscript"],
        "actionParameters": {
            "recordingFeatures": recording_features(),
            "recordingStorageSettings": [{
                "StorageType": "OnedriveForBusiness",
                "StorageLocation": "Recordings",
                "FileName": file_name,
                "GroupId": null
            }],
            "correlationId": correlation_id,
            "meetingTitle": "Meeting in \"av-test\"",
            "spokenLanguage": "en-us",
            "type": "start"
        },
        "participantSkypeToken": ""
    })
}

pub fn stop_recording_payload(timestamp: &str, caller_mri: &str, participant_id: &str) -> Value {
    json!({
        "timestamp": timestamp,
        "participantMri": caller_mri,
        "participantLegId": participant_id,
        "action": "stop",
        "processingModes": ["recording", "realTimeTranscript"],
        "participantSkypeToken": ""
    })
}

pub fn stop_transcription_payload(
    timestamp: &str,
    caller_mri: &str,
    participant_id: &str,
) -> Value {
    json!({
        "timestamp": timestamp,
        "participantMri": caller_mri,
        "participantLegId": participant_id,
        "action": "stop",
        "mode": "transcription",
        "processingModes": ["closedCaptions"],
        "participantSkypeToken": ""
    })
}

pub fn is_recorder_payload(payload: &str) -> bool {
    payload.contains(RECORDER_BOT_MRI)
        || payload.contains(RECORDER_HOST_SUFFIX)
        || payload.contains("addParticipantSuccess")
}

pub fn recorder_base_from_payload(payload: &str) -> Option<String> {
    let suffix = payload.find(RECORDER_HOST_SUFFIX)?;
    let host_start = payload[..suffix].rfind(RECORDER_HOST_PREFIX)?;
    let after = &payload[suffix..];
    let end = after.find(['/', '"', ' '])?;
    Some(format!(
        "{FLIGHTPROXY_RECORDER_PREFIX}{}",
        &payload[host_start..suffix + end]
    ))
}
