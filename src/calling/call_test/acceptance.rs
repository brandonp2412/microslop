use super::*;
use ost_microsoft::calling as microsoft_calling;
use std::io::Write as _;

pub(super) fn check_call_end(v: &serde_json::Value) -> Option<String> {
    let (label, call_end) = if let Some(call_end) = v.get("callEnd") {
        ("Call ended", call_end)
    } else {
        ("Conversation ended", v.get("conversationEnd")?)
    };
    let code = call_end.get("code").and_then(|c| c.as_u64()).unwrap_or(0);
    let sub_code = call_end
        .get("subCode")
        .and_then(|c| c.as_u64())
        .unwrap_or(0);
    let phrase = call_end
        .get("phrase")
        .and_then(|s| s.as_str())
        .unwrap_or("unknown");
    let reason = call_end
        .get("reason")
        .and_then(|s| s.as_str())
        .unwrap_or("");
    let result_cat = call_end
        .get("resultCategories")
        .and_then(|r| {
            if let Some(arr) = r.as_array() {
                Some(
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                )
            } else {
                r.as_str().map(|s| s.to_string())
            }
        })
        .unwrap_or_default();
    Some(format!(
        "{}: {} (code={}, subCode={}, reason={}, categories={})",
        label, phrase, code, sub_code, reason, result_cat
    ))
}

/// Check a parsed JSON value for sessionRejection and return a descriptive string.
pub(super) fn check_session_rejection(v: &serde_json::Value) -> Option<String> {
    let rejection = v.get("sessionRejection")?;
    tracing::debug!(
        "Full sessionRejection: {}",
        serde_json::to_string_pretty(rejection).unwrap_or_default()
    );
    let code = rejection.get("code").and_then(|c| c.as_u64()).unwrap_or(0);
    let sub_code = rejection
        .get("subCode")
        .and_then(|c| c.as_u64())
        .unwrap_or(0);
    let phrase = rejection
        .get("phrase")
        .and_then(|s| s.as_str())
        .unwrap_or("unknown");
    let result_cat = rejection
        .get("resultCategories")
        .and_then(|r| r.as_str())
        .unwrap_or("");
    let diag = rejection
        .get("diagnosticContext")
        .and_then(|d| d.as_str())
        .unwrap_or("");
    let mut msg = format!(
        "Call rejected: {} (code={}, subCode={})",
        phrase, code, sub_code
    );
    if !result_cat.is_empty() {
        msg.push_str(&format!(" resultCategories={}", result_cat));
    }
    if !diag.is_empty() {
        msg.push_str(&format!(" diag={}", diag));
    }
    Some(msg)
}

/// Try to extract a call-relevant JSON payload from a frame.
///
/// Handles Trouter `3:::` deliveries and Socket.IO `5:ACK_ID::` events, including
/// stringified and compressed nested call payloads.
pub fn extract_call_payload(frame: &str) -> Option<serde_json::Value> {
    let json_str = extract_json_from_frame(frame)?;
    let value: serde_json::Value = serde_json::from_str(json_str).ok()?;

    let is_gzip = value
        .pointer("/headers/X-Microsoft-Skype-Content-Encoding")
        .or_else(|| value.pointer("/data/headers/X-Microsoft-Skype-Content-Encoding"))
        .and_then(|header| header.as_str())
        .is_some_and(|header| header.eq_ignore_ascii_case("gzip"));

    if let Some(payload) = extract_terminal_http_callback(&value, is_gzip) {
        return Some(payload);
    }

    if is_gzip {
        for path in &["/body", "/data/body"] {
            let Some(body) = value.pointer(path).and_then(|body| body.as_str()) else {
                continue;
            };
            let Some(decoded) = decompress_gzip_base64(body) else {
                continue;
            };
            let Ok(decoded) = serde_json::from_str::<serde_json::Value>(&decoded) else {
                continue;
            };
            if let Some(payload) = find_call_payload(&decoded) {
                return Some(payload);
            }
        }
    }

    find_call_payload(&value)
}

fn extract_terminal_http_callback(
    value: &serde_json::Value,
    is_gzip: bool,
) -> Option<serde_json::Value> {
    let url = value
        .get("url")
        .or_else(|| value.pointer("/data/url"))?
        .as_str()?;
    let key = if url.contains("/conversation/conversationEnd/") {
        "conversationEnd"
    } else if url.contains("/call/end/") {
        "callEnd"
    } else if url.contains("/call/rejection/") {
        "sessionRejection"
    } else {
        return None;
    };
    let body = value
        .get("body")
        .or_else(|| value.pointer("/data/body"))?
        .as_str()?;
    let decoded = if is_gzip {
        decompress_gzip_base64(body)?
    } else {
        body.to_owned()
    };
    let body: serde_json::Value = serde_json::from_str(&decoded).ok()?;
    if body.get(key).is_some() {
        Some(body)
    } else {
        Some(serde_json::json!({key: body}))
    }
}

fn find_call_payload(value: &serde_json::Value) -> Option<serde_json::Value> {
    if value.get("callAcceptance").is_some()
        || value.get("mediaAnswer").is_some()
        || value.get("mediaNegotiation").is_some()
        || value.get("sessionRejection").is_some()
        || value.get("callEnd").is_some()
        || value.get("conversationEnd").is_some()
        || value.pointer("/mediaContent/blob").is_some()
    {
        return Some(value.clone());
    }

    match value {
        serde_json::Value::Object(object) => object.iter().find_map(|(key, child)| {
            if let Some(encoded) = child.as_str() {
                let decoded = match key.as_str() {
                    "cp" => decompress_gzip_base64(encoded)
                        .and_then(|decoded| serde_json::from_str(&decoded).ok()),
                    "gp" => base64::engine::general_purpose::STANDARD
                        .decode(encoded)
                        .ok()
                        .and_then(|decoded| serde_json::from_slice(&decoded).ok()),
                    _ if encoded.trim_start().starts_with('{')
                        || encoded.trim_start().starts_with('[') =>
                    {
                        serde_json::from_str(encoded).ok()
                    }
                    _ => None,
                };
                if let Some(decoded) = decoded {
                    if let Some(payload) = find_call_payload(&decoded) {
                        return Some(payload);
                    }
                }
            }
            find_call_payload(child)
        }),
        serde_json::Value::Array(items) => items.iter().find_map(find_call_payload),
        _ => None,
    }
}

/// Decode base64 then decompress gzip data.
pub(super) fn decompress_gzip_base64(b64: &str) -> Option<String> {
    use std::io::Read as _;
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    let mut decoder = flate2::read::GzDecoder::new(&bytes[..]);
    let mut output = String::new();
    decoder.read_to_string(&mut output).ok()?;
    Some(output)
}

/// Wait on the Trouter WebSocket for a media answer or call acceptance event.
///
/// For channel calls via the conversation API, we expect a mediaAnswer callback
/// on our Trouter path with the remote SDP. We also handle sessionRejection.
fn trace_frame(frame: &str) {
    let Ok(path) = std::env::var("MICROSLOP_CALL_TRACE_PATH") else {
        return;
    };
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    else {
        return;
    };
    let _ = writeln!(file, "{frame}");
}

pub(super) async fn wait_for_call_acceptance(
    ws: &mut websocket::TrouterSocket,
    timeout: Duration,
) -> Result<CallAcceptanceResponse> {
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        tokio::select! {
            frame = ws.recv_frame() => {
                match frame? {
                    Some(text) => {
                        trace_frame(&text);
                        // Respond to heartbeats
                        if text.starts_with("2::") {
                            ws.send_text("2::").await.ok();
                            continue;
                        }

                        // Log all non-heartbeat frames for debugging
                        let truncated: String = text.chars().take(300).collect();
                        tracing::info!("Trouter frame: {}", truncated);

                        // Extract call payload (handles both 3::: and 5::: formats)
                        if let Some(v) = extract_call_payload(&text) {
                            tracing::debug!("Extracted call payload: {}", serde_json::to_string_pretty(&v).unwrap_or_default());

                            if text.contains("call/end") || text.contains("conversationEnd") || text.contains("conversationUpdate") {
                                tracing::info!("Frame body [{}]: {}",
                                    if text.contains("call/end") { "call/end" }
                                    else if text.contains("conversationEnd") { "conversationEnd" }
                                    else { "conversationUpdate" },
                                    serde_json::to_string(&v).unwrap_or_default());
                            }

                            if let Some(reason) = check_session_rejection(&v) {
                                tracing::warn!("{}", reason);
                                return Ok(CallAcceptanceResponse {
                                    sdp_blob: None,
                                    end_url: None,
                                    rejection_reason: Some(reason),
                                    acknowledgement_url: None,
                                    call_leg_url: None,
                                });
                            }

                            // Check for callEnd (server-side call termination)
                            if let Some(reason) = check_call_end(&v) {
                                tracing::warn!("{}", reason);
                                return Ok(CallAcceptanceResponse {
                                    sdp_blob: None,
                                    end_url: None,
                                    rejection_reason: Some(reason),
                                    acknowledgement_url: None,
                                    call_leg_url: None,
                                });
                            }

                            // Check for mediaAnswer or callAcceptance with SDP
                            if let Some(blob) = v.pointer("/callAcceptance/mediaContent/blob")
                                .or_else(|| v.pointer("/mediaContent/blob"))
                                .or_else(|| v.pointer("/mediaAnswer/mediaContent/blob"))
                                .and_then(|b| b.as_str())
                            {
                                let links_base = if v.get("callAcceptance").is_some() {
                                    "/callAcceptance/links"
                                } else if v.get("mediaAnswer").is_some() {
                                    "/mediaAnswer/links"
                                } else {
                                    "/links"
                                };
                                let get_link = |name: &str| -> Option<String> {
                                    v.pointer(&format!("{}/{}", links_base, name))
                                        .or_else(|| v.pointer(&format!("/links/{name}")))
                                        .and_then(|u| u.as_str())
                                        .map(|s| s.to_string())
                                };

                                let end_url = get_link("end");
                                let acknowledgement_url = get_link("acknowledgement");
                                let call_leg_url = get_link("callLeg");

                                trace_frame(&format!("# acceptance parsed ack={:?} leg={:?}", acknowledgement_url, call_leg_url));
                                tracing::info!("Received media answer/acceptance with SDP ({} bytes)", blob.len());
                                tracing::info!("  acknowledgement_url: {:?}", acknowledgement_url);
                                tracing::info!("  call_leg_url: {:?}", call_leg_url);

                                return Ok(CallAcceptanceResponse {
                                    sdp_blob: Some(blob.to_string()),
                                    end_url,
                                    rejection_reason: None,
                                    acknowledgement_url,
                                    call_leg_url,
                                });
                            }

                            // Acceptance without SDP
                            if v.get("callAcceptance").is_some() {
                                tracing::warn!("Call accepted but no SDP in response");
                                return Ok(CallAcceptanceResponse {
                                    sdp_blob: None,
                                    end_url: v.pointer("/callAcceptance/links/end")
                                        .and_then(|u| u.as_str())
                                        .map(str::to_owned),
                                    rejection_reason: Some("Call accepted without SDP — media setup impossible".to_string()),
                                    acknowledgement_url: v
                                        .pointer("/callAcceptance/links/acknowledgement")
                                        .and_then(|url| url.as_str())
                                        .map(str::to_owned),
                                    call_leg_url: v
                                        .pointer("/callAcceptance/links/callLeg")
                                        .and_then(|url| url.as_str())
                                        .map(str::to_owned),
                                });
                            }
                        }

                        let dbg_trunc: String = text.chars().take(200).collect();
                        tracing::debug!("Trouter frame (not call event): {}", dbg_trunc);
                    }
                    None => anyhow::bail!("WebSocket closed while waiting for acceptance"),
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                anyhow::bail!("Timeout waiting for call acceptance ({}s)", timeout.as_secs());
            }
        }
    }
}

fn renegotiation_answer(local_sdp: &str, remote_sdp: &str) -> Result<String> {
    fn media_payloads(sdp: &str, kind: &str) -> Option<std::collections::HashSet<String>> {
        let prefix = format!("m={kind} ");
        sdp.lines()
            .find(|line| line.starts_with(&prefix))
            .map(|line| line.split_whitespace().skip(3).map(str::to_owned).collect())
    }

    let remote_audio = media_payloads(remote_sdp, "audio")
        .context("Media renegotiation offer has no audio m-line")?;
    let remote_video = media_payloads(remote_sdp, "video")
        .context("Media renegotiation offer has no video m-line")?;
    let mut answer = String::new();
    let mut in_media = false;
    let mut allowed_payloads = std::collections::HashSet::<String>::new();

    for line in local_sdp.lines() {
        if let Some(rest) = line.strip_prefix("o=") {
            let mut fields = rest.split_whitespace().collect::<Vec<_>>();
            if fields.len() >= 3 {
                fields[2] = "1";
                answer.push_str("o=");
                answer.push_str(&fields.join(" "));
                answer.push_str("\r\n");
                continue;
            }
        }
        if line.starts_with("a=group:BUNDLE") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("m=") {
            let fields = rest.split_whitespace().collect::<Vec<_>>();
            if fields.len() < 4 {
                anyhow::bail!("Malformed local media line: {line}");
            }
            let kind = fields[0];
            in_media = true;
            let remote = match kind {
                "audio" => &remote_audio,
                "video" => &remote_video,
                _ => {
                    answer.push_str(line);
                    answer.push_str("\r\n");
                    continue;
                }
            };
            allowed_payloads = fields[3..]
                .iter()
                .filter(|payload| remote.contains(**payload))
                .map(|payload| (*payload).to_owned())
                .collect();
            anyhow::ensure!(
                !allowed_payloads.is_empty(),
                "No common {kind} codec in media renegotiation"
            );
            answer.push_str(&format!(
                "m={} {} {} {}\r\n",
                kind,
                fields[1],
                fields[2],
                fields[3..]
                    .iter()
                    .filter(|payload| allowed_payloads.contains(**payload))
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
            continue;
        }
        if in_media {
            let payload = line
                .strip_prefix("a=rtpmap:")
                .or_else(|| line.strip_prefix("a=fmtp:"))
                .or_else(|| line.strip_prefix("a=rtcp-fb:"))
                .or_else(|| line.strip_prefix("a=x-caps:"))
                .and_then(|rest| rest.split([' ', ':']).next());
            if payload.is_some_and(|payload| payload != "*" && !allowed_payloads.contains(payload))
            {
                continue;
            }
        }
        answer.push_str(line);
        answer.push_str("\r\n");
    }

    for section in remote_sdp.split("\nm=").skip(1) {
        let section = format!("m={section}");
        let Some(media_line) = section.lines().next() else {
            continue;
        };
        let fields = media_line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 4 || matches!(fields[0], "m=audio" | "m=video") {
            continue;
        }
        answer.push_str(&format!(
            "{} 0 {} {}\r\n",
            fields[0],
            fields[2],
            fields[3..].join(" ")
        ));
        if let Some(mid) = section.lines().find(|line| line.starts_with("a=mid:")) {
            answer.push_str(mid);
            answer.push_str("\r\n");
        }
        answer.push_str("a=inactive\r\n");
    }

    Ok(answer)
}

async fn handle_active_frame(
    ws: &mut websocket::TrouterSocket,
    text: &str,
    http: &reqwest::Client,
    params: &signaling::ConversationCallParams<'_>,
    local_sdp: &str,
    media: Option<&MediaLegHandles>,
) -> Result<Option<String>> {
    trace_frame(text);
    if text.starts_with("2::") {
        ws.send_text("2::").await.ok();
        return Ok(None);
    }
    let Some(value) = extract_call_payload(text) else {
        return Ok(None);
    };
    if let Some(reason) = check_call_end(&value).or_else(|| check_session_rejection(&value)) {
        return Ok(Some(reason));
    }
    if let Some(url) = value
        .pointer("/callAcceptance/links/acknowledgement")
        .and_then(|link| link.as_str())
    {
        trace_frame("# active callAcceptance acknowledgement start");
        signaling::acknowledge_call_acceptance(http, url, params).await?;
        trace_frame("# active callAcceptance acknowledgement done");
    }
    if let Some(negotiation) = value.get("mediaNegotiation") {
        let media_answer_url = negotiation
            .pointer("/links/mediaAnswer")
            .and_then(|url| url.as_str());
        let media_leg_id = negotiation
            .pointer("/mediaContent/mediaLegId")
            .and_then(|id| id.as_str());
        let remote_sdp = negotiation
            .pointer("/mediaContent/blob")
            .and_then(|blob| blob.as_str());
        if let (Some(media_answer_url), Some(media_leg_id), Some(remote_sdp)) =
            (media_answer_url, media_leg_id, remote_sdp)
        {
            let answer = renegotiation_answer(local_sdp, remote_sdp)?;
            signaling::answer_media_renegotiation(
                http,
                media_answer_url,
                params,
                &answer,
                media_leg_id,
            )
            .await?;
            if let Some(media) = media {
                update_video_transport(media, remote_sdp).await?;
            }
        }
    }
    Ok(None)
}

pub(super) async fn await_with_call_signaling<F, T>(
    ws: &mut websocket::TrouterSocket,
    operation: F,
    http: &reqwest::Client,
    params: &signaling::ConversationCallParams<'_>,
    local_sdp: &str,
) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    tokio::pin!(operation);
    loop {
        tokio::select! {
            result = &mut operation => return result,
            frame = ws.recv_frame() => match frame? {
                Some(text) => {
                    if let Some(reason) = handle_active_frame(ws, &text, http, params, local_sdp, None).await? {
                        anyhow::bail!(reason);
                    }
                }
                None => anyhow::bail!("Trouter closed while preparing media"),
            },
        }
    }
}

pub(super) async fn wait_for_call_end(
    ws: &mut websocket::TrouterSocket,
    timeout: Duration,
    http: &reqwest::Client,
    params: &signaling::ConversationCallParams<'_>,
    local_sdp: &str,
    media: Option<&MediaLegHandles>,
) -> Result<Option<String>> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        tokio::select! {
            frame = ws.recv_frame() => match frame? {
                Some(text) => {
                    if let Some(reason) = handle_active_frame(ws, &text, http, params, local_sdp, media).await? {
                        return Ok(Some(reason));
                    }
                }
                None => return Ok(Some("Trouter closed".to_string())),
            },
            _ = tokio::time::sleep_until(deadline) => return Ok(None),
        }
    }
}

/// Extract JSON payload from a socket.io frame string.
///
/// Trouter data frames use `3:::{...}` while Socket.IO events can include an
/// acknowledgement ID, for example `5:42::{...}`.
pub(super) fn extract_json_from_frame(frame: &str) -> Option<&str> {
    for prefix in &["3:::", "3::"] {
        if let Some(rest) = frame.strip_prefix(prefix) {
            return Some(rest);
        }
    }
    if let Some(rest) = frame.strip_prefix("5:") {
        let json_start = rest.find('{')?;
        return Some(&rest[json_start..]);
    }
    None
}

/// End a call by posting to a specific end URL.
pub(super) async fn end_call_by_url(
    http: &reqwest::Client,
    skype_token: &str,
    end_url: &str,
) -> Result<()> {
    tracing::info!("Ending call -> POST {}", end_url);

    let resp = http
        .post(end_url)
        .header(microsoft_calling::SKYPE_TOKEN_HEADER, skype_token)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({}))
        .send()
        .await
        .context("Failed to POST end call")?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    tracing::info!("Call ended ({}): {}", status, body);

    Ok(())
}

/// Extract user MRI (e.g. "8:orgid:<guid>") from a Skype token.
///
/// Skype tokens are JWTs. The payload contains a "skypeid" claim like
/// "orgid:<guid>" which we prefix with "8:" to form the MRI.
pub(super) fn extract_mri_from_skype_token(token: &str) -> Option<String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let payload = parts[1];
    let padded = match payload.len() % 4 {
        2 => format!("{}==", payload),
        3 => format!("{}=", payload),
        _ => payload.to_string(),
    };
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(padded.trim_end_matches('='))
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(payload))
        .ok()?;
    let json: serde_json::Value = serde_json::from_slice(&decoded).ok()?;

    if let Some(skypeid) = json.get("skypeid").and_then(|v| v.as_str()) {
        return Some(if skypeid.starts_with("8:") {
            skypeid.to_string()
        } else {
            format!("8:{skypeid}")
        });
    }

    if let Some(oid) = json.get("oid").and_then(|v| v.as_str()) {
        return Some(format!("8:orgid:{}", oid));
    }

    None
}

/// Extract the callee's OID from a 1:1 thread ID.
///
/// Thread format: `19:{oid1}_{oid2}@unq.gbl.spaces`
/// Returns the OID that is NOT the caller's OID.
pub(super) fn extract_callee_oid_from_thread(thread_id: &str, caller_oid: &str) -> Option<String> {
    let inner = thread_id
        .strip_prefix("19:")
        .and_then(|s| s.strip_suffix("@unq.gbl.spaces"))?;

    let parts: Vec<&str> = inner.split('_').collect();
    if parts.len() != 2 {
        return None;
    }

    if parts[0] == caller_oid {
        Some(parts[1].to_string())
    } else if parts[1] == caller_oid {
        Some(parts[0].to_string())
    } else {
        tracing::warn!(
            "Thread ID {} doesn't contain caller OID {}",
            thread_id,
            caller_oid
        );
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_call_payload, extract_json_from_frame};
    use base64::Engine;
    use std::io::Write as _;

    #[tokio::test]
    async fn active_call_acknowledges_late_acceptance_before_remote_hangup() {
        use futures::{SinkExt, StreamExt};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio_tungstenite::tungstenite::Message;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let http_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ack_url = format!("http://{}/ack", http_listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            socket.send(Message::Text(format!(
                "3:::{}",
                serde_json::json!({"id": 1, "callAcceptance": {"links": {"acknowledgement": ack_url}}})
            ))).await.unwrap();
            assert_eq!(
                socket.next().await.unwrap().unwrap().into_text().unwrap(),
                r#"3:::{"id":1,"status":200}"#
            );
            let (mut stream, _) = http_listener.accept().await.unwrap();
            let mut request = vec![0; 8192];
            let count = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..count]).to_ascii_lowercase();
            assert!(request.starts_with("post /ack http/1.1"));
            assert!(!request.contains("x-microsoft-skype-message-id: test"));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
            socket
                .send(Message::Text(
                    r#"3:::{"callEnd":{"code":200,"phrase":"Remote hangup"}}"#.to_owned(),
                ))
                .await
                .unwrap();
        });
        let session = serde_json::from_value(serde_json::json!({
            "socketio": format!("http://{address}"), "surl": "",
            "connectparams": {"sr":"", "issuer":"", "sp":"", "se":"", "st":"", "sig":""}
        }))
        .unwrap();
        let mut socket =
            crate::trouter::websocket::TrouterSocket::connect(&session, "test", "test")
                .await
                .unwrap();
        let params = crate::calling::signaling::ConversationCallParams {
            call_token: "test",
            personal: false,
            trouter_surl: "http://example.test/",
            caller_mri: "test",
            caller_display_name: "Test",
            endpoint_id: "test",
            participant_id: "test",
            thread_id: "self-chat",
            chain_id: "test",
            message_id: "test",
            caller_oid: "test",
            tenant_id: "test",
        };
        let result = super::wait_for_call_end(
            &mut socket,
            std::time::Duration::from_secs(2),
            &reqwest::Client::new(),
            &params,
            "v=0\r\no=- 0 0 IN IP4 10.0.0.1\r\ns=session\r\nc=IN IP4 10.0.0.1\r\nt=0 0\r\nm=audio 40000 RTP/SAVP 0\r\na=rtpmap:0 PCMU/8000\r\na=sendrecv\r\nm=video 40002 RTP/SAVP 122 121 123\r\na=rtpmap:122 X-H264UC/90000\r\na=rtpmap:121 x-rtvc1/90000\r\na=rtpmap:123 x-ulpfecuc/90000\r\na=x-caps:121 test\r\na=sendrecv\r\n",
            None,
        )
        .await
        .unwrap();
        server.abort();
        assert!(
            result
                .as_deref()
                .is_some_and(|reason| reason.contains("Remote hangup")),
            "late acceptance was not acknowledged: {result:?}"
        );
    }

    #[tokio::test]
    async fn active_call_answers_media_renegotiation() {
        use futures::{SinkExt, StreamExt};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio_tungstenite::tungstenite::Message;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let http_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let media_answer_url = format!("http://{}/answer", http_listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            socket
                .send(Message::Text(format!(
                    "3:::{}",
                    serde_json::json!({
                        "id": 7,
                        "mediaNegotiation": {
                            "mediaContent": {
                                "blob": "v=0\r\nm=audio 3479 RTP/SAVP 0 111\r\nm=video 3480 RTP/SAVP 122 107 123\r\nm=x-data 3480 RTP/SAVP 127 126\r\na=mid:4\r\n",
                                "mediaLegId": "leg-42"
                            },
                            "links": {"mediaAnswer": media_answer_url}
                        }
                    })
                )))
                .await
                .unwrap();
            assert_eq!(
                socket.next().await.unwrap().unwrap().into_text().unwrap(),
                r#"3:::{"id":7,"status":200}"#
            );
            let (mut stream, _) = http_listener.accept().await.unwrap();
            let mut request = vec![0; 16384];
            let count = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..count]);
            assert!(request.starts_with("POST /answer HTTP/1.1"));
            let body = request.split_once("\r\n\r\n").unwrap().1;
            let payload: serde_json::Value = serde_json::from_str(body).unwrap();
            let answer = payload
                .pointer("/mediaAnswer/mediaContent/blob")
                .and_then(|blob| blob.as_str())
                .unwrap();
            assert!(
                answer.contains("m=video 40002 RTP/SAVP 122 123"),
                "{answer}"
            );
            assert!(!answer.contains("x-rtvc1"), "{answer}");
            assert!(answer.contains("m=x-data 0 RTP/SAVP 127 126"), "{answer}");
            assert_eq!(
                payload["mediaAnswer"]["mediaContent"]["mediaLegId"],
                "leg-42"
            );
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
            socket
                .send(Message::Text(
                    r#"3:::{"callEnd":{"code":200,"phrase":"Remote hangup"}}"#.to_owned(),
                ))
                .await
                .unwrap();
        });
        let session = serde_json::from_value(serde_json::json!({
            "socketio": format!("http://{address}"), "surl": "",
            "connectparams": {"sr":"", "issuer":"", "sp":"", "se":"", "st":"", "sig":""}
        }))
        .unwrap();
        let mut socket =
            crate::trouter::websocket::TrouterSocket::connect(&session, "test", "test")
                .await
                .unwrap();
        let params = crate::calling::signaling::ConversationCallParams {
            call_token: "test",
            personal: false,
            trouter_surl: "http://example.test/",
            caller_mri: "test",
            caller_display_name: "Test",
            endpoint_id: "test",
            participant_id: "test",
            thread_id: "self-chat",
            chain_id: "test",
            message_id: "test",
            caller_oid: "test",
            tenant_id: "test",
        };
        let result = super::wait_for_call_end(
            &mut socket,
            std::time::Duration::from_secs(2),
            &reqwest::Client::new(),
            &params,
            "v=0\r\no=- 0 0 IN IP4 10.0.0.1\r\ns=session\r\nc=IN IP4 10.0.0.1\r\nt=0 0\r\nm=audio 40000 RTP/SAVP 0\r\na=rtpmap:0 PCMU/8000\r\na=sendrecv\r\nm=video 40002 RTP/SAVP 122 121 123\r\na=rtpmap:122 X-H264UC/90000\r\na=rtpmap:121 x-rtvc1/90000\r\na=rtpmap:123 x-ulpfecuc/90000\r\na=x-caps:121 test\r\na=sendrecv\r\n",
            None,
        )
        .await
        .unwrap();
        server.await.unwrap();
        assert!(
            result
                .as_deref()
                .is_some_and(|reason| reason.contains("Remote hangup"))
        );
    }

    #[tokio::test]
    async fn media_answer_preserves_acknowledgement_and_call_leg_links() {
        use futures::SinkExt;
        use tokio_tungstenite::tungstenite::Message;

        for nested in [false, true] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
                let mut payload = serde_json::json!({
                    "mediaAnswer": {"mediaContent": {"blob": "remote-sdp"}}
                });
                let links = serde_json::json!({
                    "acknowledgement": "https://example.test/ack",
                    "callLeg": "https://example.test/leg",
                    "end": "https://example.test/end"
                });
                if nested {
                    payload["mediaAnswer"]["links"] = links;
                } else {
                    payload["links"] = links;
                }
                socket
                    .send(Message::Text(format!("3:::{payload}")))
                    .await
                    .unwrap();
            });
            let session = serde_json::from_value(serde_json::json!({
                "socketio": format!("http://{address}"),
                "surl": "",
                "connectparams": {"sr":"", "issuer":"", "sp":"", "se":"", "st":"", "sig":""}
            }))
            .unwrap();
            let mut socket =
                crate::trouter::websocket::TrouterSocket::connect(&session, "test", "test")
                    .await
                    .unwrap();
            let answer =
                super::wait_for_call_acceptance(&mut socket, std::time::Duration::from_secs(2))
                    .await
                    .unwrap();
            server.await.unwrap();
            assert_eq!(answer.sdp_blob.as_deref(), Some("remote-sdp"));
            assert_eq!(
                answer.acknowledgement_url.as_deref(),
                Some("https://example.test/ack")
            );
            assert_eq!(
                answer.call_leg_url.as_deref(),
                Some("https://example.test/leg")
            );
            assert_eq!(answer.end_url.as_deref(), Some("https://example.test/end"));
        }
    }

    #[test]
    fn extracts_terminal_conversation_callback() {
        let body = serde_json::json!({
            "code": 409,
            "subCode": 5704,
            "phrase": "Conversation cannot be created due to a failure to resolve ownership.",
            "resultCategories": ["ExpectedError"]
        })
        .to_string();
        let frame = format!(
            "3:::{}",
            serde_json::json!({
                "id": 1,
                "method": "POST",
                "url": "/v4/f/token/callAgent/ep/path/conversation/conversationEnd/",
                "body": body
            })
        );
        let payload = extract_call_payload(&frame).expect("conversation end should parse");
        assert!(
            super::check_call_end(&payload).is_some_and(
                |reason| reason.contains("code=409") && reason.contains("subCode=5704")
            )
        );
    }

    #[test]
    fn preserves_wrapped_call_end_details() {
        let body = serde_json::json!({
            "callEnd": {
                "reason": "clientError",
                "code": 408,
                "subCode": 10056,
                "phrase": "Call Controller timed out while waiting for acknowledgement.",
                "resultCategories": ["UnexpectedClientError"]
            }
        })
        .to_string();
        let frame = format!(
            "3:::{}",
            serde_json::json!({
                "id": 1,
                "method": "POST",
                "url": "/v4/f/token/callAgent/ep/path/call/end/",
                "body": body
            })
        );
        let payload = extract_call_payload(&frame).expect("call end should parse");
        assert!(super::check_call_end(&payload).is_some_and(|reason| {
            reason.contains("code=408")
                && reason.contains("subCode=10056")
                && reason.contains("timed out while waiting for acknowledgement")
        }));
    }

    #[test]
    fn extracts_socketio_event_with_ack_id() {
        let frame = r#"5:42::{"name":"event","args":[]}"#;
        assert_eq!(
            extract_json_from_frame(frame),
            Some(r#"{"name":"event","args":[]}"#)
        );
    }

    #[test]
    fn unwraps_call_acceptance_from_socketio_string_body() {
        let body = serde_json::json!({
            "callAcceptance": {
                "mediaContent": {"blob": "remote-sdp"},
                "links": {"acknowledgement": "https://ack"}
            }
        })
        .to_string();
        let envelope = serde_json::json!({"name": "event", "args": [{"body": body}]}).to_string();
        let frame = format!("5:42::{envelope}");
        let payload = extract_call_payload(&frame).expect("call acceptance should be unwrapped");
        assert_eq!(
            payload
                .pointer("/callAcceptance/mediaContent/blob")
                .and_then(|v| v.as_str()),
            Some("remote-sdp")
        );
    }

    #[test]
    fn unwraps_call_acceptance_from_compressed_cp() {
        let inner = r#"{"callAcceptance":{"mediaContent":{"blob":"remote-sdp"}}}"#;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(inner.as_bytes()).unwrap();
        let cp = base64::engine::general_purpose::STANDARD.encode(encoder.finish().unwrap());
        let envelope = serde_json::json!({"name": "event", "args": [{"cp": cp}]}).to_string();
        let frame = format!("5:7::{envelope}");

        let payload = extract_call_payload(&frame).expect("compressed acceptance should parse");
        assert_eq!(
            payload
                .pointer("/callAcceptance/mediaContent/blob")
                .and_then(|v| v.as_str()),
            Some("remote-sdp")
        );
    }
}
