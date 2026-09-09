//! Authentication module for Microsoft Teams
//!
//! Implements OAuth2 device code flow for Azure AD authentication,
//! then exchanges the AAD token for a Skype token.

pub mod oauth;
pub mod skype;
pub mod tokens;

pub use oauth::{login, logout, status};
pub use tokens::{StoredToken, TokenStore};

use ost_microsoft::auth;

pub struct AuthConfig {
    pub client_id: &'static str,
    pub tenant: &'static str,
    pub scope: &'static str,
}

impl AuthConfig {
    pub fn work() -> Self {
        Self {
            client_id: auth::WORK_CLIENT_ID,
            tenant: "common",
            scope: auth::TEAMS_SCOPE,
        }
    }

    pub fn personal() -> Self {
        Self {
            client_id: auth::PERSONAL_CLIENT_ID,
            tenant: "consumers",
            scope: auth::PERSONAL_TEAMS_SCOPE,
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self::work()
    }
}
