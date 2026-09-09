use super::*;

impl<'a> TeamsService<'a> {
    pub async fn list_chats(&self, limit: usize) -> Result<Vec<Chat>> {
        self.list_chats_inner(limit, true).await
    }

    pub async fn list_chats_uncached(&self, limit: usize) -> Result<Vec<Chat>> {
        self.list_chats_inner(limit, false).await
    }

    async fn list_chats_inner(&self, limit: usize, persist: bool) -> Result<Vec<Chat>> {
        chat_diag!(format!("list_chats start limit={limit}"));
        debug_assert!(should_refresh_chats());
        let current_user = self.current_user().await.ok();
        chat_diag!(format!(
            "current_user id={} name={}",
            current_user
                .as_ref()
                .and_then(|user| user.id.as_deref())
                .unwrap_or("<missing>"),
            current_user
                .as_ref()
                .and_then(|user| user.display_name.as_deref())
                .unwrap_or("<missing>")
        ));
        let chats = if persist && GRAPH_CHATS_FORBIDDEN.load(Ordering::Relaxed) {
            let chats = self.list_native_chats(limit, current_user.as_ref()).await?;
            chat_diag!(format!("source=native returned={}", chats.len()));
            chats
        } else {
            match self.list_graph_chats(current_user.as_ref(), limit).await {
                Ok(chats) => {
                    chat_diag!(format!("source=graph returned={}", chats.len()));
                    chats
                }
                Err(error) => {
                    if persist && is_forbidden(&error) {
                        GRAPH_CHATS_FORBIDDEN.store(true, Ordering::Relaxed);
                    }
                    chat_diag!(format!("source=graph error={error:#}"));
                    tracing::warn!("Could not load chats from Microsoft Graph: {error:#}");
                    let chats = self.list_native_chats(limit, current_user.as_ref()).await?;
                    chat_diag!(format!("source=native returned={}", chats.len()));
                    chats
                }
            }
        };

        if persist {
            if let Ok(mut cache) = chat_cache().write() {
                *cache = Some(chats.clone());
            }
            if let Ok(serialized) = serde_json::to_string(&chats) {
                tokio::task::spawn_blocking(move || {
                    if let Err(error) = PlatformCacheStorage::new()
                        .and_then(|cache| cache.set("chat-cache", &serialized))
                    {
                        tracing::warn!("Could not persist the Teams chat cache: {error:#}");
                    }
                });
            }
        }
        Ok(chats)
    }

    pub(super) async fn list_native_conversations(
        &self,
        limit: usize,
        channel_team_ids: &HashSet<String>,
    ) -> Result<Vec<Conversation>> {
        let page_size = 50;
        let mut conversations = Vec::new();
        let mut seen_ids = HashSet::new();
        let mut visible_count = 0;
        let mut next_url = Some(format!(
            "{DEFAULT_CHAT_SERVICE}/v1/users/ME/conversations?view=mychats&pageSize={page_size}"
        ));

        while visible_count < limit {
            let Some(url) = next_url.take() else {
                break;
            };
            let response = self.chat_get::<ConversationsResponse>(&url).await?;
            let backward_link = response
                .metadata
                .and_then(|metadata| metadata.backward_link)
                .filter(|link| link != &url && !link.is_empty());
            let page = response.conversations.unwrap_or_default();
            chat_diag!(format!(
                "native_page items={} has_backward={}",
                page.len(),
                backward_link.is_some()
            ));
            #[cfg(debug_assertions)]
            for conversation in &page {
                chat_diag!(format!(
                    "native_raw id={} topic={} last_sender={}",
                    conversation.id.as_deref().unwrap_or("<missing>"),
                    conversation
                        .thread_properties
                        .as_ref()
                        .and_then(|properties| properties.topic.as_deref())
                        .unwrap_or("<missing>"),
                    conversation
                        .last_message
                        .as_ref()
                        .and_then(|message| message.im_display_name.as_deref())
                        .unwrap_or("<missing>")
                ));
            }
            let page_len = page.len();
            for conversation in page {
                let is_new = conversation
                    .id
                    .as_deref()
                    .is_none_or(|id| seen_ids.insert(id.to_owned()));
                if !is_new {
                    continue;
                }
                let visible = conversation
                    .id
                    .as_deref()
                    .is_some_and(|id| is_user_chat_id(id) && !channel_team_ids.contains(id));
                conversations.push(conversation);
                if visible {
                    visible_count += 1;
                    if visible_count >= limit {
                        break;
                    }
                }
            }
            if visible_count >= limit || page_len == 0 {
                break;
            }
            next_url = backward_link;
        }
        Ok(conversations)
    }

    pub(super) async fn list_native_chats(
        &self,
        limit: usize,
        current_user: Option<&GraphUser>,
    ) -> Result<Vec<Chat>> {
        let teams = self.list_teams().await.unwrap_or_default();
        let channel_team_ids = teams
            .into_iter()
            .flat_map(|team| team.channels.into_iter().map(|channel| channel.id))
            .collect::<HashSet<_>>();
        let conversations = match self
            .list_native_conversations(limit, &channel_team_ids)
            .await
        {
            Ok(conversations) => conversations,
            Err(network_error) => return cached_chats(network_error).await,
        };
        let mut chat_members = if GRAPH_CHATS_FORBIDDEN.load(Ordering::Relaxed) {
            HashMap::new()
        } else {
            match current_user.and_then(|user| user.id.as_deref()) {
                Some(current_user_id) => match self.chat_members(current_user_id).await {
                    Ok(members) => members,
                    Err(error) => {
                        if is_forbidden(&error) {
                            GRAPH_CHATS_FORBIDDEN.store(true, Ordering::Relaxed);
                        }
                        tracing::warn!("Could not load chat members for avatar data: {error:#}");
                        HashMap::new()
                    }
                },
                None => HashMap::new(),
            }
        };
        if let Some(current_user_id) = current_user.and_then(|user| user.id.as_deref()) {
            let missing_members = conversations
                .iter()
                .filter_map(|conversation| {
                    let chat_id = conversation.id.as_deref()?;
                    if channel_team_ids.contains(chat_id)
                        || chat_members
                            .get(chat_id)
                            .is_some_and(|members| !members.names.is_empty())
                    {
                        return None;
                    }
                    let user_id = direct_chat_other_user_id(chat_id, Some(current_user_id))?;
                    Some((chat_id.to_owned(), user_id))
                })
                .collect::<Vec<_>>();
            let current_user_id = current_user_id.to_owned();
            let resolved_members =
                futures::stream::iter(missing_members.into_iter().map(|(chat_id, user_id)| {
                    let current_user_id = current_user_id.clone();
                    async move {
                        let cached_name = load_cached_messages(&chat_id)
                            .await
                            .ok()
                            .flatten()
                            .and_then(|messages| {
                                messages.into_iter().rev().find_map(|message| {
                                    let sender = message.sender.trim();
                                    let is_current = message.is_from_current_user
                                        || message
                                            .sender_id
                                            .as_deref()
                                            .is_some_and(|id| ids_equal(id, &current_user_id));
                                    (!is_current
                                        && sender != "Unknown"
                                        && is_displayable_chat_name(sender))
                                    .then(|| sender.to_owned())
                                })
                            });
                        let display_name = match cached_name {
                            Some(name) => Some(name),
                            None => self.user_display_name(&user_id).await.ok(),
                        };
                        (chat_id, user_id, display_name)
                    }
                }))
                .buffer_unordered(8)
                .collect::<Vec<_>>()
                .await;
            for (chat_id, user_id, display_name) in resolved_members {
                if let Some(display_name) = display_name {
                    chat_members.insert(
                        chat_id,
                        ChatMembers {
                            names: vec![display_name],
                            photo_user_id: Some(user_id.clone()),
                            user_ids: vec![user_id],
                        },
                    );
                }
            }

            let group_ids = conversations
                .iter()
                .filter_map(|conversation| {
                    let chat_id = conversation.id.as_deref()?;
                    let is_group = chat_id.contains("thread") || chat_id.contains("meeting");
                    (is_group && !channel_team_ids.contains(chat_id)).then_some(chat_id)
                })
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let cached_group_metadata =
                futures::stream::iter(group_ids.into_iter().map(|chat_id| {
                    let current_user_id = current_user_id.clone();
                    async move {
                        let mut names = Vec::new();
                        let mut user_ids = Vec::new();
                        if let Some(messages) = load_cached_messages(&chat_id).await.ok().flatten()
                        {
                            for message in messages.into_iter().rev() {
                                let sender = message.sender.trim();
                                let is_current = message.is_from_current_user
                                    || message
                                        .sender_id
                                        .as_deref()
                                        .is_some_and(|id| ids_equal(id, &current_user_id));
                                if !is_current
                                    && sender != "Unknown"
                                    && !sender.is_empty()
                                    && is_displayable_chat_name(sender)
                                    && !names
                                        .iter()
                                        .any(|name: &String| name.eq_ignore_ascii_case(sender))
                                    && names.len() < 4
                                {
                                    names.push(sender.to_owned());
                                }
                                if !is_current
                                    && let Some(sender_id) = message.sender_id
                                    && !user_ids.iter().any(|id: &String| ids_equal(id, &sender_id))
                                    && user_ids.len() < 4
                                {
                                    user_ids.push(sender_id);
                                }
                                if names.len() >= 4 && user_ids.len() >= 4 {
                                    break;
                                }
                            }
                        }
                        (chat_id, names, user_ids)
                    }
                }))
                .buffer_unordered(16)
                .collect::<Vec<_>>()
                .await;
            for (chat_id, names, user_ids) in cached_group_metadata {
                if names.is_empty() && user_ids.is_empty() {
                    continue;
                }
                match chat_members.entry(chat_id) {
                    std::collections::hash_map::Entry::Occupied(mut entry) => {
                        let members = entry.get_mut();
                        if members.names.is_empty() {
                            members.names = names;
                        }
                        if members.user_ids.is_empty() {
                            members.user_ids = user_ids;
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert(ChatMembers {
                            names,
                            photo_user_id: None,
                            user_ids,
                        });
                    }
                }
            }
        }
        let mut chats = conversations
            .into_iter()
            .filter_map(|conversation| {
                let id = conversation.id?;
                if !is_user_chat_id(&id) || channel_team_ids.contains(&id) {
                    return None;
                }
                let last_message = conversation.last_message;
                let last_message_id = last_message.as_ref().and_then(|message| message.id.clone());
                let raw_topic = conversation
                    .thread_properties
                    .as_ref()
                    .and_then(|properties| properties.topic.as_deref());
                let raw_last_message_display_name = last_message
                    .as_ref()
                    .and_then(|message| message.im_display_name.as_deref());
                let members = chat_members.get(&id);
                let fallback_sender_id = last_message
                    .as_ref()
                    .and_then(|message| native_sender_id(message.from.as_deref()));
                let member_names = members.map(|members| members.names.as_slice());
                let current_user_name = current_user.and_then(|user| user.display_name.as_deref());
                let is_group = id.contains("thread") || id.contains("meeting");
                let name = chat_name_or_id(
                    raw_topic,
                    member_names,
                    raw_last_message_display_name,
                    current_user_name,
                    &id,
                )
                .or_else(|| {
                    if is_self_chat_id(&id, current_user.and_then(|user| user.id.as_deref())) {
                        return Some(
                            current_user_name
                                .filter(|name| is_displayable_chat_name(name))
                                .unwrap_or("Self chat")
                                .to_owned(),
                        );
                    }
                    let has_message = last_message.as_ref().is_some_and(|message| {
                        message.content.as_deref().is_some_and(|content| {
                            !message_content_and_quotes(content).0.is_empty()
                                || content.contains("<img")
                        })
                    });
                    if !has_message && members.is_none_or(|members| members.user_ids.is_empty()) {
                        return None;
                    }
                    Some(if is_group {
                        "Unnamed group chat".to_owned()
                    } else {
                        "Unnamed chat".to_owned()
                    })
                })?;
                chat_diag!(format!(
                    "native_classify id={} self={} group={} name={}",
                    id,
                    is_self_chat_id(&id, current_user.and_then(|user| user.id.as_deref())),
                    is_group,
                    name
                ));
                Some(Chat {
                    is_group,
                    profile_photo_user_id: profile_photo_user_id(
                        members,
                        &id,
                        current_user.and_then(|user| user.id.as_deref()),
                    ),
                    member_user_ids: members
                        .map(|members| members.user_ids.clone())
                        .filter(|ids| !ids.is_empty())
                        .or_else(|| fallback_sender_id.clone().map(|id| vec![id]))
                        .unwrap_or_default(),
                    team_id: None,
                    last_message_id,
                    last_message_preview: last_message
                        .and_then(|message| message.content)
                        .map(|content| message_content_and_quotes(&content).0),
                    id,
                    name,
                })
            })
            .collect::<Vec<_>>();

        let missing_previews = chats
            .iter()
            .enumerate()
            .filter(|(_, chat)| {
                chat.last_message_preview
                    .as_deref()
                    .is_none_or(str::is_empty)
            })
            .map(|(index, chat)| (index, chat.id.clone()))
            .collect::<Vec<_>>();
        let cached_previews = futures::stream::iter(missing_previews.into_iter().map(
            |(index, chat_id)| async move {
                let preview = load_cached_messages(&chat_id)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|messages| {
                        messages.into_iter().rev().find_map(|message| {
                            let content = message.content.trim();
                            (!content.is_empty()).then(|| content.to_owned())
                        })
                    });
                (index, preview)
            },
        ))
        .buffer_unordered(8)
        .collect::<Vec<_>>()
        .await;
        for (index, preview) in cached_previews {
            if let Some(preview) = preview {
                chats[index].last_message_preview = Some(preview);
            }
        }
        Ok(chats)
    }

    pub(super) async fn native_member_names(
        &self,
        thread_id: &str,
    ) -> Result<HashMap<String, String>> {
        let response: NativeMembersResponse = self
            .chat_get(&format!(
                "{DEFAULT_CHAT_SERVICE}/v1/threads/{thread_id}/members"
            ))
            .await?;
        Ok(response
            .members
            .into_iter()
            .filter_map(|member| {
                let id = native_sender_id(Some(&member.id))?;
                let name = member
                    .user_display_name
                    .filter(|name| is_displayable_chat_name(name))?;
                Some((id, name))
            })
            .collect())
    }
}
