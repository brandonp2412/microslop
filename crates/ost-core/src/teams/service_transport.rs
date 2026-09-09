use super::*;

impl<'a> TeamsService<'a> {
    pub(super) async fn graph_get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let token = self.auth.graph_token().await?;
        let response = self
            .client
            .get(format!("{GRAPH_BASE}{path}"))
            .bearer_auth(token)
            .send()
            .await
            .context("Could not reach Microsoft Graph")?;
        let response = if is_unauthorized(response.status()) {
            self.auth.refresh_graph_token().await?;
            self.client
                .get(format!("{GRAPH_BASE}{path}"))
                .bearer_auth(self.auth.graph_token().await?)
                .send()
                .await
                .context("Could not reach Microsoft Graph")?
        } else {
            response
        };
        response
            .error_for_status()
            .context("Microsoft Graph rejected the request")?
            .json()
            .await
            .context("Could not parse the Microsoft Graph response")
    }

    pub(super) async fn graph_post<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
        B: Serialize + ?Sized,
    {
        let response = self
            .client
            .post(format!("{GRAPH_BASE}{path}"))
            .bearer_auth(self.auth.graph_token().await?)
            .json(body)
            .send()
            .await
            .context("Could not reach Microsoft Graph")?;
        let response = if is_unauthorized(response.status()) {
            self.auth.refresh_graph_token().await?;
            self.client
                .post(format!("{GRAPH_BASE}{path}"))
                .bearer_auth(self.auth.graph_token().await?)
                .json(body)
                .send()
                .await
                .context("Could not reach Microsoft Graph")?
        } else {
            response
        };
        response
            .error_for_status()
            .context("Microsoft Graph rejected the request")?
            .json()
            .await
            .context("Could not parse the Microsoft Graph response")
    }

    pub(super) async fn current_user(&self) -> Result<GraphUser> {
        self.graph_get("/me").await
    }

    pub async fn user_display_name(&self, user_id: &str) -> Result<String> {
        let user: GraphUser = self.graph_get(&format!("/users/{user_id}")).await?;
        user.display_name
            .filter(|name| is_displayable_chat_name(name))
            .context("Microsoft Graph returned a user without a display name")
    }

    pub(super) async fn list_graph_chats(
        &self,
        current_user: Option<&GraphUser>,
        limit: usize,
    ) -> Result<Vec<Chat>> {
        let mut chats = Vec::new();
        let mut next_url = Some(format!(
            "{GRAPH_BASE}/me/chats?$top=50&$expand=members,lastMessagePreview&$orderby=lastMessagePreview/createdDateTime desc"
        ));

        while chats.len() < limit {
            let Some(url) = next_url else {
                break;
            };
            let page: GraphCollection<GraphChat> = self.graph_get_url(&url).await?;
            chat_diag!(format!(
                "graph_page items={} has_next={}",
                page.value.len(),
                page.next_link.is_some()
            ));
            next_url = page.next_link;
            chats.extend(page.value.into_iter().filter_map(|chat| {
                #[cfg(debug_assertions)]
                let raw_member_ids = chat
                    .members
                    .as_ref()
                    .map(|members| {
                        members
                            .iter()
                            .map(|member| member.user_id.as_deref().unwrap_or("<missing>"))
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .unwrap_or_else(|| "<none>".to_owned());
                chat_diag!(format!(
                    "graph_raw id={} type={} topic={} hidden={} members={}",
                    chat.id,
                    chat.chat_type.as_deref().unwrap_or("<missing>"),
                    chat.topic.as_deref().unwrap_or("<missing>"),
                    chat.viewpoint
                        .as_ref()
                        .and_then(|viewpoint| viewpoint.is_hidden)
                        .unwrap_or(false),
                    raw_member_ids
                ));
                if chat
                    .viewpoint
                    .as_ref()
                    .and_then(|viewpoint| viewpoint.is_hidden)
                    .unwrap_or(false)
                {
                    return None;
                }
                let all_members = chat.members.unwrap_or_default();
                let current_user_id = current_user.and_then(|user| user.id.as_deref());
                let is_self_chat =
                    is_graph_self_chat(chat.chat_type.as_deref(), &all_members, current_user_id);
                chat_diag!(format!(
                    "graph_classify id={} self={} group={}",
                    chat.id,
                    is_self_chat,
                    chat.chat_type.as_deref() != Some("oneOnOne")
                ));
                let members: Vec<_> = all_members
                    .into_iter()
                    .filter(|member| member.user_id.as_deref() != current_user_id)
                    .collect();
                let is_group = chat.chat_type.as_deref() != Some("oneOnOne");
                let name = if is_self_chat {
                    current_user
                        .and_then(|user| user.display_name.clone())
                        .unwrap_or_else(|| "Self chat".to_owned())
                } else {
                    chat.topic
                        .filter(|topic| is_displayable_chat_name(topic))
                        .or_else(|| {
                            let names = members
                                .iter()
                                .filter_map(|member| member.display_name.as_deref())
                                .filter(|name| is_displayable_chat_name(name))
                                .collect::<Vec<_>>();
                            (!names.is_empty()).then(|| names.join(", "))
                        })
                        .unwrap_or_else(|| {
                            if is_group {
                                "Group chat".to_owned()
                            } else {
                                "Chat".to_owned()
                            }
                        })
                };
                let profile_photo_user_id = if is_self_chat {
                    current_user_id.map(ToOwned::to_owned)
                } else {
                    (members.len() == 1)
                        .then(|| members[0].user_id.clone())
                        .flatten()
                };
                let member_user_ids = if is_self_chat {
                    current_user_id.into_iter().map(ToOwned::to_owned).collect()
                } else {
                    members
                        .iter()
                        .filter_map(|member| member.user_id.clone())
                        .collect()
                };
                let last_message = chat.last_message_preview;
                Some(Chat {
                    is_group,
                    last_message_id: last_message.as_ref().and_then(|message| message.id.clone()),
                    last_message_preview: last_message
                        .and_then(|message| message.body)
                        .and_then(|body| body.content)
                        .map(|content| message_content_and_quotes(&content).0),
                    id: chat.id,
                    name,
                    profile_photo_user_id,
                    member_user_ids,
                    team_id: None,
                })
            }));
        }
        chats.truncate(limit);
        Ok(chats)
    }

    pub(super) async fn read_graph_messages(
        &self,
        conversation: GraphConversation<'_>,
        limit: usize,
        current_user: Option<&GraphUser>,
        include_images: bool,
    ) -> Result<Vec<ChatMessage>> {
        let page_size = limit.clamp(1, 50);
        let mut next_url = Some(format!(
            "{GRAPH_BASE}{}?$top={page_size}",
            conversation.messages_path()
        ));
        let mut raw_messages = Vec::new();

        while raw_messages.len() < limit {
            let Some(url) = next_url else {
                break;
            };
            let page: GraphCollection<GraphMessage> =
                tokio::time::timeout(std::time::Duration::from_secs(12), self.graph_get_url(&url))
                    .await
                    .context("Microsoft Graph message page timed out")??;
            next_url = page.next_link;
            raw_messages.extend(page.value);
        }
        raw_messages.truncate(limit);

        let mut messages =
            futures::stream::iter(raw_messages.into_iter().map(|message| async move {
                let message_id = message.id.clone().unwrap_or_default();
                let html = message
                    .body
                    .as_ref()
                    .and_then(|body| body.content.as_deref())
                    .unwrap_or_default();
                let mut image_markup = std::borrow::Cow::Borrowed(html);
                for attachment in message.attachments.iter().flatten() {
                    if let Some(content) = attachment.content.as_deref() {
                        image_markup.to_mut().push_str(content);
                    }
                    if attachment
                        .content_type
                        .as_deref()
                        .is_some_and(|content_type| content_type.starts_with("image/"))
                        && let Some(url) = attachment.content_url.as_deref()
                    {
                        let markup = image_markup.to_mut();
                        markup.push_str("<img src=\"");
                        markup.push_str(url);
                        markup.push_str("\">");
                    }
                    if let Some(url) = attachment.thumbnail_url.as_deref() {
                        let markup = image_markup.to_mut();
                        markup.push_str("<img src=\"");
                        markup.push_str(url);
                        markup.push_str("\">");
                    }
                }
                let images = if include_images {
                    tokio::time::timeout(std::time::Duration::from_secs(2), async {
                        if message_id.is_empty() {
                            self.remote_message_images(&image_markup).await
                        } else {
                            self.message_images(conversation, &message_id, &image_markup)
                                .await
                                .unwrap_or_else(|error| {
                                    tracing::debug!("Could not load message images: {error:#}");
                                    Vec::new()
                                })
                        }
                    })
                    .await
                    .unwrap_or_else(|_| {
                        tracing::debug!("Timed out loading message images for {message_id}");
                        Vec::new()
                    })
                } else {
                    Vec::new()
                };
                let mut urls: Vec<_> = hosted_content_ids(&image_markup)
                    .into_iter()
                    .map(|id| {
                        microsoft_teams::hosted_image_url(
                            &conversation.messages_path(),
                            &message_id,
                            &id,
                        )
                    })
                    .collect();
                urls.extend(image_src_urls(&image_markup));
                graph_message(message, current_user, images).map(|mut message| {
                    message.image_urls = urls;
                    message
                })
            }))
            .buffer_unordered(32)
            .filter_map(|message| async move { message })
            .collect::<Vec<_>>()
            .await;
        messages.sort_by(|left, right| left.timestamp.cmp(&right.timestamp));
        Ok(messages)
    }

    pub(super) async fn message_images(
        &self,
        conversation: GraphConversation<'_>,
        message_id: &str,
        html: &str,
    ) -> Result<Vec<MessageImage>> {
        let hosted_content_ids = hosted_content_ids(html);
        if hosted_content_ids.is_empty() {
            return Ok(self.remote_message_images(html).await);
        }
        let token = self.auth.graph_token().await?;
        let mut images = futures::stream::iter(hosted_content_ids.into_iter().map(|content_id| {
            let token = &token;
            async move {
                let url = format!(
                    "{GRAPH_BASE}{}/{message_id}/hostedContents/{content_id}/$value",
                    conversation.messages_path()
                );
                let response = self
                    .client
                    .get(&url)
                    .bearer_auth(token)
                    .timeout(std::time::Duration::from_secs(5))
                    .send()
                    .await
                    .context("Could not retrieve Teams hosted message content")?
                    .error_for_status()
                    .context("Microsoft Graph rejected hosted message content")?;
                let content_type = response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("image/png")
                    .to_owned();
                if !content_type.starts_with("image/") {
                    return Ok(None);
                }
                let bytes = response
                    .bytes()
                    .await
                    .context("Could not read Teams hosted message content")?;
                Ok::<_, anyhow::Error>(Some(MessageImage {
                    source_url: Some(url),
                    content_type,
                    data_base64: STANDARD.encode(bytes),
                }))
            }
        }))
        .buffered(4)
        .filter_map(|result| async move {
            match result {
                Ok(image) => image,
                Err(error) => {
                    tracing::debug!("Could not load Teams hosted message image: {error:#}");
                    None
                }
            }
        })
        .collect::<Vec<_>>()
        .await;
        images.extend(self.remote_message_images(html).await);
        Ok(images)
    }

    pub(super) async fn remote_message_images(&self, html: &str) -> Vec<MessageImage> {
        futures::stream::iter(image_src_urls(html).into_iter().map(|url| async move {
            match self.download_message_image(&url).await {
                Ok(image) => image,
                Err(error) => {
                    tracing::debug!("Could not load Teams message image: {error:#}");
                    None
                }
            }
        }))
        .buffered(4)
        .filter_map(|image| async move { image })
        .collect()
        .await
    }

    pub async fn download_message_image(&self, url: &str) -> Result<Option<MessageImage>> {
        let parsed = reqwest::Url::parse(url).context("Invalid Teams message image URL")?;
        let host = parsed.host_str().unwrap_or_default();
        let Some(image_auth) = microsoft_teams::message_image_auth(host) else {
            return Ok(None);
        };
        let graph_image = image_auth == microsoft_teams::MessageImageAuth::Graph;
        let teams_image = image_auth == microsoft_teams::MessageImageAuth::Skype;

        let mut request = self
            .client
            .get(parsed.clone())
            .timeout(std::time::Duration::from_secs(5));
        if graph_image {
            request = request.bearer_auth(self.auth.graph_token().await?);
        } else if teams_image {
            let token = self.auth.skype_token().await?;
            request = request
                .header(
                    reqwest::header::COOKIE,
                    microsoft_teams::asm_cookie_value(&token),
                )
                .header("Accept", "image/*");
        } else {
            request = request.header("Accept", "image/*");
        }
        let mut response = request
            .send()
            .await
            .context("Could not retrieve Teams message image")?;
        if teams_image && response.status() == reqwest::StatusCode::UNAUTHORIZED {
            let token = self.auth.skype_token().await?;
            response = self
                .client
                .get(parsed)
                .timeout(std::time::Duration::from_secs(5))
                .header(
                    reqwest::header::AUTHORIZATION,
                    microsoft_teams::native_auth_value(&token),
                )
                .header(
                    microsoft_teams::NATIVE_AUTH_HEADER,
                    microsoft_teams::native_auth_value(&token),
                )
                .header("Accept", "image/*")
                .send()
                .await
                .context("Could not retry Teams message image")?;
        }
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let response = response
            .error_for_status()
            .context("Teams rejected the message image request")?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("image/jpeg")
            .split(';')
            .next()
            .unwrap_or("image/jpeg")
            .to_owned();
        if !content_type.starts_with("image/") {
            return Ok(None);
        }
        let bytes = response
            .bytes()
            .await
            .context("Could not read Teams message image")?;
        Ok(Some(MessageImage {
            source_url: Some(url.to_owned()),
            content_type,
            data_base64: STANDARD.encode(bytes),
        }))
    }

    pub(super) async fn chat_members(
        &self,
        current_user_id: &str,
    ) -> Result<HashMap<String, ChatMembers>> {
        let mut members_by_chat = HashMap::new();
        let mut next_url = Some(format!("{GRAPH_BASE}/me/chats?$top=50&$expand=members"));

        while let Some(url) = next_url {
            let chats: GraphCollection<GraphChat> = self.graph_get_url(&url).await?;
            next_url = chats.next_link;
            members_by_chat.extend(chats.value.into_iter().map(|chat| {
                let members: Vec<_> = chat
                    .members
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|member| member.user_id.as_deref() != Some(current_user_id))
                    .collect();
                let names = members
                    .iter()
                    .filter_map(|member| member.display_name.as_ref())
                    .filter(|name| is_displayable_chat_name(name))
                    .cloned()
                    .collect();
                let photo_user_id = (members.len() == 1)
                    .then(|| members[0].user_id.clone())
                    .flatten();
                let user_ids = members
                    .iter()
                    .filter_map(|member| member.user_id.clone())
                    .collect();
                (
                    chat.id,
                    ChatMembers {
                        names,
                        photo_user_id,
                        user_ids,
                    },
                )
            }));
        }

        Ok(members_by_chat)
    }

    pub(super) async fn chat_get<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T> {
        let token = self.auth.skype_token().await?;
        let response = self
            .client
            .get(url)
            .header(
                microsoft_teams::NATIVE_AUTH_HEADER,
                microsoft_teams::native_auth_value(&token),
            )
            .send()
            .await
            .context("Could not reach the Teams chat service")?;
        let response = if is_unauthorized(response.status()) {
            self.auth.refresh_skype_token().await?;
            self.client
                .get(url)
                .header(
                    microsoft_teams::NATIVE_AUTH_HEADER,
                    microsoft_teams::native_auth_value(&self.auth.skype_token().await?),
                )
                .send()
                .await
                .context("Could not reach the Teams chat service")?
        } else {
            response
        };
        response
            .error_for_status()
            .context("Teams rejected the request")?
            .json()
            .await
            .context("Could not parse the Teams response")
    }

    pub(super) async fn graph_get_url<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T> {
        let token = self.auth.graph_token().await?;
        let response = self
            .client
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .context("Could not reach Microsoft Graph")?;
        let response = if is_unauthorized(response.status()) {
            self.auth.refresh_graph_token().await?;
            self.client
                .get(url)
                .bearer_auth(self.auth.graph_token().await?)
                .send()
                .await
                .context("Could not reach Microsoft Graph")?
        } else {
            response
        };
        response
            .error_for_status()
            .context("Microsoft Graph rejected the request")?
            .json()
            .await
            .context("Could not parse the Microsoft Graph response")
    }
}
