//! Call recording via recorder bot injection.
//!
//! Implements the recording flow:
//! 1. Add recorder bot as call participant via conversation API
//! 2. Start transcription via call recorder service
//! 3. Start recording via call recorder service

use anyhow::{bail, Context, Result};
use chrono::Utc;
use std::time::Duration;

use super::call_test::extract_call_payload;
use super::signaling::{self, trouter_callback, ConversationCallParams};
use crate::trouter::websocket::TrouterSocket;
use ost_microsoft::calling as microsoft_calling;

/// Teams web client waits ~11s, but we use 2s to race against solo-call teardown.
const TRANSCRIPTION_TO_RECORDING_DELAY_SECS: u64 = 2;

pub struct RecordingRequest<'a> {
    pub caller_mri: &'a str,
    pub participant_id: &'a str,
    pub endpoint_id: &'a str,
    pub chain_id: &'a str,
    pub message_id: &'a str,
    pub thread_id: &'a str,
    pub display_name: &'a str,
    pub trouter_surl: &'a str,
    pub ic3_token: &'a str,
    pub recorder_token: &'a str,
    pub skype_token: &'a str,
    pub conversation_controller: &'a str,
    pub add_participant_url_override: Option<&'a str>,
}

pub struct RecordingParams<'a> {
    pub caller_mri: &'a str,
    pub participant_id: &'a str,
    pub endpoint_id: &'a str,
    pub chain_id: &'a str,
    pub message_id: &'a str,
    pub thread_id: &'a str,
    pub display_name: &'a str,
    pub trouter_surl: &'a str,
    pub ic3_token: &'a str,
    pub recorder_token: &'a str,
    pub skype_token: &'a str,
    pub conversation_id: &'a str,
    pub add_participant_url: &'a str,
}

/// Step 1: Add the recorder bot as a call participant.
pub async fn add_recorder_bot(
    http: &reqwest::Client,
    params: &RecordingParams<'_>,
) -> Result<String> {
    let bot_participant_id = uuid::Uuid::new_v4().to_string();

    // Trouter callback URLs for add-participant success/failure notifications
    let tc = |path: &str| trouter_callback(params.trouter_surl, params.endpoint_id, path);

    // Payload matches real Teams web client capture (reqid 4222).
    // has debugContent, uses /addParticipant endpoint.
    let payload = microsoft_calling::add_recorder_payload(
        &microsoft_calling::RecorderPayloadContext {
            caller_mri: params.caller_mri,
            display_name: params.display_name,
            endpoint_id: params.endpoint_id,
            participant_id: params.participant_id,
            chain_id: params.chain_id,
            thread_id: params.thread_id,
            recorder_token: params.recorder_token,
        },
        &bot_participant_id,
        &tc,
    );

    // Generate a fresh message-id for this request.  The conv server uses
    // x-microsoft-skype-message-id for deduplication; reusing the call-placement
    let recorder_message_id = uuid::Uuid::new_v4().to_string();

    tracing::info!(
        "Adding recorder bot -> POST {} (msg_id={})",
        params.add_participant_url,
        recorder_message_id
    );
    tracing::debug!(
        "Recorder bot payload: {}",
        serde_json::to_string_pretty(&payload).unwrap_or_default()
    );

    let resp = http
        .post(params.add_participant_url)
        .header("Authorization", format!("Bearer {}", params.ic3_token))
        .header("Content-Type", "application/json")
        .header(microsoft_calling::CHAIN_ID_HEADER, params.chain_id)
        .header(microsoft_calling::MESSAGE_ID_HEADER, &recorder_message_id)
        .header(
            microsoft_calling::CLIENT_HEADER,
            microsoft_calling::SKYPE_CLIENT_HEADER,
        )
        .header(
            microsoft_calling::PARTITION_HEADER,
            microsoft_calling::TEAMS_PARTITION,
        )
        .header(
            microsoft_calling::REGION_HEADER,
            microsoft_calling::TEAMS_REGION,
        )
        .header(
            microsoft_calling::RING_HEADER,
            microsoft_calling::TEAMS_RING,
        )
        .header(
            microsoft_calling::MIGRATION_HEADER,
            microsoft_calling::MIGRATION_VALUE,
        )
        .json(&payload)
        .send()
        .await
        .context("Failed to POST add recorder bot")?;

    let status = resp.status();
    let resp_headers = resp.headers().clone();
    let body = resp.text().await.unwrap_or_default();

    for (k, v) in resp_headers.iter() {
        tracing::debug!(
            "addParticipant resp header: {}: {}",
            k,
            v.to_str().unwrap_or("(binary)")
        );
    }
    if resp_headers
        .get(microsoft_calling::CACHED_RESPONSE_HEADER)
        .is_some()
    {
        tracing::warn!("Server returned cached response — message-id may have been reused");
    }

    if !status.is_success() {
        bail!(
            "Add recorder bot failed ({}): {}",
            status,
            &body[..body.len().min(500)]
        );
    }

    tracing::info!("Recorder bot added ({}): {} bytes", status, body.len());
    tracing::debug!(
        "Recorder bot response (first 2000): {}",
        &body[..body.len().min(2000)]
    );
    if let Ok(()) = std::fs::write("/tmp/add_recorder_response.json", &body) {
        tracing::info!("Full add-recorder response saved to /tmp/add_recorder_response.json");
    }
    Ok(body)
}

/// Step 2: Start transcription via the call recorder service.
async fn start_transcription_at(
    http: &reqwest::Client,
    params: &RecordingParams<'_>,
    recorder_base: &str,
) -> Result<()> {
    let url = microsoft_calling::recorder_command_url(recorder_base, params.conversation_id);
    let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let payload = microsoft_calling::start_transcription_payload(
        &timestamp,
        params.caller_mri,
        params.participant_id,
    );

    tracing::info!("Starting transcription -> POST {}", url);

    let resp = http
        .post(&url)
        .header("Authorization", format!("Bearer {}", params.recorder_token))
        .header(microsoft_calling::SKYPE_TOKEN_HEADER, params.skype_token)
        .header(microsoft_calling::CHAIN_ID_HEADER, params.chain_id)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .context("Failed to POST start transcription")?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        bail!(
            "Start transcription failed ({}): {}",
            status,
            &body[..body.len().min(500)]
        );
    }

    tracing::info!("Transcription started ({})", status);
    Ok(())
}

/// Step 3: Start recording via the call recorder service.
async fn start_recording_at(
    http: &reqwest::Client,
    params: &RecordingParams<'_>,
    recorder_base: &str,
) -> Result<()> {
    let url = microsoft_calling::recorder_command_url(recorder_base, params.conversation_id);
    let correlation_id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now();
    let date_str = now.format("%Y%m%dT%H%M%S").to_string();
    let file_name = format!("Meeting in \"av-test\"-{}-Meeting Recording", date_str);
    let timestamp = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let payload = microsoft_calling::start_recording_payload(
        &timestamp,
        params.caller_mri,
        params.participant_id,
        &file_name,
        &correlation_id,
    );

    tracing::info!("Starting recording -> POST {}", url);

    let resp = http
        .post(&url)
        .header("Authorization", format!("Bearer {}", params.recorder_token))
        .header(microsoft_calling::SKYPE_TOKEN_HEADER, params.skype_token)
        .header(microsoft_calling::CHAIN_ID_HEADER, params.chain_id)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .context("Failed to POST start recording")?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        bail!(
            "Start recording failed ({}): {}",
            status,
            &body[..body.len().min(500)]
        );
    }

    tracing::info!("Recording started ({})", status);
    Ok(())
}

/// Run the full recording flow: add bot, start transcription, start recording.
///
/// After adding the recorder bot, waits for the `addParticipantSuccess` Trouter
/// callback which contains the recorder service URL and recording session ID.
/// These are dynamically assigned per call and cannot be hardcoded.
pub async fn start_call_recording(
    http: &reqwest::Client,
    ws: &mut TrouterSocket,
    request: RecordingRequest<'_>,
) -> Result<RecordingSession> {
    // Use the exact addParticipant URL from the epconv response if available,
    // otherwise derive it from conversationController as a fallback.
    let add_url = match request.add_participant_url_override {
        Some(url) => {
            tracing::info!("Using addParticipant URL from epconv response: {}", url);
            url.to_string()
        }
        None => {
            let derived = derive_add_participant_url_for_bot(request.conversation_controller);
            tracing::info!(
                "Derived addParticipant URL (no link in response): {}",
                derived
            );
            derived
        }
    };

    let placeholder_conv_id = extract_conversation_id(request.conversation_controller)
        .unwrap_or_else(|| "unknown".to_string());

    let params = RecordingParams {
        caller_mri: request.caller_mri,
        participant_id: request.participant_id,
        endpoint_id: request.endpoint_id,
        chain_id: request.chain_id,
        message_id: request.message_id,
        thread_id: request.thread_id,
        display_name: request.display_name,
        trouter_surl: request.trouter_surl,
        ic3_token: request.ic3_token,
        recorder_token: request.recorder_token,
        skype_token: request.skype_token,
        conversation_id: &placeholder_conv_id,
        add_participant_url: &add_url,
    };

    tracing::info!("Starting recording flow (add URL: {})", add_url);

    // including the recorder bot's participant entry with its conversationController URL.
    let add_response = add_recorder_bot(http, &params).await?;

    // Step 2: Extract recorder service URL from the add-participant response body.
    // The response is a conversationUpdate JSON containing participants, one of which
    // is the recorder bot with a conversationController URL pointing at the recorder service.
    let recorder_info = serde_json::from_str::<serde_json::Value>(&add_response)
        .ok()
        .and_then(|v| extract_recorder_from_payload(&v));

    // Fallback: try Trouter callback if the HTTP response didn't contain recorder info
    let recorder_info = match recorder_info {
        Some(info) => Some(info),
        None => {
            tracing::info!("Recorder URL not in HTTP response, waiting on Trouter (30s)...");
            // Build ConversationCallParams for acknowledging callAcceptance frames
            let conv_params = ConversationCallParams {
                call_token: params.ic3_token,
                personal: false,
                trouter_surl: params.trouter_surl,
                caller_mri: params.caller_mri,
                caller_display_name: params.display_name,
                endpoint_id: params.endpoint_id,
                participant_id: params.participant_id,
                thread_id: params.thread_id,
                chain_id: params.chain_id,
                message_id: params.message_id,
                caller_oid: "",
                tenant_id: "",
            };
            wait_for_recorder_info(ws, Duration::from_secs(30), http, &conv_params).await
        }
    };

    let (recorder_base, recorder_conv_id) = match recorder_info {
        Some((base, cid)) => {
            tracing::info!(
                "Recorder service discovered: base={}, conv_id={}",
                base,
                cid
            );
            (base, cid)
        }
        None => {
            bail!(
                "Cannot determine recorder service URL from HTTP response or Trouter callback. \
                 The recorder endpoint and session ID are dynamically assigned per call."
            );
        }
    };

    let params = RecordingParams {
        conversation_id: &recorder_conv_id,
        ..params
    };

    start_transcription_at(http, &params, &recorder_base).await?;

    tracing::info!(
        "Waiting {}s before starting recording...",
        TRANSCRIPTION_TO_RECORDING_DELAY_SECS
    );
    tokio::time::sleep(Duration::from_secs(TRANSCRIPTION_TO_RECORDING_DELAY_SECS)).await;

    start_recording_at(http, &params, &recorder_base).await?;

    tracing::info!("Recording flow complete");
    Ok(RecordingSession {
        recorder_base,
        conversation_id: recorder_conv_id,
        recorder_token: request.recorder_token.to_string(),
        skype_token: request.skype_token.to_string(),
        chain_id: request.chain_id.to_string(),
        caller_mri: request.caller_mri.to_string(),
        participant_id: request.participant_id.to_string(),
    })
}

pub struct RecordingSession {
    pub recorder_base: String,
    pub conversation_id: String,
    pub recorder_token: String,
    pub skype_token: String,
    pub chain_id: String,
    pub caller_mri: String,
    pub participant_id: String,
}

pub async fn stop_call_recording(http: &reqwest::Client, session: &RecordingSession) -> Result<()> {
    let url =
        microsoft_calling::recorder_command_url(&session.recorder_base, &session.conversation_id);
    let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let payload = microsoft_calling::stop_recording_payload(
        &timestamp,
        &session.caller_mri,
        &session.participant_id,
    );

    tracing::info!("Stopping recording -> POST {}", url);
    let resp = http
        .post(&url)
        .header(
            "Authorization",
            format!("Bearer {}", session.recorder_token),
        )
        .header(microsoft_calling::SKYPE_TOKEN_HEADER, &session.skype_token)
        .header(microsoft_calling::CHAIN_ID_HEADER, &session.chain_id)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .context("Failed to POST stop recording")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    tracing::info!(
        "Stop recording response ({}): {}",
        status,
        &body[..body.len().min(200)]
    );

    let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let payload = microsoft_calling::stop_transcription_payload(
        &timestamp,
        &session.caller_mri,
        &session.participant_id,
    );

    tracing::info!("Stopping transcription -> POST {}", url);
    let resp = http
        .post(&url)
        .header(
            "Authorization",
            format!("Bearer {}", session.recorder_token),
        )
        .header(microsoft_calling::SKYPE_TOKEN_HEADER, &session.skype_token)
        .header(microsoft_calling::CHAIN_ID_HEADER, &session.chain_id)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .context("Failed to POST stop transcription")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    tracing::info!(
        "Stop transcription response ({}): {}",
        status,
        &body[..body.len().min(200)]
    );

    Ok(())
}

/// Wait for Trouter frames to find the recorder bot's service URL and session ID.
///
/// When the recorder bot successfully joins, the conv service sends an
/// `addParticipantSuccess` callback via Trouter containing a `conversationUpdate`
/// with the recorder bot's participant info. The bot's `conversationController` URL
/// reveals the recorder service endpoint and recording session ID.
async fn wait_for_recorder_info(
    ws: &mut TrouterSocket,
    timeout: Duration,
    http: &reqwest::Client,
    conv_params: &ConversationCallParams<'_>,
) -> Option<(String, String)> {
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        tokio::select! {
            frame = ws.recv_frame() => {
                match frame {
                    Ok(Some(text)) => {
                        if text.starts_with("2::") {
                            ws.send_text("2::").await.ok();
                            continue;
                        }

                        if text.starts_with("5:") && !text.contains("callAgent") {
                            continue;
                        }

                        let trunc: String = text.chars().take(300).collect();
                        tracing::info!("Recording wait frame (len={}): {}", text.len(), trunc);

                        // Decompress and parse every data frame (3::: frames are gzip-compressed,
                        if let Some(payload) = extract_call_payload(&text) {
                            // Acknowledge any callAcceptance frames (e.g. triggered by recorder bot joining).
                            // Without this, CC kills the call with 430/10065 after ~20s.
                            if let Some(ack_url) = payload
                                .pointer("/callAcceptance/links/acknowledgement")
                                .or_else(|| payload.pointer("/links/acknowledgement"))
                                .and_then(|v| v.as_str())
                            {
                                tracing::info!("Acknowledging callAcceptance from recording wait -> {}", ack_url);
                                match tokio::time::timeout(
                                    Duration::from_secs(5),
                                    signaling::acknowledge_call_acceptance(http, ack_url, conv_params),
                                ).await {
                                    Ok(Ok(())) => {},
                                    Ok(Err(e)) => tracing::warn!("Failed to acknowledge callAcceptance: {:#}", e),
                                    Err(_) => tracing::warn!("Timed out acknowledging callAcceptance (5s)"),
                                }
                            }

                            let payload_str = serde_json::to_string(&payload).unwrap_or_default();

                            // Check if this frame contains the recorder bot or callrecorder URL
                            if microsoft_calling::is_recorder_payload(&payload_str) {
                                tracing::info!("Found recorder-related Trouter frame");
                                std::fs::write("/tmp/recorder_trouter_payload.json", &payload_str).ok();
                                if let Some(result) = extract_recorder_from_payload(&payload) {
                                    return Some(result);
                                }
                            } else {
                                static FRAME_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                                let n = FRAME_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                std::fs::write(format!("/tmp/recorder_frame_{}.json", n), &payload_str).ok();
                                tracing::info!("Parsed frame {} - no recorder match, saved to /tmp/recorder_frame_{}.json", n, n);
                            }
                        }
                    }
                    Ok(None) => {
                        tracing::warn!("WebSocket closed while waiting for recorder info");
                        return None;
                    }
                    Err(e) => {
                        tracing::warn!("WebSocket error while waiting for recorder info: {}", e);
                        return None;
                    }
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                tracing::warn!("Timeout waiting for recorder info");
                return None;
            }
        }
    }
}

/// Extract recorder service base URL and conversation ID from a Trouter callback payload.
///
/// The recorder bot (28:bdd75849-...) has a `conversationController` URL like:
/// `https://api.flightproxy.teams.microsoft.com/api/v2/ep/{recorder-host}:{port}/...`
/// We extract the FlightProxy base + ep hostname:port as the recorder service base,
/// and the conversation ID from the URL path.
fn extract_recorder_from_payload(payload: &serde_json::Value) -> Option<(String, String)> {
    // Search the entire payload JSON string for recorder-related URLs
    let payload_str = serde_json::to_string(payload).unwrap_or_default();

    // Look for callrecorder hostname pattern in the payload
    // Pattern: aks-prod-XXXX-pNN-api.callrecorder.teams.cloud.microsoft:NNNNN
    if let Some(recorder_base) = microsoft_calling::recorder_base_from_payload(&payload_str) {
        if let Some(conv_id) = extract_recorder_conv_id(&payload_str) {
            return Some((recorder_base, conv_id));
        }
    }

    // Fallback: search for any conversationController URL with callrecorder
    tracing::debug!(
        "Could not find callrecorder URL in payload, searching for conv ID patterns..."
    );

    // Try to extract conversation ID from conversationController URL of the recorder bot
    if let Some(participants) = payload.get("participants").and_then(|p| p.as_array()) {
        for p in participants {
            let id = p.get("id").and_then(|i| i.as_str()).unwrap_or("");
            if id == microsoft_calling::RECORDER_BOT_MRI {
                if let Some(cc) = p
                    .pointer("/endpoints/0/conversationController")
                    .or_else(|| p.get("conversationController"))
                    .and_then(|c| c.as_str())
                {
                    tracing::debug!("Recorder bot conversationController: {}", cc);
                    // Extract conv ID and recorder service URL from this
                    let conv_id = extract_conversation_id(cc);
                    if let Some(cid) = conv_id {
                        return Some((microsoft_calling::RECORDER_SERVICE_BASE.to_string(), cid));
                    }
                }
            }
        }
    }

    tracing::warn!("Could not extract recorder info from Trouter payload");
    None
}

/// Try to find a recording session conversation ID in a payload string.
/// Looks for UUID patterns near callrecorder references.
fn extract_recorder_conv_id(payload: &str) -> Option<String> {
    if let Some(idx) = payload.find("/v2/oncommand/") {
        let after = &payload[idx + "/v2/oncommand/".len()..];
        let end = after.find(['"', '/', '?', ' ']).unwrap_or(after.len());
        let conv_id = &after[..end];
        if !conv_id.is_empty() {
            tracing::debug!("Found recorder conv ID from oncommand URL: {}", conv_id);
            return Some(conv_id.to_string());
        }
    }

    // Look for UUID pattern near callrecorder references
    if let Some(cr_idx) = payload.find("callrecorder") {
        let search_start = cr_idx.saturating_sub(200);
        let search_end = (cr_idx + 400).min(payload.len());
        let search_area = &payload[search_start..search_end];

        for (i, _) in search_area.match_indices('-') {
            if i >= 8 && i + 28 <= search_area.len() {
                let candidate = &search_area[i - 8..i + 28];
                if candidate.len() == 36
                    && candidate.chars().enumerate().all(|(j, c)| {
                        if j == 8 || j == 13 || j == 18 || j == 23 {
                            c == '-'
                        } else {
                            c.is_ascii_hexdigit()
                        }
                    })
                {
                    tracing::debug!("Found UUID near callrecorder: {}", candidate);
                    return Some(candidate.to_string());
                }
            }
        }
    }

    None
}

/// Extract conversation ID from a conversationController URL.
///
/// The URL looks like:
/// `https://...region.conv.skype.com/conv/{convId}`
/// or `https://...region.conv.skype.com/conv/{convId}/...`
fn extract_conversation_id(url: &str) -> Option<String> {
    // Find "/conv/" and take the next path segment (base64url-encoded UUID)
    let conv_marker = "/conv/";
    let idx = url.find(conv_marker)?;
    let after = &url[idx + conv_marker.len()..];
    let end = after.find(['/', '?']).unwrap_or(after.len());
    let b64_id = &after[..end];
    if b64_id.is_empty() {
        return None;
    }
    // Try to decode base64url to UUID (little-endian bytes)
    decode_base64_uuid(b64_id).or_else(|| Some(b64_id.to_string()))
}

/// Decode a base64url-encoded 16-byte value to a UUID string (little-endian format).
///
/// The conversation controller URL contains a base64url-encoded GUID where the bytes
/// are in Windows/COM little-endian format (first 3 groups byte-swapped).
fn decode_base64_uuid(b64: &str) -> Option<String> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    let bytes = URL_SAFE_NO_PAD.decode(b64).ok()?;
    if bytes.len() != 16 {
        return None;
    }
    // UUID from little-endian bytes (Data1=LE u32, Data2=LE u16, Data3=LE u16, rest=big-endian)
    Some(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[3],
        bytes[2],
        bytes[1],
        bytes[0],
        bytes[5],
        bytes[4],
        bytes[7],
        bytes[6],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    ))
}

/// Derive the addParticipant URL for bot injection from a conversationController URL.
///
/// Real Teams client uses `/addParticipant` (not `/add`) when injecting bots.
fn derive_add_participant_url_for_bot(conversation_controller: &str) -> String {
    if let Some(idx) = conversation_controller.find('?') {
        let (path, query) = conversation_controller.split_at(idx);
        format!("{}/addParticipant{}", path.trim_end_matches('/'), query)
    } else {
        format!(
            "{}/addParticipant",
            conversation_controller.trim_end_matches('/')
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_conversation_id() {
        let url = "https://amer03-1.conv.skype.com/conv/abc123-def-456";
        assert_eq!(
            extract_conversation_id(url),
            Some("abc123-def-456".to_string())
        );

        let url2 = "https://amer03-1.conv.skype.com/conv/abc123/something";
        assert_eq!(extract_conversation_id(url2), Some("abc123".to_string()));
    }
}
