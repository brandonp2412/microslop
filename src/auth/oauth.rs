//! OAuth2 device code flow for Azure AD, plus Skype token exchange

use anyhow::{Context, Result};
use oauth2::{
    basic::BasicClient, AuthUrl, ClientId, DeviceAuthorizationUrl, RefreshToken, Scope,
    StandardDeviceAuthorizationResponse, TokenResponse, TokenUrl,
};

use super::skype::exchange_skype_token;
use super::{AuthConfig, TokenStore};
use crate::config::Config;
use ost_microsoft::auth as microsoft_auth;

fn auth_config(personal: bool) -> AuthConfig {
    if personal {
        AuthConfig::personal()
    } else {
        AuthConfig::work()
    }
}

fn build_client(auth_config: &AuthConfig) -> Result<BasicClient> {
    let auth_url = AuthUrl::new(microsoft_auth::oauth_url(auth_config.tenant, "authorize"))?;
    let token_url = TokenUrl::new(microsoft_auth::oauth_url(auth_config.tenant, "token"))?;
    let device_url =
        DeviceAuthorizationUrl::new(microsoft_auth::oauth_url(auth_config.tenant, "devicecode"))?;

    Ok(BasicClient::new(
        ClientId::new(auth_config.client_id.to_string()),
        None,
        auth_url,
        Some(token_url),
    )
    .set_device_authorization_url(device_url))
}

async fn acquire_skype_exchange_token(
    client: &BasicClient,
    refresh_token_str: &str,
    personal: bool,
) -> Result<String> {
    let scope = if personal {
        microsoft_auth::PERSONAL_SKYPE_SCOPE
    } else {
        microsoft_auth::TEAMS_SCOPE
    };
    let token_response = client
        .exchange_refresh_token(&RefreshToken::new(refresh_token_str.to_string()))
        .add_scope(Scope::new(scope.to_string()))
        .request_async(oauth2::reqwest::async_http_client)
        .await
        .context("Failed to acquire Skype exchange token")?;
    Ok(token_response.access_token().secret().to_string())
}

async fn acquire_ic3_token(
    client: &BasicClient,
    refresh_token_str: &str,
) -> Result<(String, Option<u64>)> {
    let token_response = client
        .exchange_refresh_token(&RefreshToken::new(refresh_token_str.to_string()))
        .add_scope(Scope::new(microsoft_auth::IC3_RESOURCE_SCOPE.to_string()))
        .add_scope(Scope::new("offline_access".to_string()))
        .request_async(oauth2::reqwest::async_http_client)
        .await
        .context("Failed to acquire IC3 token")?;

    Ok((
        token_response.access_token().secret().to_string(),
        token_response.expires_in().map(|d| d.as_secs()),
    ))
}

/// Acquire a recorder service AAD token (audience: 4580fd1d-e5a3-4f56-9ad1-aab0e3bf8f76).
async fn acquire_recorder_token(
    client: &BasicClient,
    refresh_token_str: &str,
) -> Result<(String, Option<u64>)> {
    let token_response = client
        .exchange_refresh_token(&RefreshToken::new(refresh_token_str.to_string()))
        .add_scope(Scope::new(microsoft_auth::RECORDER_SCOPE.to_string()))
        .add_scope(Scope::new("offline_access".to_string()))
        .request_async(oauth2::reqwest::async_http_client)
        .await
        .context("Failed to acquire recorder token")?;

    Ok((
        token_response.access_token().secret().to_string(),
        token_response.expires_in().map(|d| d.as_secs()),
    ))
}

/// Acquire a Graph API token by exchanging the refresh token with Graph scope.
async fn acquire_graph_token(
    client: &BasicClient,
    refresh_token_str: &str,
) -> Result<(String, Option<u64>)> {
    let token_response = client
        .exchange_refresh_token(&RefreshToken::new(refresh_token_str.to_string()))
        .add_scope(Scope::new(microsoft_auth::GRAPH_RESOURCE_SCOPE.to_string()))
        .add_scope(Scope::new("offline_access".to_string()))
        .request_async(oauth2::reqwest::async_http_client)
        .await
        .context("Failed to acquire Graph token")?;

    Ok((
        token_response.access_token().secret().to_string(),
        token_response.expires_in().map(|d| d.as_secs()),
    ))
}

/// Refresh the AAD access token using a stored refresh_token, then
/// re-exchange for a Skype token. Returns Ok(true) if refresh succeeded.
pub async fn refresh() -> Result<bool> {
    let mut config = Config::load()?;
    let refresh_token_str = match config.get_refresh_token() {
        Some(rt) => rt,
        None => return Ok(false),
    };

    let auth_config = auth_config(config.personal);
    let client = build_client(&auth_config)?;

    tracing::info!("Refreshing AAD token...");

    let token_response = client
        .exchange_refresh_token(&RefreshToken::new(refresh_token_str))
        .add_scope(Scope::new(auth_config.scope.to_string()))
        .request_async(oauth2::reqwest::async_http_client)
        .await
        .context("Failed to refresh AAD token")?;

    config.set_access_token(
        token_response.access_token().secret().to_string(),
        token_response.expires_in().map(|d| d.as_secs()),
    );

    if let Some(new_rt) = token_response.refresh_token() {
        config.set_refresh_token(new_rt.secret().to_string());
    }

    let aad_token = if config.personal {
        let refresh_token = config
            .get_refresh_token()
            .context("Personal account refresh token is missing")?;
        acquire_skype_exchange_token(&client, &refresh_token, true).await?
    } else {
        token_response.access_token().secret().to_string()
    };
    match exchange_skype_token(&aad_token, config.personal).await {
        Ok((skype_tok, expires_in, region_gtms)) => {
            config.set_skype_token(skype_tok, expires_in);
            if let Some(gtms) = region_gtms {
                config.set_region_gtms(gtms);
            }
            tracing::info!("Skype token refreshed");
        }
        Err(e) => {
            tracing::warn!("Skype token exchange failed during refresh: {:#}", e);
        }
    }

    let rt_for_graph = config.get_refresh_token().unwrap_or_default();
    if !rt_for_graph.is_empty() {
        match acquire_graph_token(&client, &rt_for_graph).await {
            Ok((graph_tok, expires_in)) => {
                config.set_graph_token(graph_tok, expires_in);
                tracing::info!("Graph token acquired");
            }
            Err(e) => {
                tracing::warn!("Graph token acquisition failed: {:#}", e);
            }
        }
    }

    if !config.personal {
        let rt_for_ic3 = config.get_refresh_token().unwrap_or_default();
        if !rt_for_ic3.is_empty() {
            match acquire_ic3_token(&client, &rt_for_ic3).await {
                Ok((ic3_tok, expires_in)) => {
                    config.set_ic3_token(ic3_tok, expires_in);
                    tracing::info!("IC3 token acquired");
                }
                Err(e) => {
                    tracing::warn!("IC3 token acquisition failed: {:#}", e);
                }
            }
        }
    }

    if !config.personal {
        let rt_for_recorder = config.get_refresh_token().unwrap_or_default();
        if !rt_for_recorder.is_empty() {
            match acquire_recorder_token(&client, &rt_for_recorder).await {
                Ok((rec_tok, expires_in)) => {
                    config.set_recorder_token(rec_tok, expires_in);
                    tracing::info!("Recorder token acquired");
                }
                Err(e) => {
                    tracing::warn!("Recorder token acquisition failed: {:#}", e);
                }
            }
        }
    }

    config.save()?;
    tracing::info!("Token refresh complete");
    Ok(true)
}

pub async fn login(force: bool, personal: bool) -> Result<()> {
    {
        let config = Config::load()?;

        if !force && config.personal == personal {
            if let Some(token) = config.get_access_token() {
                if !token.is_expired() {
                    let missing_tokens = !personal
                        && (config.get_ic3_token().is_none()
                            || config.get_recorder_token().is_none());
                    if missing_tokens && config.get_refresh_token().is_some() {
                        tracing::info!(
                            "AAD token valid but some derived tokens missing, refreshing..."
                        );
                        if let Ok(true) = refresh().await {
                            println!("Tokens refreshed (acquired missing derived tokens).");
                            return Ok(());
                        }
                    }
                    println!(
                        "Already logged in (AAD token valid). Use --force to re-authenticate."
                    );
                    return Ok(());
                }
                if config.get_refresh_token().is_some() {
                    tracing::info!("AAD token expired, attempting refresh...");
                    match refresh().await {
                        Ok(true) => {
                            println!("Token refreshed successfully.");
                            return Ok(());
                        }
                        Ok(false) => {}
                        Err(e) => {
                            tracing::warn!("Refresh failed, falling back to device code: {:#}", e);
                        }
                    }
                }
            }
        }
    }

    let auth_config = auth_config(personal);
    let client = build_client(&auth_config)?;

    tracing::info!("Initiating device code flow...");

    let device_auth_response: StandardDeviceAuthorizationResponse = client
        .exchange_device_code()?
        .add_scope(Scope::new(auth_config.scope.to_string()))
        .request_async(oauth2::reqwest::async_http_client)
        .await
        .context("Failed to request device code")?;

    let verification_url = device_auth_response.verification_uri().as_str();
    let user_code = device_auth_response.user_code().secret();

    println!();
    println!("To sign in, visit: {}", verification_url);
    println!("Enter code:        {}", user_code);
    println!();

    tracing::info!("Waiting for authentication...");

    let token_response = client
        .exchange_device_access_token(&device_auth_response)
        .request_async(oauth2::reqwest::async_http_client, tokio::time::sleep, None)
        .await
        .context("Failed to exchange device code for token")?;

    let mut config = Config::load()?;
    config.personal = personal;
    config.tenant_id = if personal {
        Some(microsoft_auth::PERSONAL_TENANT_ID.to_string())
    } else {
        jwt_string_claim(token_response.access_token().secret(), "tid")
    };
    config.set_access_token(
        token_response.access_token().secret().to_string(),
        token_response.expires_in().map(|d| d.as_secs()),
    );

    if let Some(refresh_token) = token_response.refresh_token() {
        config.set_refresh_token(refresh_token.secret().to_string());
    }

    let aad_token = if personal {
        let refresh_token = config
            .get_refresh_token()
            .context("Personal account refresh token is missing")?;
        acquire_skype_exchange_token(&client, &refresh_token, true).await?
    } else {
        token_response.access_token().secret().to_string()
    };
    let mut skype_ok = false;
    match exchange_skype_token(&aad_token, personal).await {
        Ok((skype_tok, expires_in, region_gtms)) => {
            config.set_skype_token(skype_tok, expires_in);
            if let Some(gtms) = region_gtms {
                config.set_region_gtms(gtms);
            }
            skype_ok = true;
        }
        Err(e) => {
            tracing::warn!("Skype token exchange failed: {:#}", e);
            eprintln!("Warning: Skype token exchange failed; some operations may not work.");
        }
    }

    let mut graph_ok = false;
    if let Some(ref rt) = config.get_refresh_token() {
        match acquire_graph_token(&client, rt).await {
            Ok((graph_tok, expires_in)) => {
                config.set_graph_token(graph_tok, expires_in);
                graph_ok = true;
            }
            Err(e) => {
                tracing::warn!("Graph token acquisition failed: {:#}", e);
                eprintln!("Warning: Graph token acquisition failed; whoami/chats may not work.");
            }
        }
    }

    let mut ic3_ok = personal;
    if !personal {
        if let Some(ref rt) = config.get_refresh_token() {
            match acquire_ic3_token(&client, rt).await {
                Ok((ic3_tok, expires_in)) => {
                    config.set_ic3_token(ic3_tok, expires_in);
                    ic3_ok = true;
                }
                Err(e) => {
                    tracing::warn!("IC3 token acquisition failed: {:#}", e);
                    eprintln!("Warning: IC3 token acquisition failed; trouter may not work.");
                }
            }
        }
    }

    let mut recorder_ok = personal;
    if !personal {
        if let Some(ref rt) = config.get_refresh_token() {
            match acquire_recorder_token(&client, rt).await {
                Ok((rec_tok, expires_in)) => {
                    config.set_recorder_token(rec_tok, expires_in);
                    recorder_ok = true;
                }
                Err(e) => {
                    tracing::warn!("Recorder token acquisition failed: {:#}", e);
                    eprintln!(
                        "Warning: Recorder token acquisition failed; recording may not work."
                    );
                }
            }
        }
    }

    config.save()?;
    if skype_ok && graph_ok && ic3_ok && recorder_ok {
        println!("Login successful.");
    } else {
        println!(
            "Login partially successful (missing: {}).",
            [
                (!skype_ok).then_some("Skype"),
                (!graph_ok).then_some("Graph"),
                (!ic3_ok).then_some("IC3"),
                (!recorder_ok).then_some("Recorder")
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ")
        );
    }
    Ok(())
}

fn jwt_string_claim(token: &str, claim: &str) -> Option<String> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let payload = token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice::<serde_json::Value>(&decoded)
        .ok()?
        .get(claim)?
        .as_str()
        .map(ToOwned::to_owned)
}

pub async fn logout() -> Result<()> {
    let mut config = Config::load()?;
    config.clear_tokens();
    config.save()?;
    println!("Logged out.");
    Ok(())
}

pub async fn status() -> Result<()> {
    let config = Config::load()?;

    match config.get_access_token() {
        Some(token) if !token.is_expired() => {
            println!("AAD token:   valid");
            if let Some(exp) = token.expires_at {
                println!("  expires_at: {}", exp);
            }
        }
        Some(_) => {
            println!("AAD token:   expired");
        }
        None => {
            println!("AAD token:   none");
        }
    }

    match config.get_refresh_token() {
        Some(_) => println!("Refresh tok: present"),
        None => println!("Refresh tok: none"),
    }

    match config.get_graph_token() {
        Some(token) if !token.is_expired() => {
            println!("Graph token: valid");
            if let Some(exp) = token.expires_at {
                println!("  expires_at: {}", exp);
            }
        }
        Some(_) => {
            println!("Graph token: expired");
        }
        None => {
            println!("Graph token: none");
        }
    }

    match config.get_ic3_token() {
        Some(token) if !token.is_expired() => {
            println!("IC3 token:   valid");
            if let Some(exp) = token.expires_at {
                println!("  expires_at: {}", exp);
            }
        }
        Some(_) => {
            println!("IC3 token:   expired");
        }
        None => {
            println!("IC3 token:   none");
        }
    }

    match config.get_recorder_token() {
        Some(token) if !token.is_expired() => {
            println!("Recorder tk: valid");
            if let Some(exp) = token.expires_at {
                println!("  expires_at: {}", exp);
            }
        }
        Some(_) => {
            println!("Recorder tk: expired");
        }
        None => {
            println!("Recorder tk: none");
        }
    }

    match config.get_skype_token() {
        Some(token) if !token.is_expired() => {
            println!("Skype token: valid");
            if let Some(exp) = token.expires_at {
                println!("  expires_at: {}", exp);
            }
        }
        Some(_) => {
            println!("Skype token: expired");
        }
        None => {
            println!("Skype token: none");
        }
    }

    if config.region_gtms.is_some() {
        println!("Region GTMs: present");
    } else {
        println!("Region GTMs: none");
    }

    if config.get_access_token().is_none() {
        println!("\nRun 'teams-cli login' to authenticate.");
    }

    Ok(())
}
