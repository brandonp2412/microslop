//! Skype token exchange
//!
//! After obtaining an AAD access token, exchange it for a Skype token
//! via the Teams authsvc endpoint. The Skype token is what Teams APIs
//! actually require for most operations.

use anyhow::{bail, Context, Result};
use ost_microsoft::auth as microsoft_auth;
use serde::Deserialize;

/// Response from Teams authsvc token exchange
#[derive(Debug, Deserialize)]
pub struct AuthzResponse {
    pub tokens: Option<AuthzTokens>,
    #[serde(rename = "skypeToken")]
    pub personal_token: Option<PersonalAuthzTokens>,
    #[serde(rename = "regionGtms")]
    pub region_gtms: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct AuthzTokens {
    #[serde(rename = "skypeToken")]
    pub skype_token: Option<String>,
    #[serde(rename = "expiresIn")]
    pub expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct PersonalAuthzTokens {
    pub skypetoken: Option<String>,
    #[serde(rename = "expiresIn")]
    pub expires_in: Option<u64>,
}

/// Exchange an AAD access token for a Skype token.
/// Returns (skype_token, expires_in_secs, region_gtms).
pub async fn exchange_skype_token(
    aad_token: &str,
    personal: bool,
) -> Result<(String, Option<u64>, Option<serde_json::Value>)> {
    let url = if personal {
        microsoft_auth::PERSONAL_AUTHZ_URL
    } else {
        microsoft_auth::WORK_AUTHZ_URL
    };

    tracing::debug!("Exchanging AAD token for Skype token at {}", url);

    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .bearer_auth(aad_token)
        .header("Content-Length", "0")
        .send()
        .await
        .context("Failed to call authsvc for Skype token exchange")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!(
            "Skype token exchange failed (HTTP {}): {}",
            status.as_u16(),
            body
        );
    }

    let authz: AuthzResponse = resp
        .json()
        .await
        .context("Failed to parse authsvc response")?;

    let (skype_token, expires_in) = if let Some(tokens) = authz.tokens {
        (
            tokens
                .skype_token
                .context("authsvc response missing 'skypeToken'")?,
            tokens.expires_in,
        )
    } else if let Some(tokens) = authz.personal_token {
        (
            tokens
                .skypetoken
                .context("personal authz response missing 'skypetoken'")?,
            tokens.expires_in,
        )
    } else {
        bail!("authsvc response missing Skype token");
    };

    Ok((skype_token, expires_in, authz.region_gtms))
}
