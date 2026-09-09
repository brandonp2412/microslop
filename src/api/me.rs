//! User profile endpoint (/me)

use anyhow::{Context, Result};
use ost_microsoft::teams::models::GraphMe as MeResponse;

use super::client::TeamsClient;

/// Fetch and display current user info from Graph /me endpoint (prints to stdout).
pub async fn whoami() -> Result<()> {
    let client = TeamsClient::new().await?;
    let info = whoami_data(&client).await?;

    println!();
    println!("Display Name: {}", info.display_name);
    println!("Mail:         {}", info.mail.as_deref().unwrap_or("(none)"));
    println!("ID:           {}", info.id);

    Ok(())
}

/// User info for TUI display.
pub struct UserInfo {
    pub display_name: String,
    pub mail: Option<String>,
    pub id: String,
}

pub async fn whoami_data(client: &TeamsClient) -> Result<UserInfo> {
    let resp = client.graph_get("/me").await?;
    let me: MeResponse = resp.json().await.context("Failed to parse /me response")?;

    Ok(UserInfo {
        display_name: me.display_name.unwrap_or_else(|| "User".to_string()),
        mail: me.mail,
        id: me.id,
    })
}
