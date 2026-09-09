use std::sync::Arc;

use once_cell::sync::Lazy;
use ost_core::auth::{
    AuthStatus as CoreAuthStatus, DeviceCode as CoreDeviceCode, SavedAccount as CoreSavedAccount,
};

#[derive(Clone)]
pub struct AuthStatus {
    pub signed_in: bool,
    pub login_in_progress: bool,
}

impl From<CoreAuthStatus> for AuthStatus {
    fn from(status: CoreAuthStatus) -> Self {
        Self {
            signed_in: status.signed_in,
            login_in_progress: status.login_in_progress,
        }
    }
}

#[derive(Clone)]
pub struct SavedAccount {
    pub id: String,
    pub display_name: String,
    pub username: String,
}

impl From<CoreSavedAccount> for SavedAccount {
    fn from(account: CoreSavedAccount) -> Self {
        Self {
            id: account.id,
            display_name: account.display_name,
            username: account.username,
        }
    }
}

#[derive(Clone)]
pub struct DeviceCode {
    pub verification_uri: String,
    pub user_code: String,
    pub expires_in_seconds: u64,
}

impl From<CoreDeviceCode> for DeviceCode {
    fn from(device_code: CoreDeviceCode) -> Self {
        Self {
            verification_uri: device_code.verification_uri,
            user_code: device_code.user_code,
            expires_in_seconds: device_code.expires_in_seconds,
        }
    }
}

static AUTH_SERVICE: Lazy<Arc<ost_core::auth::AuthService>> =
    Lazy::new(|| Arc::new(ost_core::auth::AuthService::new()));

pub(crate) fn service() -> &'static Arc<ost_core::auth::AuthService> {
    &AUTH_SERVICE
}

pub async fn get_auth_status() -> anyhow::Result<AuthStatus> {
    AUTH_SERVICE.status().await.map(Into::into)
}

pub async fn restore_work_session() -> anyhow::Result<AuthStatus> {
    AUTH_SERVICE.restore_work_session().await.map(Into::into)
}

pub async fn list_saved_accounts() -> anyhow::Result<Vec<SavedAccount>> {
    Ok(AUTH_SERVICE
        .saved_accounts()
        .await?
        .into_iter()
        .map(Into::into)
        .collect())
}

pub async fn active_account_id() -> anyhow::Result<Option<String>> {
    AUTH_SERVICE.active_account_id().await
}

pub async fn switch_saved_account(account_id: String) -> anyhow::Result<AuthStatus> {
    AUTH_SERVICE
        .switch_account(&account_id)
        .await
        .map(Into::into)
}

pub async fn begin_work_login() -> anyhow::Result<DeviceCode> {
    AUTH_SERVICE.begin_work_login().await.map(Into::into)
}

pub async fn complete_work_login() -> anyhow::Result<AuthStatus> {
    AUTH_SERVICE.complete_work_login().await.map(Into::into)
}

pub async fn cancel_work_login() {
    AUTH_SERVICE.cancel_work_login().await;
}

pub async fn logout() -> anyhow::Result<AuthStatus> {
    AUTH_SERVICE.logout().await.map(Into::into)
}
