//! Trouter v4 session negotiation

use anyhow::{Context, Result};
use ost_microsoft::calling as microsoft_calling;

pub use microsoft_calling::TrouterSession as SessionResponse;

/// Negotiate a Trouter session, returning connection parameters.
pub async fn negotiate(
    http: &reqwest::Client,
    skype_token: &str,
) -> Result<(SessionResponse, String)> {
    let epid = uuid::Uuid::new_v4().to_string();
    let url = microsoft_calling::trouter_negotiation_url(&epid);

    tracing::info!("Negotiating trouter session (epid={})", epid);

    let resp = http
        .get(&url)
        .header(microsoft_calling::SKYPE_TOKEN_HEADER, skype_token)
        .send()
        .await
        .context("Trouter session negotiation request failed")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Trouter session negotiation failed: {} — {}", status, body);
    }

    let session: SessionResponse = resp
        .json()
        .await
        .context("Failed to parse trouter session response")?;

    tracing::info!("Trouter session negotiated: socketio={}", session.socketio);
    tracing::debug!("Trouter surl={}", session.surl);

    Ok((session, epid))
}

/// Get a socket.io session ID by sending an authenticated GET request.
pub async fn get_session_id(
    _http: &reqwest::Client,
    session: &SessionResponse,
    skype_token: &str,
    epid: &str,
) -> Result<String> {
    let url = session.session_url(epid);

    tracing::info!("Getting socket.io session ID...");
    tracing::debug!("Session URL: {}", url);

    // Squads uses a no-redirect client for this request
    let no_redirect = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("Failed to build HTTP client")?;

    let resp = no_redirect
        .get(&url)
        .header(microsoft_calling::SKYPE_TOKEN_HEADER, skype_token)
        .send()
        .await
        .context("Socket.io session request failed")?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        anyhow::bail!("Socket.io session request failed: {} — {}", status, text);
    }
    tracing::debug!("Session response: {}", text);

    // Format: "{session_id}:180:180:websocket,xhr-polling"
    let session_id = text
        .split(':')
        .next()
        .context("Empty session response")?
        .to_string();

    tracing::info!("Got socket.io session ID: {}", session_id);
    Ok(session_id)
}
