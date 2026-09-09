use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToken {
    pub token: String,
    pub expires_at: Option<u64>,
}

impl StoredToken {
    pub fn new(token: String, expires_in_secs: Option<u64>) -> Self {
        let expires_at = expires_in_secs.map(|secs| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + secs
        });

        Self { token, expires_at }
    }

    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(exp) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                now + 300 >= exp
            }
            None => false,
        }
    }
}

pub trait TokenStore {
    fn get_access_token(&self) -> Option<StoredToken>;
    fn set_access_token(&mut self, token: String, expires_in: Option<u64>);
    fn get_refresh_token(&self) -> Option<String>;
    fn set_refresh_token(&mut self, token: String);
    fn clear_tokens(&mut self);
}
