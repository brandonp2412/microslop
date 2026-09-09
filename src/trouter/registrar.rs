//! Trouter registrar — registers our endpoint with the Teams notification service

use anyhow::{Context, Result};
use ost_microsoft::calling as microsoft_calling;

/// Register our trouter endpoint with the Teams registrar service.
///
/// Performs three separate registrations (TeamsCDLWebWorker, SkypeSpacesWeb,
/// NextGenCalling) as the real Teams client does.
pub async fn register(
    http: &reqwest::Client,
    skype_token: &str,
    registrar_url: &str,
    trouter_surl: &str,
) -> Result<()> {
    let url = registrar_url.trim_end_matches('/').to_string();

    for entry in microsoft_calling::REGISTRATIONS {
        let reg_id = uuid::Uuid::new_v4().to_string();
        let path = format!("{}{}", trouter_surl, entry.path_suffix);

        let payload = microsoft_calling::registrar_payload(*entry, &reg_id, &path);

        tracing::info!(
            "Registering {} at {} (appId={}, templateKey={})",
            if entry.path_suffix.is_empty() {
                "base"
            } else {
                entry.path_suffix
            },
            url,
            entry.app_id,
            entry.template_key,
        );

        let mut attempt = 0u8;
        let resp = loop {
            attempt += 1;
            match http
                .post(&url)
                .header(microsoft_calling::SKYPE_TOKEN_HEADER, skype_token)
                .json(&payload)
                .send()
                .await
            {
                Ok(response) => break response,
                Err(error) if attempt < 3 => {
                    tracing::warn!(
                        "Registrar transport failed for {} on attempt {attempt}: {error}",
                        entry.app_id,
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(250 * attempt as u64))
                        .await;
                }
                Err(error) => return Err(error).context("Registrar POST failed"),
            }
        };

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Registrar {} returned {}: {}", entry.app_id, status, body);
        }
        tracing::info!("Registrar {} registration succeeded", entry.app_id);
    }

    Ok(())
}
