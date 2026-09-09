use super::*;

impl<'a> TeamsService<'a> {
    pub async fn read_messages(
        &self,
        chat_id: &str,
        limit: usize,
        include_images: bool,
    ) -> Result<Vec<ChatMessage>> {
        let current_user = self.current_user().await.ok();
        let fresh_messages = if GRAPH_CHATS_FORBIDDEN.load(Ordering::Relaxed) {
            self.read_native_messages(chat_id, limit, current_user.as_ref(), include_images)
                .await
        } else {
            match self
                .read_graph_messages(
                    GraphConversation::Chat(chat_id),
                    limit,
                    current_user.as_ref(),
                    include_images,
                )
                .await
            {
                Ok(messages) => Ok(messages),
                Err(graph_error) => {
                    if is_forbidden(&graph_error) {
                        GRAPH_CHATS_FORBIDDEN.store(true, Ordering::Relaxed);
                    }
                    tracing::debug!(
                        "Falling back to native chat messages for {chat_id}: {graph_error:#}"
                    );
                    self.read_native_messages(chat_id, limit, current_user.as_ref(), include_images)
                        .await
                }
            }
        };

        match fresh_messages {
            Ok(messages) => {
                if include_images {
                    persist_messages(chat_id, &messages);
                }
                Ok(messages)
            }
            Err(network_error) => {
                if let Some(cached) = load_cached_messages(chat_id).await? {
                    tracing::debug!(
                        "Using cached chat messages for {chat_id} after refresh failed: {network_error:#}"
                    );
                    let start = cached.len().saturating_sub(limit);
                    return Ok(cached.into_iter().skip(start).collect());
                }
                Err(network_error)
            }
        }
    }

    pub(super) async fn read_native_messages(
        &self,
        chat_id: &str,
        limit: usize,
        current_user: Option<&GraphUser>,
        include_images: bool,
    ) -> Result<Vec<ChatMessage>> {
        let page_size = limit.clamp(1, 100);
        let mut next_url = Some(format!(
            "{DEFAULT_CHAT_SERVICE}/v1/users/ME/conversations/{chat_id}/messages?pageSize={page_size}"
        ));
        let mut visited_urls = HashSet::new();
        let mut raw_messages = Vec::new();
        while raw_messages.len() < limit {
            let Some(url) = next_url.take() else {
                break;
            };
            if !visited_urls.insert(url.clone()) {
                break;
            }
            let page: NativeMessagesResponse = self.chat_get(&url).await?;
            next_url = page
                .metadata
                .and_then(|metadata| metadata.backward_link)
                .filter(|link| !link.is_empty());
            raw_messages.extend(page.messages);
        }
        raw_messages.truncate(limit);
        let current_user_id = current_user.and_then(|user| user.id.as_deref());
        let parsed = futures::stream::iter(raw_messages.into_iter().map(|native| async move {
            let images = if include_images {
                tokio::time::timeout(
                    std::time::Duration::from_secs(15),
                    self.remote_message_images(native.content.as_deref().unwrap_or_default()),
                )
                .await
                .unwrap_or_default()
            } else {
                Vec::new()
            };
            native_message(native, images, current_user_id)
        }))
        .buffer_unordered(24)
        .collect::<Vec<_>>()
        .await;
        let current_user_name = current_user.and_then(|user| user.display_name.as_deref());
        let fallback_chat_name = cached_chat_name(chat_id);
        let self_chat = is_self_chat_id(chat_id, current_user.and_then(|user| user.id.as_deref()));
        let parsed = parsed
            .into_iter()
            .flatten()
            .map(|mut message| {
                message.is_from_current_user =
                    self_chat || is_current_user_message(&message, current_user);
                if message.sender.trim().is_empty() || message.sender == "Unknown" {
                    if message.is_from_current_user {
                        if let Some(name) =
                            current_user_name.filter(|name| is_displayable_chat_name(name))
                        {
                            message.sender = name.to_owned();
                        }
                    } else if let Some(name) = fallback_chat_name.as_ref() {
                        message.sender = name.clone();
                    }
                }
                message
            })
            .collect();
        Ok(order_chat_messages_chronologically(parsed))
    }

    pub async fn read_channel_messages(
        &self,
        team_id: &str,
        channel_id: &str,
        limit: usize,
        include_images: bool,
    ) -> Result<Vec<ChatMessage>> {
        let current_user = self.current_user().await.ok();
        let native = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            self.read_native_messages(channel_id, limit, current_user.as_ref(), include_images),
        )
        .await;
        if let Ok(Ok(mut messages)) = native {
            self.resolve_unknown_channel_senders(
                team_id,
                channel_id,
                &mut messages,
                current_user.as_ref(),
            )
            .await;
            if include_images {
                persist_messages(channel_id, &messages);
            }
            return Ok(messages);
        }

        let graph = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            self.read_graph_messages(
                GraphConversation::Channel {
                    team_id,
                    channel_id,
                },
                limit,
                current_user.as_ref(),
                include_images,
            ),
        )
        .await;
        if let Ok(Ok(messages)) = graph {
            if include_images {
                persist_messages(channel_id, &messages);
            }
            return Ok(messages);
        }

        if let Some(cached) = load_cached_messages(channel_id).await? {
            tracing::debug!(
                "Using cached channel messages for {channel_id} after both live readers failed"
            );
            let start = cached.len().saturating_sub(limit);
            return Ok(cached.into_iter().skip(start).collect());
        }

        match (native, graph) {
            (Ok(Err(native_error)), _) => Err(native_error),
            (_, Ok(Err(graph_error))) => Err(graph_error),
            _ => bail!("Teams channel message refresh timed out"),
        }
    }

    pub(super) async fn resolve_unknown_channel_senders(
        &self,
        team_id: &str,
        channel_id: &str,
        messages: &mut [ChatMessage],
        current_user: Option<&GraphUser>,
    ) {
        let unresolved = messages
            .iter()
            .enumerate()
            .filter(|(_, message)| {
                (message.sender.trim().is_empty() || message.sender == "Unknown")
                    && !message.id.is_empty()
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if unresolved.is_empty() {
            return;
        }
        let known_names = messages
            .iter()
            .filter_map(|message| {
                let sender_id = message.sender_id.as_deref()?;
                let sender = message.sender.trim();
                (!sender.is_empty() && sender != "Unknown").then(|| {
                    (
                        sender_id.trim_matches(['{', '}']).to_ascii_lowercase(),
                        sender.to_owned(),
                    )
                })
            })
            .collect::<HashMap<_, _>>();

        let native_names = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            self.native_member_names(channel_id),
        )
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default()
        .into_iter()
        .map(|(id, name)| (id.trim_matches(['{', '}']).to_ascii_lowercase(), name))
        .collect::<HashMap<_, _>>();

        let resolutions = futures::stream::iter(unresolved.into_iter().map(|index| {
            let message = &messages[index];
            let message_id = message.id.as_str();
            let sender_id = message.sender_id.as_deref();
            let sender_key = sender_id.map(|id| id.trim_matches(['{', '}']).to_ascii_lowercase());
            let cached_name = sender_key
                .as_deref()
                .and_then(|id| known_names.get(id).cloned());
            let native_name = sender_key
                .as_deref()
                .and_then(|id| native_names.get(id).cloned());
            async move {
                if let (Some(sender_id), Some(name)) = (sender_id, cached_name) {
                    return Some((index, name, Some(sender_id.to_owned())));
                }
                if let (Some(sender_id), Some(name)) = (sender_id, native_name) {
                    return Some((index, name, Some(sender_id.to_owned())));
                }
                if let Some(sender_id) = sender_id
                    && let Ok(Ok(name)) = tokio::time::timeout(
                        std::time::Duration::from_secs(3),
                        self.user_display_name(sender_id),
                    )
                    .await
                {
                    return Some((index, name, Some(sender_id.to_owned())));
                }

                let path = format!("/teams/{team_id}/channels/{channel_id}/messages/{message_id}");
                let message = tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    self.graph_get::<GraphMessage>(&path),
                )
                .await
                .ok()?
                .ok()?;
                let identity = graph_message_sender(&message)?;
                let name = identity
                    .display_name
                    .as_deref()
                    .filter(|name| !name.trim().is_empty())?
                    .to_owned();
                Some((index, name, identity.id.clone()))
            }
        }))
        .buffer_unordered(8)
        .filter_map(|resolution| async move { resolution })
        .collect::<Vec<_>>()
        .await;

        for (index, name, sender_id) in resolutions {
            let Some(message) = messages.get_mut(index) else {
                continue;
            };
            message.sender = name;
            if sender_id.is_some() {
                message.sender_id = sender_id;
            }
            message.is_from_current_user = is_current_user_message(message, current_user);
        }
        for message in messages
            .iter_mut()
            .filter(|message| message.sender.trim().is_empty() || message.sender == "Unknown")
        {
            message.sender = "Unknown sender".to_owned();
        }
    }
}
