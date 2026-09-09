use super::*;

impl<'a> TeamsService<'a> {
    pub async fn send_message(&self, chat_id: &str, message: &str) -> Result<()> {
        if message.trim().is_empty() {
            bail!("A message cannot be empty");
        }
        let url = format!("{DEFAULT_CHAT_SERVICE}/v1/users/ME/conversations/{chat_id}/messages");
        let token = self.auth.skype_token().await?;
        self.client
            .post(url)
            .header(
                microsoft_teams::NATIVE_AUTH_HEADER,
                microsoft_teams::native_auth_value(&token),
            )
            .json(&serde_json::json!({"content": format!("<p>{}</p>", message_html(message)), "messagetype": "RichText/Html", "contenttype": "text"}))
            .send().await.context("Could not send the Teams message")?.error_for_status()
            .context("Teams rejected the message")?;
        Ok(())
    }

    pub async fn send_audio_message(
        &self,
        chat_id: &str,
        content_type: &str,
        data_base64: &str,
    ) -> Result<()> {
        let data = STANDARD
            .decode(data_base64.trim())
            .context("Audio content is not valid base64")?;
        if data.is_empty() {
            bail!("Audio content is empty");
        }
        let token = self.auth.skype_token().await?;
        let permissions = ams_permissions(chat_id);
        let ams_base = self
            .auth
            .region_gtm("amsV2")
            .await
            .or(self.auth.region_gtm("ams").await)
            .unwrap_or_else(|| DEFAULT_AMS_SERVICE.to_owned())
            .trim_end_matches('/')
            .to_owned();
        let objects_url = format!("{ams_base}/v1/objects");
        let response = self
            .client
            .post(&objects_url)
            .header(
                reqwest::header::AUTHORIZATION,
                microsoft_teams::ams_auth_value(&token),
            )
            .header(reqwest::header::USER_AGENT, AMS_USER_AGENT)
            .json(&serde_json::json!({
                "type": "sharing/audio",
                "permissions": permissions,
                "filename": "voice-message.m4a"
            }))
            .send()
            .await
            .context("Could not create the Teams voice-message media object")?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!(
                "Teams rejected the voice-message media object at {objects_url} ({status}): {body}"
            );
        }
        let object = response
            .json::<serde_json::Value>()
            .await
            .context("Could not parse the Teams voice-message media object")?;
        let object_id = object
            .get("id")
            .and_then(serde_json::Value::as_str)
            .context("Teams voice-message media object has no id")?;
        let object_url = format!("{objects_url}/{object_id}");
        let upload_response = self
            .client
            .put(format!("{object_url}/content/audio"))
            .header(
                reqwest::header::AUTHORIZATION,
                microsoft_teams::ams_auth_value(&token),
            )
            .header(reqwest::header::USER_AGENT, AMS_USER_AGENT)
            .header(
                reqwest::header::CONTENT_TYPE,
                if content_type.trim().is_empty() {
                    "audio/mp4"
                } else {
                    content_type
                },
            )
            .body(data)
            .send()
            .await
            .context("Could not upload the Teams voice message")?;
        let upload_status = upload_response.status();
        if !upload_status.is_success() {
            let body = upload_response.text().await.unwrap_or_default();
            bail!("Teams rejected the voice-message upload ({upload_status}): {body}");
        }
        let content = audio_message_content(&objects_url, object_id);
        self.client
            .post(format!(
                "{DEFAULT_CHAT_SERVICE}/v1/users/ME/conversations/{chat_id}/messages"
            ))
            .header(
                microsoft_teams::NATIVE_AUTH_HEADER,
                microsoft_teams::native_auth_value(&token),
            )
            .json(&serde_json::json!({
                "content": content,
                "messagetype": "RichText/Media_AudioMsg",
                "contenttype": "text",
                "amsreferences": [object_id]
            }))
            .send()
            .await
            .context("Could not send the Teams voice message")?
            .error_for_status()
            .context("Teams rejected the voice message")?;
        Ok(())
    }

    pub async fn send_channel_message(
        &self,
        team_id: &str,
        channel_id: &str,
        message: &str,
    ) -> Result<()> {
        if message.trim().is_empty() {
            bail!("A message cannot be empty");
        }
        let token = self.auth.graph_token().await?;
        let path = format!("/teams/{team_id}/channels/{channel_id}/messages");
        self.client.post(format!("{GRAPH_BASE}{path}")).bearer_auth(token)
            .json(&serde_json::json!({"body": {"contentType": "html", "content": message_html(message)}}))
            .send().await.context("Could not send the channel message")?.error_for_status()
            .context("Microsoft Graph rejected the channel message")?;
        Ok(())
    }

    pub async fn send_image_message(
        &self,
        chat_id: Option<&str>,
        team_id: Option<&str>,
        channel_id: Option<&str>,
        caption: &str,
        content_type: &str,
        data_base64: &str,
    ) -> Result<()> {
        if !content_type.starts_with("image/") || data_base64.trim().is_empty() {
            bail!("Image content is missing or invalid");
        }
        let path = match (chat_id, team_id, channel_id) {
            (Some(chat_id), None, None) => format!("/chats/{chat_id}/messages"),
            (None, Some(team_id), Some(channel_id)) => {
                format!("/teams/{team_id}/channels/{channel_id}/messages")
            }
            _ => bail!("Invalid conversation for image upload"),
        };
        let caption_html = if caption.trim().is_empty() {
            String::new()
        } else {
            format!("<p>{}</p>", message_html(caption))
        };
        let body = format!("{caption_html}<div><img src=\"../hostedContents/1/$value\" /></div>");
        let token = self.auth.graph_token().await?;
        self.client
            .post(format!("{GRAPH_BASE}{path}"))
            .bearer_auth(token)
            .json(&serde_json::json!({
                "body": {"contentType": "html", "content": body},
                "hostedContents": [{
                    "@microsoft.graph.temporaryId": "1",
                    "contentBytes": data_base64,
                    "contentType": content_type,
                }]
            }))
            .send()
            .await
            .context("Could not upload the Teams image")?
            .error_for_status()
            .context("Microsoft Graph rejected the Teams image")?;
        Ok(())
    }

    pub async fn send_file_message(
        &self,
        chat_id: Option<&str>,
        team_id: Option<&str>,
        channel_id: Option<&str>,
        file_name: &str,
        content_type: &str,
        data_base64: &str,
    ) -> Result<()> {
        let file_name = file_name
            .trim()
            .rsplit(['/', '\\'])
            .next()
            .filter(|name| !name.is_empty())
            .context("File name is missing")?;
        let data = STANDARD
            .decode(data_base64.trim())
            .context("File content is not valid base64")?;
        if data.is_empty() {
            bail!("File content is empty");
        }

        let token = self.auth.graph_token().await?;
        let stored_name = format!("{}-{file_name}", uuid::Uuid::new_v4());
        let encoded_name =
            url::form_urlencoded::byte_serialize(stored_name.as_bytes()).collect::<String>();
        let upload = self
            .client
            .put(format!(
                "{GRAPH_BASE}/me/drive/root:/Microsoft%20Teams%20Chat%20Files/{encoded_name}:/content"
            ))
            .bearer_auth(&token)
            .header(
                reqwest::header::CONTENT_TYPE,
                if content_type.trim().is_empty() {
                    "application/octet-stream"
                } else {
                    content_type
                },
            )
            .body(data)
            .send()
            .await
            .context("Could not upload the Teams file")?;
        let upload = if upload.status().is_success() {
            upload
        } else {
            let data = STANDARD
                .decode(data_base64.trim())
                .context("File content is not valid base64")?;
            self.client
                .put(format!(
                    "{GRAPH_BASE}/me/drive/root:/{encoded_name}:/content"
                ))
                .bearer_auth(&token)
                .header(
                    reqwest::header::CONTENT_TYPE,
                    if content_type.trim().is_empty() {
                        "application/octet-stream"
                    } else {
                        content_type
                    },
                )
                .body(data)
                .send()
                .await
                .context("Could not upload the Teams file")?
        };
        let item = upload
            .error_for_status()
            .context("Microsoft Graph rejected the Teams file upload")?
            .json::<serde_json::Value>()
            .await
            .context("Could not parse the uploaded Teams file")?;
        let item_id = item
            .get("id")
            .and_then(serde_json::Value::as_str)
            .context("Uploaded Teams file has no drive item ID")?;
        let link = self
            .client
            .post(format!("{GRAPH_BASE}/me/drive/items/{item_id}/createLink"))
            .bearer_auth(&token)
            .json(&serde_json::json!({"type": "view", "scope": "organization"}))
            .send()
            .await
            .context("Could not create a Teams file share link")?
            .error_for_status()
            .context("Microsoft Graph rejected the Teams file share link")?
            .json::<serde_json::Value>()
            .await
            .context("Could not parse the Teams file share link")?;
        let content_url = link
            .pointer("/link/webUrl")
            .and_then(serde_json::Value::as_str)
            .context("Teams file share link has no URL")?;
        let path = match (chat_id, team_id, channel_id) {
            (Some(chat_id), None, None) => format!("/chats/{chat_id}/messages"),
            (None, Some(team_id), Some(channel_id)) => {
                format!("/teams/{team_id}/channels/{channel_id}/messages")
            }
            _ => bail!("Invalid conversation for file upload"),
        };
        let attachment_id = uuid::Uuid::new_v4().to_string();
        self.client
            .post(format!("{GRAPH_BASE}{path}"))
            .bearer_auth(token)
            .json(&serde_json::json!({
                "body": {
                    "contentType": "html",
                    "content": format!("<attachment id=\"{attachment_id}\"></attachment>")
                },
                "attachments": [{
                    "id": attachment_id,
                    "contentType": "reference",
                    "contentUrl": content_url,
                    "name": file_name
                }]
            }))
            .send()
            .await
            .context("Could not send the Teams file attachment")?
            .error_for_status()
            .context("Microsoft Graph rejected the Teams file attachment")?;
        Ok(())
    }

    pub async fn set_reaction(
        &self,
        chat_id: Option<&str>,
        team_id: Option<&str>,
        channel_id: Option<&str>,
        message_id: &str,
        reaction_type: &str,
        remove: bool,
    ) -> Result<()> {
        if message_id.is_empty() {
            bail!("This message cannot be reacted to");
        }
        match (chat_id, team_id, channel_id) {
            (Some(chat_id), None, None) => {
                let mut url = url::Url::parse(DEFAULT_CHAT_SERVICE)?;
                url.path_segments_mut()
                    .map_err(|_| anyhow::anyhow!("Invalid Teams chat service URL"))?
                    .extend([
                        "v1",
                        "users",
                        "ME",
                        "conversations",
                        chat_id,
                        "messages",
                        message_id,
                        "properties",
                    ]);
                url.query_pairs_mut().append_pair("name", "emotions");
                let body = if remove {
                    serde_json::json!({"emotions": {"key": reaction_type}})
                } else {
                    serde_json::json!({"emotions": {"key": reaction_type, "value": message_id}})
                };
                let token = self.auth.skype_token().await?;
                let request = if remove {
                    self.client.delete(url)
                } else {
                    self.client.put(url)
                };
                request
                    .header(
                        microsoft_teams::NATIVE_AUTH_HEADER,
                        microsoft_teams::native_auth_value(&token),
                    )
                    .json(&body)
                    .send()
                    .await
                    .context("Could not update the Teams chat reaction")?
                    .error_for_status()
                    .context("Teams chat service rejected the reaction update")?;
            }
            (None, Some(team_id), Some(channel_id)) => {
                let token = self.auth.graph_token().await?;
                let action = if remove {
                    "unsetReaction"
                } else {
                    "setReaction"
                };
                let path = format!(
                    "/teams/{team_id}/channels/{channel_id}/messages/{message_id}/{action}"
                );
                self.client
                    .post(format!("{GRAPH_BASE}{path}"))
                    .bearer_auth(token)
                    .json(&serde_json::json!({"reactionType": reaction_type}))
                    .send()
                    .await
                    .context("Could not update the Teams channel reaction")?
                    .error_for_status()
                    .context("Microsoft Graph rejected the channel reaction update")?;
            }
            _ => bail!("Invalid conversation for reaction"),
        }
        Ok(())
    }
}
