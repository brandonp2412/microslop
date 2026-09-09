//! Call signaling HTTP operations — accept, send media answer, end call.
//!
//! Includes the two-phase conversation API for call placement via epconv.

use anyhow::{Context, Result};
pub use microsoft_calling::echo_thread_id;
pub(crate) use microsoft_calling::{
    trouter_callback, CLIENT_HEADER, MIGRATION_HEADER, MIGRATION_VALUE, PARTITION_HEADER,
    PROXY_CLUSTER_CONTEXT_HEADER, REFERER_HEADER, REGION_HEADER, RING_HEADER, SKYPE_CLIENT_HEADER,
    SKYPE_TOKEN_HEADER, TEAMS_PARTITION, TEAMS_REFERER, TEAMS_REGION, TEAMS_RING,
};
use ost_microsoft::calling as microsoft_calling;

use super::CallNotification;
use uuid;

/// Response from phase 1 (create conversation).
#[derive(Debug)]
pub struct ConversationCreated {
    /// URL to POST phase 2 to (the conversationController).
    pub conversation_controller: String,
    /// The exact addParticipant URL from the response links (if present).
    pub add_participant_url: Option<String>,
    /// Full response body for debugging.
    pub response_body: String,
}

/// Response from phase 2 (join with SDP).
#[derive(Debug)]
pub struct ConversationJoined {
    /// CC active URL from response headers (x-microsoft-skype-proxy-cluster-context).
    pub cc_active_url: Option<String>,
    /// Full response body for debugging.
    pub response_body: String,
}

/// Parameters for the two-phase conversation call placement.
pub struct ConversationCallParams<'a> {
    pub call_token: &'a str,
    pub personal: bool,
    pub trouter_surl: &'a str,
    pub caller_mri: &'a str,
    pub caller_display_name: &'a str,
    pub endpoint_id: &'a str,
    pub participant_id: &'a str,
    pub thread_id: &'a str,
    pub chain_id: &'a str,
    pub message_id: &'a str,
    /// OID (object ID) extracted from caller MRI, e.g. the GUID part of "8:orgid:{guid}".
    pub caller_oid: &'a str,
    pub tenant_id: &'a str,
}

fn response_link<'a>(response: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    response
        .pointer(&format!("/links/{name}"))
        .or_else(|| response.pointer(&format!("/conversationResponse/links/{name}")))
        .and_then(|value| value.as_str())
}

fn payload_context<'a>(
    params: &ConversationCallParams<'a>,
) -> microsoft_calling::PayloadContext<'a> {
    microsoft_calling::PayloadContext {
        caller_mri: params.caller_mri,
        caller_display_name: params.caller_display_name,
        endpoint_id: params.endpoint_id,
        participant_id: params.participant_id,
        thread_id: params.thread_id,
        caller_oid: params.caller_oid,
        tenant_id: params.tenant_id,
        message_id: params.message_id,
    }
}

fn call_controller_post(
    http: &reqwest::Client,
    url: &str,
    params: &ConversationCallParams<'_>,
    message_id: &str,
) -> reqwest::RequestBuilder {
    let request = http
        .post(url)
        .header("Content-Type", "application/json")
        .header(microsoft_calling::CHAIN_ID_HEADER, params.chain_id)
        .header(microsoft_calling::MESSAGE_ID_HEADER, message_id)
        .header(CLIENT_HEADER, SKYPE_CLIENT_HEADER)
        .header(RING_HEADER, TEAMS_RING);
    if params.personal {
        request
            .header(SKYPE_TOKEN_HEADER, params.call_token)
            .header(REFERER_HEADER, microsoft_calling::TEAMS_LIVE_REFERER)
    } else {
        request
            .header("Authorization", format!("Bearer {}", params.call_token))
            .header(REFERER_HEADER, TEAMS_REFERER)
            .header(PARTITION_HEADER, TEAMS_PARTITION)
            .header(REGION_HEADER, TEAMS_REGION)
            .header(MIGRATION_HEADER, MIGRATION_VALUE)
    }
}

fn skype_post(http: &reqwest::Client, url: &str, skype_token: &str) -> reqwest::RequestBuilder {
    http.post(url)
        .header(SKYPE_TOKEN_HEADER, skype_token)
        .header("Content-Type", "application/json")
}

/// Phase 1: Create a conversation by POSTing to /api/v2/epconv.
///
/// Returns the conversationController URL from the response.
pub async fn create_conversation(
    http: &reqwest::Client,
    epconv_url: &str,
    params: &ConversationCallParams<'_>,
) -> Result<ConversationCreated> {
    let callback = |path: &str| trouter_callback(params.trouter_surl, params.endpoint_id, path);
    let payload =
        microsoft_calling::create_conversation_payload(&payload_context(params), &callback);

    tracing::info!("Phase 1: POST {} (create conversation)", epconv_url);
    tracing::debug!(
        "Phase 1 payload: {}",
        serde_json::to_string_pretty(&payload).unwrap_or_default()
    );

    let resp = call_controller_post(http, epconv_url, params, params.message_id)
        .json(&payload)
        .send()
        .await
        .context("Phase 1 POST to epconv failed")?;

    let status = resp.status();

    let headers = resp.headers().clone();
    let body = resp.text().await.unwrap_or_default();

    tracing::info!("Phase 1 response: {} ({} bytes)", status, body.len());
    tracing::debug!("Phase 1 response body: {}", &body[..body.len().min(2000)]);
    for (k, v) in headers.iter() {
        tracing::debug!(
            "Phase 1 header: {}: {}",
            k,
            v.to_str().unwrap_or("(binary)")
        );
    }

    if !status.is_success() {
        anyhow::bail!("Phase 1 epconv failed ({}): {}", status, body);
    }

    let resp_json: serde_json::Value =
        serde_json::from_str(&body).context("Phase 1 response is not valid JSON")?;

    let conv_controller = resp_json
        .pointer("/conversationController")
        .or_else(|| resp_json.pointer("/conversationResponse/conversationController"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let conv_controller = conv_controller
        .or_else(|| {
            headers
                .get("location")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .context("No conversationController in phase 1 response")?;

    let add_participant_url = response_link(&resp_json, "addParticipant").map(str::to_owned);

    tracing::info!(
        "Phase 1 success: conversationController = {}, addParticipant link = {:?}",
        conv_controller,
        add_participant_url
    );

    Ok(ConversationCreated {
        conversation_controller: conv_controller,
        add_participant_url,
        response_body: body,
    })
}

/// Phase 2: Join the conversation with an SDP offer.
///
/// POSTs to the conversationController URL from phase 1.
pub async fn join_conversation_with_sdp(
    http: &reqwest::Client,
    conversation_controller: &str,
    params: &ConversationCallParams<'_>,
    sdp_offer: &str,
) -> Result<ConversationJoined> {
    let callback = |path: &str| trouter_callback(params.trouter_surl, params.endpoint_id, path);
    let payload = microsoft_calling::join_conversation_payload(
        &payload_context(params),
        &callback,
        sdp_offer,
    );

    tracing::info!("Phase 2: POST {} (join with SDP)", conversation_controller);
    tracing::debug!(
        "Phase 2 payload: {}",
        serde_json::to_string_pretty(&payload).unwrap_or_default()
    );

    let resp = call_controller_post(http, conversation_controller, params, params.message_id)
        .json(&payload)
        .send()
        .await
        .context("Phase 2 POST to conversationController failed")?;

    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp.text().await.unwrap_or_default();

    tracing::info!("Phase 2 response: {} ({} bytes)", status, body.len());
    tracing::debug!("Phase 2 response body: {}", &body[..body.len().min(2000)]);

    if !status.is_success() {
        anyhow::bail!("Phase 2 join failed ({}): {}", status, body);
    }

    let cc_active_url = headers
        .get(PROXY_CLUSTER_CONTEXT_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    tracing::info!("Phase 2 success. CC active URL: {:?}", cc_active_url);

    Ok(ConversationJoined {
        cc_active_url,
        response_body: body,
    })
}

/// Accept an incoming call by POSTing to the acceptance URL.
pub async fn accept_call(
    http: &reqwest::Client,
    skype_token: &str,
    notification: &CallNotification,
    video: bool,
) -> Result<()> {
    let invitation = notification
        .call_invitation
        .as_ref()
        .context("No callInvitation in notification")?;
    let links = invitation
        .links
        .as_ref()
        .context("No links in callInvitation")?;
    let acceptance_url = links
        .acceptance
        .as_ref()
        .context("No acceptance URL in links")?;

    let payload = microsoft_calling::acceptance_payload(video);

    tracing::info!("Accepting call -> POST {}", acceptance_url);

    let resp = skype_post(http, acceptance_url, skype_token)
        .json(&payload)
        .send()
        .await
        .context("Failed to POST acceptance")?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if status.is_success() {
        tracing::info!("Call accepted ({}): {}", status, body);
        Ok(())
    } else {
        anyhow::bail!("Call acceptance failed ({}): {}", status, body);
    }
}

/// Send the SDP media answer to the mediaAnswer URL.
pub async fn send_media_answer(
    http: &reqwest::Client,
    skype_token: &str,
    notification: &CallNotification,
    sdp_answer: &str,
) -> Result<()> {
    let invitation = notification
        .call_invitation
        .as_ref()
        .context("No callInvitation in notification")?;
    let links = invitation
        .links
        .as_ref()
        .context("No links in callInvitation")?;
    let media_answer_url = links
        .media_answer
        .as_ref()
        .context("No mediaAnswer URL in links")?;

    let payload = microsoft_calling::media_answer_payload(sdp_answer);

    tracing::info!("Sending media answer -> POST {}", media_answer_url);

    let resp = skype_post(http, media_answer_url, skype_token)
        .json(&payload)
        .send()
        .await
        .context("Failed to POST media answer")?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if status.is_success() {
        tracing::info!("Media answer sent ({}): {}", status, body);
        Ok(())
    } else {
        anyhow::bail!("Media answer failed ({}): {}", status, body);
    }
}

pub async fn answer_media_renegotiation(
    http: &reqwest::Client,
    media_answer_url: &str,
    params: &ConversationCallParams<'_>,
    sdp_answer: &str,
    media_leg_id: &str,
) -> Result<()> {
    let payload = microsoft_calling::media_renegotiation_answer_payload(sdp_answer, media_leg_id);
    let message_id = uuid::Uuid::new_v4().to_string();
    let resp = call_controller_post(http, media_answer_url, params, &message_id)
        .json(&payload)
        .send()
        .await
        .context("Failed to POST media renegotiation answer")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if status.is_success() {
        tracing::info!("Media renegotiation answered ({status})");
        return Ok(());
    }
    anyhow::bail!("Media renegotiation answer failed ({status}): {body}")
}

/// Phase 3: Acknowledge call acceptance.
///
/// POST to the acknowledgement URL from the callAcceptance Trouter callback.
/// This tells the Call Controller we received the SDP answer and keeps the call alive.
/// Without this, CC times out and kills the call (error 430/subCode 10065).
pub async fn acknowledge_call_acceptance(
    http: &reqwest::Client,
    acknowledgement_url: &str,
    params: &ConversationCallParams<'_>,
) -> Result<()> {
    let callback = |path: &str| trouter_callback(params.trouter_surl, params.endpoint_id, path);
    let payload = microsoft_calling::call_acceptance_acknowledgement_payload(&callback);

    tracing::info!(
        "Phase 3a: Acknowledging call acceptance -> POST {}",
        acknowledgement_url
    );

    let message_id = uuid::Uuid::new_v4().to_string();
    let resp = call_controller_post(http, acknowledgement_url, params, &message_id)
        .json(&payload)
        .send()
        .await
        .context("Failed to POST call acceptance acknowledgement")?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if let Ok(path) = std::env::var("MICROSLOP_CALL_TRACE_PATH") {
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            use std::io::Write as _;
            let _ = writeln!(
                file,
                "# callAcceptanceAcknowledgement status={} body={}",
                status, body
            );
        }
    }

    if status.is_success() {
        tracing::info!("Call acceptance acknowledged ({}): {}", status, body);
        Ok(())
    } else {
        anyhow::bail!(
            "Call acceptance acknowledgement failed ({}): {}",
            status,
            body
        );
    }
}

/// Phase 3b: Register CC signaling callbacks on the call leg.
///
/// POST callParticipantUpdate with Trouter callback links to the callLeg URL.
/// This tells the Call Controller where to send subsequent signaling events
/// (media renegotiation, call end, transfer, etc.).
pub async fn register_cc_callbacks(
    http: &reqwest::Client,
    call_leg_url: &str,
    params: &ConversationCallParams<'_>,
) -> Result<()> {
    let callback = |path: &str| trouter_callback(params.trouter_surl, params.endpoint_id, path);
    let payload = microsoft_calling::cc_callback_registration_payload(&callback);

    tracing::info!(
        "Phase 3b: Registering CC callbacks -> POST {}",
        call_leg_url
    );
    tracing::debug!(
        "Phase 3b payload: {}",
        serde_json::to_string_pretty(&payload).unwrap_or_default()
    );

    let message_id = uuid::Uuid::new_v4().to_string();
    let resp = call_controller_post(http, call_leg_url, params, &message_id)
        .json(&payload)
        .send()
        .await
        .context("Failed to POST CC callback registration")?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if status.is_success() {
        tracing::info!("CC callbacks registered ({}): {}", status, body);
        Ok(())
    } else {
        anyhow::bail!("CC callback registration failed ({}): {}", status, body);
    }
}

/// Create an Echo bot call in a single epconv POST (matching real Teams client flow).
///
/// Unlike the two-phase approach for channel calls, the echo call embeds the SDP
/// directly in the epconv request. Returns both ConversationCreated and ConversationJoined.
pub async fn create_echo_call(
    http: &reqwest::Client,
    epconv_url: &str,
    params: &ConversationCallParams<'_>,
    sdp_offer: &str,
) -> Result<(ConversationCreated, ConversationJoined)> {
    let callback = |path: &str| trouter_callback(params.trouter_surl, params.endpoint_id, path);
    let payload =
        microsoft_calling::echo_call_payload(&payload_context(params), &callback, sdp_offer);

    tracing::info!(
        "Echo call: POST {} (single-shot epconv with SDP, scenario=UserInitiatedTestCall)",
        epconv_url
    );

    let resp = call_controller_post(http, epconv_url, params, params.message_id)
        .json(&payload)
        .send()
        .await
        .context("Echo call POST to epconv failed")?;

    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp.text().await.unwrap_or_default();

    tracing::info!("Echo call response: {} ({} bytes)", status, body.len());
    tracing::debug!("Echo call response body: {}", &body[..body.len().min(2000)]);
    for (k, v) in headers.iter() {
        tracing::debug!(
            "Echo call header: {}: {}",
            k,
            v.to_str().unwrap_or("(binary)")
        );
    }

    if !status.is_success() {
        anyhow::bail!("Echo call epconv failed ({}): {}", status, body);
    }

    let resp_json: serde_json::Value =
        serde_json::from_str(&body).context("Echo call response is not valid JSON")?;

    let conv_controller = resp_json
        .pointer("/conversationController")
        .or_else(|| resp_json.pointer("/conversationResponse/conversationController"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            headers
                .get("location")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .context("No conversationController in echo call response")?;

    let add_participant_url = response_link(&resp_json, "addParticipant").map(str::to_owned);

    tracing::info!(
        "Echo call: conversationController = {}, addParticipant link = {:?}",
        conv_controller,
        add_participant_url
    );

    let cc_active_url = response_link(&resp_json, "active").map(str::to_owned);

    let created = ConversationCreated {
        conversation_controller: conv_controller,
        add_participant_url,
        response_body: body.clone(),
    };
    let joined = ConversationJoined {
        cc_active_url,
        response_body: body,
    };

    Ok((created, joined))
}

/// Invite the Echo bot into the conversation via POST /conv/{id}/add.
///
/// This is step 3 of the Echo call flow: after creating the conversation (epconv)
/// and joining with SDP, we add the Echo bot as a participant with the 1:1 thread context.
pub async fn invite_echo_bot(
    http: &reqwest::Client,
    conversation_controller: &str,
    add_participant_url: Option<&str>,
    params: &ConversationCallParams<'_>,
) -> Result<()> {
    let callback = |path: &str| trouter_callback(params.trouter_surl, params.endpoint_id, path);
    let fallback_url = microsoft_calling::echo_bot_invite_url(conversation_controller);
    let mut add_urls = Vec::with_capacity(2);
    if let Some(url) = add_participant_url {
        add_urls.push(url.to_owned());
    }
    if add_urls.first() != Some(&fallback_url) {
        add_urls.push(fallback_url);
    }
    let echo_participant_id = uuid::Uuid::new_v4().to_string();
    let payload = microsoft_calling::echo_bot_invite_payload(
        &payload_context(params),
        &callback,
        &echo_participant_id,
    );

    tracing::debug!(
        "Echo bot invite payload: {}",
        serde_json::to_string_pretty(&payload).unwrap_or_default()
    );

    for (index, add_url) in add_urls.iter().enumerate() {
        let echo_bot_msg_id = uuid::Uuid::new_v4().to_string();
        tracing::info!(
            "Inviting Echo bot -> POST {} (msg_id={})",
            add_url,
            echo_bot_msg_id
        );
        let resp = call_controller_post(http, add_url, params, &echo_bot_msg_id)
            .json(&payload)
            .send()
            .await
            .context("Failed to POST echo bot invite")?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        tracing::info!(
            "Echo bot invite response: {} ({} bytes)",
            status,
            body.len()
        );
        tracing::debug!("Echo bot invite response body: {}", body);
        if status.is_success() {
            return Ok(());
        }
        if status == reqwest::StatusCode::NOT_FOUND && index + 1 < add_urls.len() {
            tracing::warn!(
                "Echo bot invite endpoint {} returned 404; retrying fallback",
                add_url
            );
            continue;
        }
        anyhow::bail!(
            "Echo bot invite failed at {} ({}): {}",
            add_url,
            status,
            body
        );
    }

    anyhow::bail!("No Echo bot invite endpoint is available")
}

/// Create a 1:1 call in a single epconv POST.
///
/// Similar to create_echo_call but without the UserInitiatedTestCall scenario.
/// Used for calling real users by their 1:1 thread ID.
pub async fn create_1to1_call(
    http: &reqwest::Client,
    epconv_url: &str,
    params: &ConversationCallParams<'_>,
    sdp_offer: &str,
    callee_mri: Option<&str>,
    include_video: bool,
) -> Result<(ConversationCreated, ConversationJoined)> {
    let callback = |path: &str| trouter_callback(params.trouter_surl, params.endpoint_id, path);
    let payload = if params.personal {
        let callee_mri = callee_mri.context("Personal 1:1 calls require a callee MRI")?;
        let callee_participant_id = uuid::Uuid::new_v4().to_string();
        let media_leg_id = uuid::Uuid::new_v4().simple().to_string().to_uppercase();
        microsoft_calling::personal_one_to_one_call_payload(
            &payload_context(params),
            &callback,
            sdp_offer,
            callee_mri,
            &callee_participant_id,
            &media_leg_id,
            include_video,
        )
    } else {
        microsoft_calling::one_to_one_call_payload(
            &payload_context(params),
            &callback,
            sdp_offer,
            callee_mri,
            include_video,
        )
    };

    tracing::info!(
        "1:1 call: POST {} (direct peer in initial epconv, video={})",
        epconv_url,
        include_video
    );

    let mut attempt = 0u8;
    let resp = loop {
        attempt += 1;
        let result = call_controller_post(http, epconv_url, params, params.message_id)
            .json(&payload)
            .send()
            .await;
        match result {
            Ok(response) => break response,
            Err(error) if attempt < 3 => {
                tracing::warn!("1:1 epconv transport failed on attempt {attempt}: {error}");
                tokio::time::sleep(std::time::Duration::from_millis(250 * attempt as u64)).await;
            }
            Err(error) => return Err(error).context("1:1 call POST to epconv failed"),
        }
    };

    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp.text().await.unwrap_or_default();

    tracing::info!("1:1 call response: {} ({} bytes)", status, body.len());
    tracing::debug!("1:1 call response body: {}", &body[..body.len().min(2000)]);

    if !status.is_success() {
        anyhow::bail!("1:1 call epconv failed ({}): {}", status, body);
    }

    let resp_json: serde_json::Value =
        serde_json::from_str(&body).context("1:1 call response is not valid JSON")?;

    let conv_controller = resp_json
        .pointer("/conversationController")
        .or_else(|| resp_json.pointer("/conversationResponse/conversationController"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            headers
                .get("location")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .context("No conversationController in 1:1 call response")?;

    let add_participant_url = response_link(&resp_json, "addParticipant").map(str::to_owned);

    tracing::info!(
        "1:1 call: conversationController = {}, addParticipant link = {:?}",
        conv_controller,
        add_participant_url
    );

    let cc_active_url = response_link(&resp_json, "active").map(str::to_owned);

    let created = ConversationCreated {
        conversation_controller: conv_controller,
        add_participant_url,
        response_body: body.clone(),
    };
    let joined = ConversationJoined {
        cc_active_url,
        response_body: body,
    };

    Ok((created, joined))
}

pub async fn invite_user(
    http: &reqwest::Client,
    conversation_controller: &str,
    add_participant_url: Option<&str>,
    params: &ConversationCallParams<'_>,
    callee_mri: &str,
    include_video: bool,
) -> Result<()> {
    let callback = |path: &str| trouter_callback(params.trouter_surl, params.endpoint_id, path);
    let fallback_url = microsoft_calling::user_invite_url(conversation_controller);
    let mut add_urls = Vec::with_capacity(2);
    if let Some(url) = add_participant_url {
        add_urls.push(url.to_owned());
    }
    if add_urls.first() != Some(&fallback_url) {
        add_urls.push(fallback_url);
    }
    let callee_participant_id = uuid::Uuid::new_v4().to_string();
    let payload = microsoft_calling::user_invite_payload(
        &payload_context(params),
        &callback,
        callee_mri,
        &callee_participant_id,
        include_video,
    );
    for (index, add_url) in add_urls.iter().enumerate() {
        let invite_msg_id = uuid::Uuid::new_v4().to_string();
        tracing::info!(
            "Inviting user {} -> POST {} (msg_id={})",
            callee_mri,
            add_url,
            invite_msg_id
        );
        let resp = call_controller_post(http, add_url, params, &invite_msg_id)
            .json(&payload)
            .send()
            .await
            .context("Failed to POST user invite")?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        tracing::info!("User invite response: {} ({} bytes)", status, body.len());
        if status.is_success() {
            return Ok(());
        }
        if status == reqwest::StatusCode::NOT_FOUND && index + 1 < add_urls.len() {
            tracing::warn!(
                "User invite endpoint {} returned 404; retrying fallback",
                add_url
            );
            continue;
        }
        anyhow::bail!("User invite failed at {} ({}): {}", add_url, status, body);
    }

    anyhow::bail!("No user invite endpoint is available")
}

/// End a call by POSTing to the end URL.
pub async fn end_call(
    http: &reqwest::Client,
    skype_token: &str,
    notification: &CallNotification,
) -> Result<()> {
    let end_url = notification
        .call_invitation
        .as_ref()
        .context("No callInvitation in notification")?;
    let end_url = end_url
        .links
        .as_ref()
        .context("No links in callInvitation")?;
    let end_url = end_url
        .end
        .as_ref()
        .context("No end URL in links")?
        .to_owned();

    tracing::info!("Ending call -> POST {}", end_url);

    let resp = skype_post(http, &end_url, skype_token)
        .json(&microsoft_calling::end_call_payload())
        .send()
        .await
        .context("Failed to POST end call")?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if status.is_success() {
        tracing::info!("Call ended ({}): {}", status, body);
        Ok(())
    } else {
        anyhow::bail!("Call end failed ({}): {}", status, body);
    }
}

#[cfg(test)]
mod tests {
    use super::response_link;

    #[test]
    fn response_link_reads_root_and_wrapped_links() {
        let root = serde_json::json!({
            "links": {
                "addParticipant": "https://root/add",
                "active": "https://root/active"
            }
        });
        let wrapped = serde_json::json!({
            "conversationResponse": {
                "links": {
                    "addParticipant": "https://wrapped/add",
                    "active": "https://wrapped/active"
                }
            }
        });

        assert_eq!(
            response_link(&root, "addParticipant"),
            Some("https://root/add")
        );
        assert_eq!(response_link(&root, "active"), Some("https://root/active"));
        assert_eq!(
            response_link(&wrapped, "addParticipant"),
            Some("https://wrapped/add")
        );
        assert_eq!(
            response_link(&wrapped, "active"),
            Some("https://wrapped/active")
        );
    }
}
