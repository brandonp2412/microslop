use super::*;

pub(super) fn html_escape(value: &str) -> String {
    if !value
        .as_bytes()
        .iter()
        .any(|byte| matches!(byte, b'&' | b'<' | b'>' | b'"' | b'\''))
    {
        return value.to_owned();
    }
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

pub(super) fn message_html(value: &str) -> String {
    let normalized = value.trim().replace("\r\n", "\n").replace('\r', "\n");
    let mut result = String::new();
    let mut in_code = false;
    let mut code_has_content = false;
    for line in normalized.split('\n') {
        if line.trim_start().starts_with("```") {
            if in_code {
                result.push_str("</code></pre>");
                in_code = false;
            } else {
                if !result.is_empty() {
                    result.push_str("<br>");
                }
                result.push_str("<pre><code>");
                in_code = true;
                code_has_content = false;
            }
            continue;
        }
        if in_code {
            if code_has_content {
                result.push('\n');
            }
            result.push_str(&html_escape(line));
            code_has_content = true;
        } else {
            if !result.is_empty() {
                result.push_str("<br>");
            }
            result.push_str(&linkify_html(line));
        }
    }
    if in_code {
        result.push_str("</code></pre>");
    }
    result
}

pub(super) fn linkify_html(value: &str) -> String {
    if !value.contains("https://") && !value.contains("http://") {
        return html_escape(value);
    }
    let mut result = String::new();
    let mut offset = 0;
    while offset < value.len() {
        let rest = &value[offset..];
        let Some(start) = [rest.find("https://"), rest.find("http://")]
            .into_iter()
            .flatten()
            .min()
        else {
            result.push_str(&html_escape(rest));
            break;
        };
        result.push_str(&html_escape(&rest[..start]));
        let url_start = offset + start;
        let tail = &value[url_start..];
        let raw_end = tail
            .find(|character: char| character.is_whitespace())
            .unwrap_or(tail.len());
        let raw = &tail[..raw_end];
        let link = raw.trim_end_matches(|character: char| {
            matches!(
                character,
                '.' | ',' | '!' | '?' | ';' | ':' | ')' | ']' | '}'
            )
        });
        let escaped = html_escape(link);
        result.push_str("<a href=\"");
        result.push_str(&escaped);
        result.push_str("\">");
        result.push_str(&escaped);
        result.push_str("</a>");
        result.push_str(&html_escape(&raw[link.len()..]));
        offset = url_start + raw_end;
    }
    result
}
pub(super) fn display_name_or_id(display_name: Option<&str>, id: &str) -> String {
    display_name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(id)
        .to_owned()
}
pub(super) fn chat_name_or_id(
    topic: Option<&str>,
    member_names: Option<&[String]>,
    last_message_display_name: Option<&str>,
    current_user_name: Option<&str>,
    id: &str,
) -> Option<String> {
    (!id.starts_with("48:"))
        .then(|| {
            topic
                .filter(|name| is_displayable_chat_name(name))
                .map(ToOwned::to_owned)
                .or_else(|| {
                    member_names
                        .filter(|names| !names.is_empty())
                        .map(|names| names.join(", "))
                })
                .or_else(|| {
                    last_message_display_name
                        .filter(|name| is_displayable_chat_name(name))
                        .map(ToOwned::to_owned)
                })
                .filter(|name| Some(name.as_ref()) != current_user_name)
        })
        .flatten()
}
pub(super) fn custom_reaction_parts(reaction_type: &str) -> Option<(String, String)> {
    let (shortcut, document_id) = reaction_type.trim().split_once(';')?;
    let shortcut = shortcut.trim();
    let document_id = document_id.trim();
    if shortcut.is_empty() || document_id.is_empty() || document_id.contains(';') {
        return None;
    }
    Some((shortcut.to_owned(), document_id.to_owned()))
}

pub(super) fn ams_permissions(chat_id: &str) -> serde_json::Value {
    let user_ids = chat_cache()
        .read()
        .ok()
        .and_then(|cache| cache.as_ref().cloned())
        .and_then(|chats| chats.into_iter().find(|chat| chat.id == chat_id))
        .map(|chat| chat.member_user_ids)
        .unwrap_or_default();
    let mut permissions = serde_json::Map::new();
    for user_id in user_ids {
        let user_id = user_id.trim();
        if user_id.is_empty() {
            continue;
        }
        let identity = if user_id.contains(':') {
            user_id.to_owned()
        } else {
            format!("8:orgid:{user_id}")
        };
        permissions.insert(identity, serde_json::json!(["read"]));
    }
    if permissions.is_empty() {
        permissions.insert(chat_id.to_owned(), serde_json::json!(["read"]));
    }
    serde_json::Value::Object(permissions)
}

pub(super) fn audio_message_content(objects_url: &str, object_id: &str) -> String {
    microsoft_teams::audio_message_content(objects_url, object_id)
}

pub(super) fn is_user_chat_id(id: &str) -> bool {
    id == "48:notes" || !id.starts_with("48:")
}
pub(super) fn is_displayable_chat_name(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty()
        && !trimmed.eq_ignore_ascii_case("undefined")
        && !looks_like_internal_chat_id(trimmed)
}

pub(super) fn looks_like_internal_chat_id(value: &str) -> bool {
    value.starts_with("19:")
        && (value.contains("@thread") || value.contains("@unq.gbl.spaces") || value.len() > 24)
}
pub(super) fn profile_photo_user_id(
    members: Option<&ChatMembers>,
    chat_id: &str,
    current_user_id: Option<&str>,
) -> Option<String> {
    if is_self_chat_id(chat_id, current_user_id) {
        return current_user_id.map(ToOwned::to_owned);
    }
    members
        .and_then(|members| members.photo_user_id.clone())
        .or_else(|| direct_chat_other_user_id(chat_id, current_user_id))
}
pub(super) fn direct_chat_other_user_id(
    chat_id: &str,
    current_user_id: Option<&str>,
) -> Option<String> {
    let (first_user_id, second_user_id) = chat_id
        .strip_prefix("19:")?
        .strip_suffix("@unq.gbl.spaces")?
        .split_once('_')?;
    match current_user_id {
        Some(current_user_id) if first_user_id == current_user_id => {
            Some(second_user_id.to_owned())
        }
        Some(current_user_id) if second_user_id == current_user_id => {
            Some(first_user_id.to_owned())
        }
        _ => None,
    }
}

pub(super) fn is_self_chat_id(chat_id: &str, current_user_id: Option<&str>) -> bool {
    let Some(current_user_id) = current_user_id else {
        return false;
    };
    if chat_id == "48:notes" {
        return true;
    }
    let Some((first, second)) = chat_id
        .strip_prefix("19:")
        .and_then(|id| id.strip_suffix("@unq.gbl.spaces"))
        .and_then(|id| id.split_once('_'))
    else {
        return false;
    };
    first == current_user_id && second == current_user_id
}

pub(super) fn is_graph_self_chat(
    chat_type: Option<&str>,
    members: &[GraphChatMember],
    current_user_id: Option<&str>,
) -> bool {
    chat_type == Some("oneOnOne")
        && current_user_id.is_some_and(|current_user_id| {
            !members.is_empty()
                && members.iter().all(|member| {
                    member
                        .user_id
                        .as_deref()
                        .is_some_and(|member_id| ids_equal(member_id, current_user_id))
                })
        })
}

pub(super) const fn should_refresh_chats() -> bool {
    true
}
pub(super) fn graph_message_sender(message: &GraphMessage) -> Option<&GraphMessageUser> {
    let from = message.from.as_ref()?;
    from.user.as_ref().or(from.application.as_ref())
}

pub(super) fn graph_message(
    message: GraphMessage,
    current_user: Option<&GraphUser>,
    images: Vec<MessageImage>,
) -> Option<ChatMessage> {
    let sender = graph_message_sender(&message);
    let sender_name = sender
        .and_then(|identity| identity.display_name.clone())
        .unwrap_or_else(|| "Unknown".to_owned());
    let sender_id = sender.and_then(|identity| identity.id.clone());
    let is_from_current_user = sender_id
        .as_deref()
        .zip(current_user.and_then(|user| user.id.as_deref()))
        .is_some_and(|(sender, current)| ids_equal(sender, current));
    let raw_content = message
        .body
        .as_ref()
        .and_then(|body| body.content.as_deref())
        .unwrap_or_default();
    let (content, mut quotes) = message_content_and_quotes(raw_content);
    for quote in graph_message_reference_quotes(message.attachments.as_deref().unwrap_or_default())
    {
        if !quotes.contains(&quote) {
            quotes.push(quote);
        }
    }
    let content = if content.trim().is_empty() {
        message
            .attachments
            .as_ref()
            .into_iter()
            .flatten()
            .filter(|attachment| attachment.content_type.as_deref() == Some("reference"))
            .filter_map(|attachment| attachment.name.as_deref())
            .map(|name| format!("📎 {name}"))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        content
    };
    let has_image_attachments = message.attachments.iter().flatten().any(|attachment| {
        attachment
            .content_type
            .as_deref()
            .is_some_and(|kind| kind.starts_with("image/"))
            || attachment.thumbnail_url.is_some()
            || attachment
                .content
                .as_deref()
                .is_some_and(|content| !image_src_urls(content).is_empty())
    });
    (!content.trim().is_empty()
        || !images.is_empty()
        || raw_content.contains("<img")
        || has_image_attachments)
        .then_some(ChatMessage {
            id: message.id.unwrap_or_default(),
            sender: sender_name,
            sender_id,
            is_from_current_user,
            timestamp: message.created_date_time.unwrap_or_default(),
            content: content.trim().to_owned(),
            quotes,
            images,
            image_urls: image_src_urls(raw_content),
            reactions: reaction_counts(
                message.reactions,
                current_user.and_then(|user| user.id.as_deref()),
            ),
        })
}
pub(super) fn native_message(
    message: NativeMessage,
    images: Vec<MessageImage>,
    current_user_id: Option<&str>,
) -> Option<ChatMessage> {
    let message_type = message.messagetype.as_deref().unwrap_or_default();
    let has_images = message
        .content
        .as_deref()
        .is_some_and(|content| content.contains("<img"));
    let (content, quotes) = if message_type == "RichText/Media_AudioMsg" {
        ("Voice message".to_owned(), Vec::new())
    } else {
        message_content_and_quotes(message.content.as_deref().unwrap_or_default())
    };
    (message_type.contains("Text") || message_type.contains("RichText"))
        .then_some(ChatMessage {
            id: message.id.unwrap_or_default(),
            sender: message
                .im_display_name
                .unwrap_or_else(|| "Unknown".to_owned()),
            sender_id: native_sender_id(message.from.as_deref()),
            is_from_current_user: false,
            timestamp: message
                .original_arrival_time
                .or(message.compose_time)
                .unwrap_or_default(),
            content: content.trim().to_owned(),
            quotes,
            images,
            image_urls: image_src_urls(message.content.as_deref().unwrap_or_default()),
            reactions: native_reaction_counts(
                message
                    .properties
                    .and_then(|properties| properties.emotions),
                current_user_id,
            ),
        })
        .filter(|message| !message.content.is_empty() || !message.images.is_empty() || has_images)
}
pub(super) fn native_sender_id(sender: Option<&str>) -> Option<String> {
    let sender = sender?.trim();
    let normalized = sender
        .strip_prefix("8:orgid:")
        .or_else(|| sender.strip_prefix("orgid:"))
        .or_else(|| sender.strip_prefix("8:guest:"))
        .or_else(|| sender.strip_prefix("8:acs:"))
        .or_else(|| sender.rsplit(':').next())?;
    if normalized.is_empty() {
        return None;
    }
    if uuid::Uuid::parse_str(normalized.trim_matches(['{', '}'])).is_err() {
        return None;
    }
    Some(normalized.to_owned())
}

pub(super) fn ids_equal(left: &str, right: &str) -> bool {
    left.trim_matches(['{', '}'])
        .eq_ignore_ascii_case(right.trim_matches(['{', '}']))
}

pub(super) fn is_current_user_message(
    message: &ChatMessage,
    current_user: Option<&GraphUser>,
) -> bool {
    let Some(current_user) = current_user else {
        return false;
    };
    if let (Some(sender_id), Some(current_user_id)) =
        (message.sender_id.as_deref(), current_user.id.as_deref())
        && ids_equal(sender_id, current_user_id)
    {
        return true;
    }
    current_user
        .display_name
        .as_deref()
        .is_some_and(|name| message.sender.trim().eq_ignore_ascii_case(name.trim()))
}

pub(super) fn message_content_and_quotes(html: &str) -> (String, Vec<MessageQuote>) {
    let mut content = html.to_owned();
    let mut quotes = Vec::new();
    let mut search_from = 0;
    loop {
        let lower = content.to_ascii_lowercase();
        let Some(relative_start) = lower[search_from..].find("<blockquote") else {
            break;
        };
        let start = search_from + relative_start;
        let Some(open_end_offset) = lower[start..].find('>') else {
            break;
        };
        let open_end = start + open_end_offset + 1;
        let opening = &content[start + 1..open_end - 1];
        if !microsoft_teams::contains_reply_schema(opening) {
            search_from = open_end;
            continue;
        }
        let Some(close_offset) = lower[open_end..].find("</blockquote>") else {
            break;
        };
        let close_end = open_end + close_offset + "</blockquote>".len();
        let block = &content[start..close_end];
        let message_id = html_attr(opening, "itemid").filter(|value| !value.trim().is_empty());
        let sender = html_itemprop_text(block, "mri").unwrap_or_default();
        let quote_content = html_itemprop_text(block, "preview")
            .or_else(|| html_itemprop_text(block, "copy"))
            .unwrap_or_default();
        if !sender.trim().is_empty() || !quote_content.trim().is_empty() {
            quotes.push(MessageQuote {
                message_id,
                sender: sender.trim().to_owned(),
                content: quote_content.trim().to_owned(),
            });
        }
        content.replace_range(start..close_end, "");
        search_from = start;
    }
    (strip_html(&content), quotes)
}

pub(super) fn html_itemprop_text(html: &str, property: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let property = property.to_ascii_lowercase();
    let needles = [
        format!("itemprop=\"{property}\""),
        format!("itemprop='{property}'"),
        format!("itemprop={property}"),
    ];
    let property_index = needles
        .iter()
        .filter_map(|needle| lower.find(needle))
        .min()?;
    let start = lower[..property_index].rfind('<')?;
    let open_end = lower[property_index..].find('>')? + property_index + 1;
    let tag_name = lower[start + 1..open_end - 1]
        .split_whitespace()
        .next()?
        .trim_matches('/');
    if tag_name.is_empty() {
        return None;
    }
    let close = format!("</{tag_name}>");
    let close_start = lower[open_end..].find(&close)? + open_end;
    Some(strip_html(&html[open_end..close_start]))
}

pub(super) fn graph_message_reference_quotes(
    attachments: &[GraphMessageAttachment],
) -> Vec<MessageQuote> {
    attachments
        .iter()
        .filter(|attachment| {
            attachment
                .content_type
                .as_deref()
                .is_some_and(|content_type| content_type.eq_ignore_ascii_case("messageReference"))
        })
        .filter_map(|attachment| attachment.content.as_deref())
        .filter_map(|content| serde_json::from_str::<serde_json::Value>(content).ok())
        .filter_map(|value| {
            let sender = value
                .pointer("/messageSender/user/displayName")
                .and_then(serde_json::Value::as_str)
                .or_else(|| {
                    value
                        .pointer("/messageSender/application/displayName")
                        .and_then(serde_json::Value::as_str)
                })
                .unwrap_or_default()
                .trim()
                .to_owned();
            let content = value
                .get("messagePreview")
                .and_then(serde_json::Value::as_str)
                .map(strip_html)
                .unwrap_or_default();
            if sender.is_empty() && content.is_empty() {
                return None;
            }
            Some(MessageQuote {
                message_id: value
                    .get("messageId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(ToOwned::to_owned),
                sender,
                content,
            })
        })
        .collect()
}

fn is_inline_emoji_tag(tag: &str) -> bool {
    let tag_name = tag
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_start_matches('/')
        .trim_end_matches('/');
    tag_name.eq_ignore_ascii_case("emoji")
        || tag_name.eq_ignore_ascii_case("customemoji")
        || html_attr(tag, "itemtype")
            .is_some_and(|itemtype| itemtype.eq_ignore_ascii_case("http://schema.skype.com/Emoji"))
}

fn without_inline_emoji_tags(html: &str) -> std::borrow::Cow<'_, str> {
    let mut remainder = html;
    let has_inline_emoji = loop {
        let Some(start) = remainder.find('<') else {
            break false;
        };
        remainder = &remainder[start..];
        let Some(end) = remainder.find('>') else {
            break false;
        };
        if is_inline_emoji_tag(&remainder[1..end]) {
            break true;
        }
        remainder = &remainder[end + 1..];
    };
    if !has_inline_emoji {
        return std::borrow::Cow::Borrowed(html);
    }
    let mut result = String::with_capacity(html.len());
    let mut remainder = html;
    while let Some(start) = remainder.find('<') {
        result.push_str(&remainder[..start]);
        remainder = &remainder[start..];
        let Some(end) = remainder.find('>') else {
            result.push_str(remainder);
            return std::borrow::Cow::Owned(result);
        };
        let tag = &remainder[1..end];
        if !is_inline_emoji_tag(tag) {
            result.push_str(&remainder[..=end]);
        }
        remainder = &remainder[end + 1..];
    }
    result.push_str(remainder);
    std::borrow::Cow::Owned(result)
}

pub(super) fn image_src_urls(html: &str) -> Vec<String> {
    let html = without_inline_emoji_tags(html);
    let mut urls = Vec::new();
    let mut seen = HashSet::new();
    let mut remainder = html.as_ref();
    while let Some(index) = remainder.find("<img") {
        remainder = &remainder[index + 4..];
        let Some(end) = remainder.find('>') else {
            break;
        };
        let tag = &remainder[..end];
        for marker in ["src=\"", "src='", "data-orig-src=\"", "target-src=\""] {
            let Some(start) = tag.find(marker) else {
                continue;
            };
            let value = &tag[start + marker.len()..];
            let quote = marker.chars().last().unwrap_or('"');
            let url = value.split(quote).next().unwrap_or_default().trim();
            if url.starts_with("https://") && !url.contains("/hostedContents/") && seen.insert(url)
            {
                urls.push(url.to_owned());
            }
            break;
        }
        remainder = &remainder[end + 1..];
    }
    let mut remainder = html.as_ref();
    while let Some(index) = remainder.find("https://") {
        remainder = &remainder[index..];
        let end = remainder
            .find(|character: char| {
                character == '"'
                    || character == '\''
                    || character == '<'
                    || character == '>'
                    || character == '\\'
                    || character.is_whitespace()
            })
            .unwrap_or(remainder.len());
        let url = remainder[..end].trim_end_matches([')', ']', '}', ',']);
        let path = url
            .split(['?', '#'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if [".gif", ".png", ".jpg", ".jpeg", ".webp"]
            .iter()
            .any(|extension| path.ends_with(extension))
            && seen.insert(url)
        {
            urls.push(url.to_owned());
        }
        remainder = &remainder[end..];
    }
    urls
}

pub(super) fn hosted_content_ids(html: &str) -> Vec<String> {
    let html = without_inline_emoji_tags(html);
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    let mut remainder = html.as_ref();
    while let Some(index) = remainder.find("hostedContents/") {
        remainder = &remainder[index + "hostedContents/".len()..];
        let id = remainder
            .split(|character: char| {
                character == '/'
                    || character == '"'
                    || character == '\''
                    || character.is_whitespace()
            })
            .next()
            .unwrap_or_default();
        if id.is_empty() {
            break;
        }
        if seen.insert(id) {
            ids.push(id.to_owned());
        }
        remainder = &remainder[id.len()..];
    }
    ids
}
pub(super) fn order_chat_messages_chronologically(
    mut messages: Vec<ChatMessage>,
) -> Vec<ChatMessage> {
    messages.sort_by(|left, right| left.timestamp.cmp(&right.timestamp));
    let mut seen_ids = HashSet::new();
    let mut seen_messages = HashSet::new();
    messages.retain(|message| {
        if !message.id.is_empty() && !seen_ids.insert(message.id.clone()) {
            return false;
        }
        let sender = if message.is_from_current_user {
            "@current".to_owned()
        } else {
            message
                .sender_id
                .as_deref()
                .filter(|id| !id.is_empty())
                .unwrap_or(&message.sender)
                .to_ascii_lowercase()
        };
        seen_messages.insert((
            message.timestamp.clone(),
            message.content.trim().to_owned(),
            sender,
            message.images.len(),
        ))
    });
    messages
}
pub(super) fn reaction_counts(
    reactions: Option<Vec<GraphReaction>>,
    current_user_id: Option<&str>,
) -> Vec<MessageReaction> {
    let mut counts = std::collections::BTreeMap::new();
    for reaction in reactions.unwrap_or_default() {
        let Some(reaction_type) = reaction
            .reaction_type
            .filter(|reaction| !reaction.is_empty())
        else {
            continue;
        };
        let user = reaction.user.and_then(|identity| identity.user);
        let selected = user
            .as_ref()
            .and_then(|user| user.id.as_deref())
            .zip(current_user_id)
            .is_some_and(|(reactor, current)| ids_equal(reactor, current));
        let entry = counts
            .entry(reaction_type)
            .or_insert((0usize, false, Vec::new()));
        if let Some(user) = user {
            entry.2.push(ReactionUser {
                id: user.id.unwrap_or_default(),
                name: user.display_name.unwrap_or_default(),
            });
        }
        entry.0 += 1;
        entry.1 |= selected;
    }
    counts
        .into_iter()
        .map(
            |(reaction_type, (count, selected, users))| MessageReaction {
                reaction_type,
                count,
                selected,
                users,
            },
        )
        .collect()
}

pub(super) fn native_reaction_counts(
    emotions: Option<serde_json::Value>,
    current_user_id: Option<&str>,
) -> Vec<MessageReaction> {
    let emotions = match emotions {
        Some(serde_json::Value::String(value)) => serde_json::from_str(&value).ok(),
        Some(value @ serde_json::Value::Array(_)) => Some(value),
        _ => None,
    };
    let mut counts = std::collections::BTreeMap::new();
    let Some(serde_json::Value::Array(emotions)) = emotions else {
        return Vec::new();
    };
    for emotion in emotions {
        let Some(reaction_type) = emotion
            .get("key")
            .and_then(serde_json::Value::as_str)
            .filter(|key| !key.is_empty() && *key != "follow")
        else {
            continue;
        };
        let users = emotion
            .get("users")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        if users.is_empty() {
            continue;
        }
        let selected = current_user_id.is_some_and(|current| {
            users.iter().any(|user| {
                user.get("mri")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|mri| native_sender_id(Some(mri)))
                    .is_some_and(|reactor| ids_equal(&reactor, current))
            })
        });
        let reactors = users
            .iter()
            .map(|user| ReactionUser {
                id: native_sender_id(user.get("mri").and_then(serde_json::Value::as_str))
                    .unwrap_or_default(),
                name: microsoft_teams::reaction_user_name(user)
                    .unwrap_or_default()
                    .to_owned(),
            })
            .collect();
        counts.insert(reaction_type.to_owned(), (users.len(), selected, reactors));
    }
    counts
        .into_iter()
        .map(
            |(reaction_type, (count, selected, users))| MessageReaction {
                reaction_type,
                count,
                selected,
                users,
            },
        )
        .collect()
}
pub(super) fn html_attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let needle = format!("{name}=");
    let start = lower.find(&needle)? + needle.len();
    let rest = tag[start..].trim_start();
    let quote = rest.chars().next()?;
    if quote == '"' || quote == '\'' {
        let value = &rest[1..];
        return value.find(quote).map(|end| value[..end].to_owned());
    }
    Some(
        rest.split_whitespace()
            .next()
            .unwrap_or_default()
            .trim_end_matches('>')
            .to_owned(),
    )
}

pub(super) fn strip_html(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut tag = String::new();
    let mut in_tag = false;
    let mut active_href: Option<(String, usize)> = None;
    for character in value.chars() {
        match character {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let tag_name = tag
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .trim_end_matches('/')
                    .to_ascii_lowercase();
                if tag_name == "a" {
                    if let Some(href) = html_attr(&tag, "href").filter(|href| !href.is_empty()) {
                        active_href = Some((href, result.len()));
                    }
                } else if tag_name == "/a"
                    && let Some((href, anchor_start)) = active_href.take()
                {
                    let anchor_text = result.get(anchor_start..).unwrap_or_default();
                    if !anchor_text.contains(&href) {
                        if !result.ends_with([' ', '\n']) && !result.is_empty() {
                            result.push(' ');
                        }
                        result.push_str(&href);
                    }
                } else if is_inline_emoji_tag(&tag) && !tag_name.starts_with('/') {
                    if let Some(alt) = html_attr(&tag, "alt")
                        .or_else(|| html_attr(&tag, "title"))
                        .filter(|value| !value.is_empty())
                    {
                        result.push_str(&alt);
                    }
                } else if tag_name == "at" && !result.ends_with('@') {
                    result.push('@');
                } else if tag_name == "pre" {
                    if !result.ends_with('\n') && !result.is_empty() {
                        result.push('\n');
                    }
                    result.push_str("```\n");
                } else if tag_name == "/pre" {
                    if !result.ends_with('\n') {
                        result.push('\n');
                    }
                    result.push_str("```\n");
                }
                if matches!(tag_name.as_str(), "br" | "/p" | "/div" | "/li")
                    && !result.ends_with('\n')
                    && !result.is_empty()
                {
                    result.push('\n');
                }
            }
            _ if in_tag => tag.push(character),
            _ => result.push(character),
        }
    }
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .trim()
        .to_owned()
}
