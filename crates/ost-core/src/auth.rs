use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use ost_microsoft::auth as microsoft_auth;
use ost_platform::{PlatformSecretStorage, SecretStorage};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

const CLIENT_ID: &str = microsoft_auth::WORK_CLIENT_ID;
const AUTHORITY: &str = microsoft_auth::ORGANIZATIONS_AUTHORITY;
const TEAMS_SCOPE: &str = microsoft_auth::TEAMS_SCOPE;
const GRAPH_SCOPE: &str = microsoft_auth::GRAPH_SCOPE;
const IC3_SCOPE: &str = microsoft_auth::IC3_SCOPE;
const TEAMS_AUTHZ_URL: &str = microsoft_auth::WORK_AUTHZ_URL;
const ACTIVE_ACCOUNT_KEY: &str = "active-account-id";
const SAVED_ACCOUNTS_KEY: &str = "saved-accounts-v1";

#[derive(Clone, Debug, Serialize)]
pub struct DeviceCode {
    pub verification_uri: String,
    pub user_code: String,
    pub expires_in_seconds: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthStatus {
    pub signed_in: bool,
    pub login_in_progress: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SavedAccount {
    pub id: String,
    pub display_name: String,
    pub username: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredAccount {
    id: String,
    display_name: String,
    username: String,
    refresh_token: String,
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: Option<u64>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Deserialize)]
struct TeamsAuthzResponse {
    tokens: Option<TeamsAuthzTokens>,
    #[serde(rename = "regionGtms")]
    region_gtms: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct TeamsAuthzTokens {
    #[serde(rename = "skypeToken")]
    skype_token: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CallAuth {
    pub teams_access_token: String,
    pub skype_token: String,
    pub ic3_token: String,
    pub graph_token: Option<String>,
    pub region_gtms: serde_json::Value,
    pub tenant_id: String,
}

#[derive(Clone, Debug)]
struct SkypeAuth {
    token: String,
    region_gtms: Option<serde_json::Value>,
}

#[derive(Clone)]
struct PendingDeviceLogin {
    device_code: String,
    expires_at: Instant,
    interval: Duration,
    cancelled: Arc<AtomicBool>,
}

pub struct AuthService {
    client: reqwest::Client,
    pending_login: Mutex<Option<PendingDeviceLogin>>,
    teams_access_token: Mutex<Option<String>>,
    skype_token: Mutex<Option<String>>,
    graph_token: Mutex<Option<String>>,
    region_gtms: Mutex<Option<serde_json::Value>>,
    tenant_id: Mutex<Option<String>>,
    refresh_token: Mutex<Option<String>>,
    refresh_gate: Mutex<()>,
    persist_credentials: bool,
}

impl Default for AuthService {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthService {
    pub fn new() -> Self {
        Self::with_persistence(true)
    }

    pub fn ephemeral() -> Self {
        Self::with_persistence(false)
    }

    fn with_persistence(persist_credentials: bool) -> Self {
        Self {
            client: reqwest::Client::new(),
            pending_login: Mutex::new(None),
            teams_access_token: Mutex::new(None),
            skype_token: Mutex::new(None),
            graph_token: Mutex::new(None),
            region_gtms: Mutex::new(None),
            tenant_id: Mutex::new(None),
            refresh_token: Mutex::new(None),
            refresh_gate: Mutex::new(()),
            persist_credentials,
        }
    }

    pub async fn status(&self) -> Result<AuthStatus> {
        let signed_in = tokio::task::spawn_blocking(|| PlatformSecretStorage.has("refresh-token"))
            .await
            .context("Could not join the secure-storage status check")??;
        Ok(AuthStatus {
            signed_in,
            login_in_progress: self.pending_login.lock().await.is_some(),
        })
    }

    pub async fn saved_accounts(&self) -> Result<Vec<SavedAccount>> {
        tokio::task::spawn_blocking(|| {
            Ok(load_stored_accounts(&PlatformSecretStorage)?
                .into_iter()
                .map(|account| SavedAccount {
                    id: account.id,
                    display_name: account.display_name,
                    username: account.username,
                })
                .collect())
        })
        .await
        .context("Could not join the saved-account read")?
    }

    pub async fn active_account_id(&self) -> Result<Option<String>> {
        tokio::task::spawn_blocking(|| PlatformSecretStorage.get(ACTIVE_ACCOUNT_KEY))
            .await
            .context("Could not join the active-account read")?
    }

    pub async fn switch_account(&self, account_id: &str) -> Result<AuthStatus> {
        let account_id = account_id.trim().to_owned();
        let refresh_token = tokio::task::spawn_blocking(move || {
            let storage = PlatformSecretStorage;
            let account = load_stored_accounts(&storage)?
                .into_iter()
                .find(|account| account.id == account_id)
                .context("Saved Microsoft account was not found")?;
            storage.set("refresh-token", &account.refresh_token)?;
            storage.set(ACTIVE_ACCOUNT_KEY, &account.id)?;
            Ok::<_, anyhow::Error>(account.refresh_token)
        })
        .await
        .context("Could not join the account switch")??;
        self.clear_runtime_session().await;
        if let Err(error) = crate::teams::clear_session_cache().await {
            tracing::warn!("Could not clear the Teams cache during account switch: {error:#}");
        }
        self.restore_from_refresh_token(refresh_token, true).await?;
        self.status().await
    }

    pub async fn restore_saved_account(&self, account_id: &str) -> Result<()> {
        let account_id = account_id.trim().to_owned();
        let refresh_token = tokio::task::spawn_blocking(move || {
            load_stored_accounts(&PlatformSecretStorage)?
                .into_iter()
                .find(|account| account.id == account_id)
                .map(|account| account.refresh_token)
                .context("Saved Microsoft account was not found")
        })
        .await
        .context("Could not join the saved-account credential read")??;
        self.restore_from_refresh_token(refresh_token, false).await
    }

    pub async fn begin_work_login(&self) -> Result<DeviceCode> {
        let response = self
            .client
            .post(format!("{AUTHORITY}/devicecode"))
            .form(&[("client_id", CLIENT_ID), ("scope", TEAMS_SCOPE)])
            .send()
            .await
            .context("Could not request a Microsoft device code")?
            .error_for_status()
            .context("Microsoft rejected the device-code request")?
            .json::<DeviceCodeResponse>()
            .await
            .context("Could not parse the Microsoft device-code response")?;

        let device_code = DeviceCode {
            verification_uri: response.verification_uri,
            user_code: response.user_code,
            expires_in_seconds: response.expires_in,
        };
        *self.pending_login.lock().await = Some(PendingDeviceLogin {
            device_code: response.device_code,
            expires_at: Instant::now() + Duration::from_secs(response.expires_in),
            interval: Duration::from_secs(response.interval.unwrap_or(5)),
            cancelled: Arc::new(AtomicBool::new(false)),
        });
        Ok(device_code)
    }

    pub async fn restore_work_session(&self) -> Result<AuthStatus> {
        let Some(refresh_token) =
            tokio::task::spawn_blocking(|| PlatformSecretStorage.get("refresh-token"))
                .await
                .context("Could not join the secure-storage session restore")??
        else {
            return self.status().await;
        };
        self.restore_from_refresh_token(refresh_token, true).await?;
        self.status().await
    }

    pub async fn complete_work_login(&self) -> Result<AuthStatus> {
        let pending = self
            .pending_login
            .lock()
            .await
            .as_ref()
            .cloned()
            .context("No active device-code login")?;
        let mut poll_interval = pending.interval;
        loop {
            if pending.cancelled.load(Ordering::Relaxed) {
                *self.pending_login.lock().await = None;
                bail!("Device-code login was cancelled");
            }
            if Instant::now() >= pending.expires_at {
                *self.pending_login.lock().await = None;
                bail!("Device code expired before sign-in completed");
            }

            let response = match self
                .client
                .post(format!("{AUTHORITY}/token"))
                .form(&[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", CLIENT_ID),
                    ("device_code", pending.device_code.as_str()),
                ])
                .send()
                .await
            {
                Ok(response) => response,
                Err(error) if error.is_connect() || error.is_timeout() => {
                    poll_interval = poll_interval
                        .saturating_mul(2)
                        .min(Duration::from_secs(60));
                    tracing::warn!(
                        "Transient Microsoft device-code polling failure; retrying in {:?}: {error}",
                        poll_interval
                    );
                    tokio::time::sleep(poll_interval).await;
                    continue;
                }
                Err(error) => {
                    return Err(error)
                        .context("Could not poll Microsoft for device-code completion");
                }
            }
            .json::<TokenResponse>()
            .await
            .context("Could not parse the Microsoft token response")?;

            match response.error.as_deref() {
                None => {
                    let access_token = response
                        .access_token
                        .context("Microsoft returned no access token")?;
                    let refresh_token = response
                        .refresh_token
                        .context("Microsoft returned no refresh token")?;
                    let skype_auth = self.exchange_skype_token(&access_token).await?;
                    let tenant_id = jwt_string_claim(&access_token, "tid");
                    *self.refresh_token.lock().await = Some(refresh_token.clone());
                    if self.persist_credentials {
                        let access_token_to_cache = access_token.clone();
                        let refresh_token_to_cache = refresh_token.clone();
                        tokio::task::spawn_blocking(move || {
                            cache_account_session(
                                &PlatformSecretStorage,
                                &access_token_to_cache,
                                &refresh_token_to_cache,
                            )
                        })
                        .await
                        .context("Could not join the secure-storage account write")??;
                    }
                    if let Err(error) = crate::teams::clear_session_cache().await {
                        tracing::warn!("Could not clear the previous Teams cache: {error:#}");
                    }
                    *self.teams_access_token.lock().await = Some(access_token);
                    *self.skype_token.lock().await = Some(skype_auth.token);
                    *self.region_gtms.lock().await = skype_auth.region_gtms;
                    *self.tenant_id.lock().await = tenant_id;
                    let _ = self.refresh_graph_token().await;
                    *self.pending_login.lock().await = None;
                    return self.status().await;
                }
                Some("authorization_pending") => tokio::time::sleep(poll_interval).await,
                Some("slow_down") => {
                    poll_interval = poll_interval.saturating_add(Duration::from_secs(5));
                    tokio::time::sleep(poll_interval).await;
                }
                Some("authorization_declined") => {
                    *self.pending_login.lock().await = None;
                    bail!("Microsoft sign-in was declined");
                }
                Some("expired_token") => {
                    *self.pending_login.lock().await = None;
                    bail!("Device code expired before sign-in completed");
                }
                Some(error) => {
                    *self.pending_login.lock().await = None;
                    bail!(
                        "Microsoft sign-in failed: {}",
                        response
                            .error_description
                            .unwrap_or_else(|| error.to_owned())
                    );
                }
            }
        }
    }

    async fn restore_from_refresh_token(
        &self,
        refresh_token: String,
        persist_account: bool,
    ) -> Result<()> {
        *self.refresh_token.lock().await = Some(refresh_token.clone());
        let response = self
            .refresh_access_token(&refresh_token, TEAMS_SCOPE)
            .await?;
        let access_token = response
            .access_token
            .context("Microsoft returned no access token while restoring the session")?;
        let refresh_token = response
            .refresh_token
            .unwrap_or_else(|| refresh_token.clone());
        *self.refresh_token.lock().await = Some(refresh_token.clone());
        let skype_auth = self.exchange_skype_token(&access_token).await?;
        let tenant_id = jwt_string_claim(&access_token, "tid");
        *self.teams_access_token.lock().await = Some(access_token.clone());
        *self.skype_token.lock().await = Some(skype_auth.token);
        *self.region_gtms.lock().await = skype_auth.region_gtms;
        *self.tenant_id.lock().await = tenant_id;
        if persist_account && self.persist_credentials {
            let access_token_to_cache = access_token.clone();
            let refresh_token_to_cache = refresh_token.clone();
            tokio::task::spawn_blocking(move || {
                cache_account_session(
                    &PlatformSecretStorage,
                    &access_token_to_cache,
                    &refresh_token_to_cache,
                )
            })
            .await
            .context("Could not join the secure-storage restored-account write")??;
        }
        let _ = self.refresh_graph_token().await;
        Ok(())
    }

    pub async fn cancel_work_login(&self) {
        if let Some(pending) = self.pending_login.lock().await.as_ref() {
            pending.cancelled.store(true, Ordering::Relaxed);
        }
    }

    pub async fn logout(&self) -> Result<AuthStatus> {
        tokio::task::spawn_blocking(|| {
            let store = PlatformSecretStorage;
            if let Some(active_account_id) = store.get(ACTIVE_ACCOUNT_KEY)? {
                let mut accounts = load_stored_accounts(&store)?;
                accounts.retain(|account| account.id != active_account_id);
                save_stored_accounts(&store, &accounts)?;
            }
            store.delete(ACTIVE_ACCOUNT_KEY)?;
            store.delete("refresh-token")?;
            store.delete("skype-token")
        })
        .await
        .context("Could not join the secure-storage logout")??;
        self.clear_runtime_session().await;
        if let Err(error) = crate::teams::clear_session_cache().await {
            tracing::warn!("Could not clear the Teams cache during sign out: {error:#}");
        }
        self.status().await
    }

    async fn clear_runtime_session(&self) {
        *self.pending_login.lock().await = None;
        *self.teams_access_token.lock().await = None;
        *self.skype_token.lock().await = None;
        *self.graph_token.lock().await = None;
        *self.region_gtms.lock().await = None;
        *self.tenant_id.lock().await = None;
        *self.refresh_token.lock().await = None;
    }

    pub(crate) async fn skype_token(&self) -> Result<String> {
        self.skype_token
            .lock()
            .await
            .clone()
            .context("No active Teams session. Sign in again.")
    }

    pub(crate) async fn teams_access_token(&self) -> Result<String> {
        self.teams_access_token
            .lock()
            .await
            .clone()
            .context("No active Teams session. Sign in again.")
    }

    pub(crate) async fn graph_token(&self) -> Result<String> {
        self.graph_token
            .lock()
            .await
            .clone()
            .context("Microsoft Graph is unavailable for this account. Sign in again or ask an administrator to grant the required consent.")
    }

    pub async fn call_auth(&self) -> Result<CallAuth> {
        let refresh_token =
            tokio::task::spawn_blocking(|| PlatformSecretStorage.get("refresh-token"))
                .await
                .context("Could not join the secure-storage call credential read")??
                .context("No cached Microsoft refresh token. Sign in again.")?;
        let ic3_token = self
            .refresh_access_token(&refresh_token, IC3_SCOPE)
            .await?
            .access_token
            .context("Microsoft returned no IC3 access token")?;
        self.refresh_skype_token().await?;
        let _ = self.refresh_graph_token().await;
        let skype_token = self.skype_token().await?;
        let region_gtms = self
            .region_gtms
            .lock()
            .await
            .clone()
            .context("Teams did not provide regional calling endpoints. Sign in again.")?;
        let tenant_id =
            self.tenant_id.lock().await.clone().context(
                "The Microsoft sign-in token did not contain a tenant id. Sign in again.",
            )?;
        let graph_token = self.graph_token.lock().await.clone();
        let teams_access_token = self.teams_access_token().await?;
        Ok(CallAuth {
            teams_access_token,
            skype_token,
            ic3_token,
            graph_token,
            region_gtms,
            tenant_id,
        })
    }

    pub(crate) async fn region_gtm(&self, key: &str) -> Option<String> {
        self.region_gtms
            .lock()
            .await
            .as_ref()?
            .get(key)?
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    }

    pub(crate) async fn refresh_graph_token(&self) -> Result<()> {
        let refresh_token = self.cached_refresh_token().await?;
        let response = self
            .refresh_access_token(&refresh_token, GRAPH_SCOPE)
            .await?;
        let token = response
            .access_token
            .context("Microsoft returned no Graph access token")?;
        *self.graph_token.lock().await = Some(token);
        Ok(())
    }

    pub(crate) async fn refresh_skype_token(&self) -> Result<()> {
        let refresh_token = self.cached_refresh_token().await?;
        let teams_token = self
            .refresh_access_token(&refresh_token, TEAMS_SCOPE)
            .await?
            .access_token
            .context("Microsoft returned no Teams access token")?;
        let tenant_id = jwt_string_claim(&teams_token, "tid");
        let skype_auth = self.exchange_skype_token(&teams_token).await?;
        *self.teams_access_token.lock().await = Some(teams_token);
        *self.skype_token.lock().await = Some(skype_auth.token);
        *self.region_gtms.lock().await = skype_auth.region_gtms;
        if tenant_id.is_some() {
            *self.tenant_id.lock().await = tenant_id;
        }
        Ok(())
    }

    async fn cached_refresh_token(&self) -> Result<String> {
        if let Some(refresh_token) = self.refresh_token.lock().await.clone() {
            return Ok(refresh_token);
        }
        let refresh_token =
            tokio::task::spawn_blocking(|| PlatformSecretStorage.get("refresh-token"))
                .await
                .context("Could not join the secure-storage token read")??
                .context("No cached Microsoft refresh token. Sign in again.")?;
        *self.refresh_token.lock().await = Some(refresh_token.clone());
        Ok(refresh_token)
    }

    async fn refresh_access_token(
        &self,
        refresh_token: &str,
        scope: &str,
    ) -> Result<TokenResponse> {
        let _refresh_guard = self.refresh_gate.lock().await;
        let refresh_token = self
            .refresh_token
            .lock()
            .await
            .clone()
            .unwrap_or_else(|| refresh_token.to_owned());
        let response = self
            .client
            .post(format!("{AUTHORITY}/token"))
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", CLIENT_ID),
                ("scope", scope),
                ("refresh_token", refresh_token.as_str()),
            ])
            .send()
            .await
            .context("Could not refresh the cached Microsoft sign-in")?;
        let status = response.status();
        let response = response
            .json::<TokenResponse>()
            .await
            .context("Could not parse the refreshed Microsoft token response")?;
        if !status.is_success() {
            bail!(
                "Microsoft rejected the cached sign-in ({}): {}",
                status,
                response
                    .error_description
                    .as_deref()
                    .or(response.error.as_deref())
                    .unwrap_or("unknown OAuth error")
            );
        }

        match response.error.as_deref() {
            None => {
                if let Some(refresh_token) = response.refresh_token.as_deref() {
                    let refresh_token = refresh_token.to_owned();
                    *self.refresh_token.lock().await = Some(refresh_token.clone());
                    if self.persist_credentials {
                        tokio::task::spawn_blocking(move || {
                            cache_refresh_token(&PlatformSecretStorage, &refresh_token)
                        })
                        .await
                        .context("Could not join the secure-storage token rotation write")??;
                    }
                }
                Ok(response)
            }
            Some(error) => bail!(
                "Microsoft could not restore the cached sign-in: {}",
                response.error_description.as_deref().unwrap_or(error)
            ),
        }
    }

    async fn exchange_skype_token(&self, access_token: &str) -> Result<SkypeAuth> {
        let response = self
            .client
            .post(TEAMS_AUTHZ_URL)
            .bearer_auth(access_token)
            .header("Content-Length", "0")
            .send()
            .await
            .context("Could not exchange the Microsoft token for a Teams token")?
            .error_for_status()
            .context("Teams rejected the token exchange")?
            .json::<TeamsAuthzResponse>()
            .await
            .context("Could not parse the Teams token exchange response")?;
        let region_gtms = response.region_gtms;
        let token = response
            .tokens
            .and_then(|tokens| tokens.skype_token)
            .context("Teams token exchange did not include a Skype token")?;
        Ok(SkypeAuth { token, region_gtms })
    }
}

pub(crate) fn is_unauthorized(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::UNAUTHORIZED
}

fn jwt_string_claim(token: &str, claim: &str) -> Option<String> {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()?
        .get(claim)?
        .as_str()
        .map(ToOwned::to_owned)
}

fn load_stored_accounts(storage: &dyn SecretStorage) -> Result<Vec<StoredAccount>> {
    let Some(serialized) = storage.get(SAVED_ACCOUNTS_KEY)? else {
        return Ok(Vec::new());
    };
    serde_json::from_str(&serialized).context("Could not parse saved Microsoft accounts")
}

fn save_stored_accounts(storage: &dyn SecretStorage, accounts: &[StoredAccount]) -> Result<()> {
    storage.set(
        SAVED_ACCOUNTS_KEY,
        &serde_json::to_string(accounts).context("Could not serialize saved Microsoft accounts")?,
    )
}

fn account_from_access_token(access_token: &str, refresh_token: &str) -> StoredAccount {
    let tenant_id = jwt_string_claim(access_token, "tid").unwrap_or_default();
    let object_id = jwt_string_claim(access_token, "oid")
        .or_else(|| jwt_string_claim(access_token, "sub"))
        .unwrap_or_default();
    let username = jwt_string_claim(access_token, "preferred_username")
        .or_else(|| jwt_string_claim(access_token, "upn"))
        .unwrap_or_default();
    let display_name = jwt_string_claim(access_token, "name")
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| {
            if username.is_empty() {
                "Microsoft account".to_owned()
            } else {
                username.clone()
            }
        });
    let id = if !tenant_id.is_empty() && !object_id.is_empty() {
        format!("{tenant_id}:{object_id}")
    } else if !object_id.is_empty() {
        object_id
    } else if !username.is_empty() {
        username.to_ascii_lowercase()
    } else {
        "microsoft-account".to_owned()
    };
    StoredAccount {
        id,
        display_name,
        username,
        refresh_token: refresh_token.to_owned(),
    }
}

fn cache_account_session(
    storage: &dyn SecretStorage,
    access_token: &str,
    refresh_token: &str,
) -> Result<()> {
    let account = account_from_access_token(access_token, refresh_token);
    let mut accounts = load_stored_accounts(storage)?;
    if let Some(existing) = accounts
        .iter_mut()
        .find(|existing| existing.id == account.id)
    {
        *existing = account.clone();
    } else {
        accounts.push(account.clone());
    }
    save_stored_accounts(storage, &accounts)?;
    storage.set(ACTIVE_ACCOUNT_KEY, &account.id)?;
    storage.set("refresh-token", refresh_token)
}

fn cache_refresh_token(storage: &dyn SecretStorage, refresh_token: &str) -> Result<()> {
    storage.set("refresh-token", refresh_token)?;
    let Some(active_account_id) = storage.get(ACTIVE_ACCOUNT_KEY)? else {
        return Ok(());
    };
    let mut accounts = load_stored_accounts(storage)?;
    if let Some(account) = accounts
        .iter_mut()
        .find(|account| account.id == active_account_id)
    {
        account.refresh_token = refresh_token.to_owned();
        save_stored_accounts(storage, &accounts)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;

    #[test]
    fn graph_token_requests_registered_permissions() {
        assert_eq!(
            GRAPH_SCOPE,
            "https://graph.microsoft.com/.default offline_access"
        );
    }

    #[test]
    fn unauthorized_responses_are_retryable() {
        assert!(is_unauthorized(reqwest::StatusCode::UNAUTHORIZED));
        assert!(!is_unauthorized(reqwest::StatusCode::FORBIDDEN));
    }

    #[test]
    fn device_code_exposes_only_user_safe_fields() {
        let device_code = DeviceCode {
            verification_uri: "https://microsoft.com/devicelogin".to_owned(),
            user_code: "ABC-123".to_owned(),
            expires_in_seconds: 900,
        };
        let json = serde_json::to_string(&device_code).unwrap();
        assert!(!json.contains("device_code"));
        assert!(json.contains("ABC-123"));
    }

    #[test]
    fn credential_cache_keeps_only_the_refresh_token() {
        let storage = TestSecretStorage::default();

        cache_refresh_token(&storage, "refresh-token").unwrap();

        assert_eq!(
            storage.value("refresh-token"),
            Some("refresh-token".to_owned())
        );
        assert_eq!(storage.value("skype-token"), None);
    }

    #[derive(Default)]
    struct TestSecretStorage(StdMutex<HashMap<String, String>>);

    impl TestSecretStorage {
        fn value(&self, key: &str) -> Option<String> {
            self.0.lock().unwrap().get(key).cloned()
        }
    }

    impl SecretStorage for TestSecretStorage {
        fn has(&self, key: &str) -> Result<bool> {
            Ok(self.value(key).is_some())
        }

        fn get(&self, key: &str) -> Result<Option<String>> {
            Ok(self.value(key))
        }

        fn set(&self, key: &str, value: &str) -> Result<()> {
            self.0
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        }

        fn delete(&self, key: &str) -> Result<()> {
            self.0.lock().unwrap().remove(key);
            Ok(())
        }
    }
}
