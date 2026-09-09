use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use base64::Engine;
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch, Mutex};

use crate::api::{auth, calls, teams, trouter};

static MESSAGE_EVENTS: Lazy<Mutex<VecDeque<(u64, trouter::MessageEvent)>>> =
    Lazy::new(|| Mutex::new(VecDeque::new()));
static CALL_EVENTS: Lazy<Mutex<VecDeque<(u64, calls::CallEvent)>>> =
    Lazy::new(|| Mutex::new(VecDeque::new()));
static MESSAGE_PUMP_STARTED: AtomicBool = AtomicBool::new(false);
static MESSAGE_PUMP_READY: AtomicBool = AtomicBool::new(false);
static MESSAGE_PUMP_STOP: Lazy<Mutex<Option<watch::Sender<bool>>>> = Lazy::new(|| Mutex::new(None));
static CALL_PUMP_STARTED: AtomicBool = AtomicBool::new(false);

const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_REQUEST_BYTES: usize = 24 * 1024 * 1024;

pub async fn run_server(bind: &str) -> Result<()> {
    let bind_addr: SocketAddr = bind
        .parse()
        .with_context(|| format!("Invalid Microslop web bridge bind address: {bind}"))?;
    if !bind_addr.ip().is_loopback() {
        anyhow::bail!("Microslop web bridge must bind to a loopback address, not {bind_addr}");
    }
    start_message_pump();
    start_call_pump().await?;

    let listener = TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("Could not bind Microslop web bridge to {bind_addr}"))?;
    eprintln!("Microslop web bridge listening on http://{bind}");

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream).await {
                eprintln!("Microslop web bridge request failed: {error:#}");
            }
        });
    }
}

fn start_message_pump() {
    if MESSAGE_PUMP_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    let (sender, mut receiver) = mpsc::channel(64);
    tokio::spawn(async move {
        let mut backoff = Duration::from_secs(1);
        loop {
            let (stop_sender, stop_receiver) = watch::channel(false);
            *MESSAGE_PUMP_STOP.lock().await = Some(stop_sender);
            match trouter::listen_message_events_to(sender.clone(), stop_receiver).await {
                Ok(()) => backoff = Duration::from_secs(1),
                Err(error) => {
                    eprintln!("Microslop web bridge message listener stopped: {error:#}");
                }
            }
            MESSAGE_PUMP_STOP.lock().await.take();
            if sender.is_closed() {
                return;
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }
    });
    tokio::spawn(async move {
        while let Some(event) = receiver.recv().await {
            if event.conversation_id.is_empty() {
                MESSAGE_PUMP_READY.store(true, Ordering::SeqCst);
                continue;
            }
            let mut queue = MESSAGE_EVENTS.lock().await;
            let sequence = queue
                .back()
                .map(|(sequence, _)| sequence.saturating_add(1))
                .unwrap_or(1);
            queue.push_back((sequence, event));
            while queue.len() > 256 {
                queue.pop_front();
            }
        }
    });
}

async fn restart_message_pump() {
    if let Some(stop) = MESSAGE_PUMP_STOP.lock().await.take() {
        let _ = stop.send(true);
    }
}

async fn start_call_pump() -> Result<()> {
    if CALL_PUMP_STARTED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let mut receiver = calls::subscribe_events_for_web().await?;
    tokio::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    let mut queue = CALL_EVENTS.lock().await;
                    let sequence = queue
                        .back()
                        .map(|(sequence, _)| sequence.saturating_add(1))
                        .unwrap_or(1);
                    queue.push_back((sequence, event));
                    while queue.len() > 256 {
                        queue.pop_front();
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            }
        }
    });
    Ok(())
}

async fn handle_connection(mut stream: TcpStream) -> Result<()> {
    let mut request = Vec::new();
    let mut chunk = [0u8; 8192];
    let header_end = loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            anyhow::bail!("Connection closed before HTTP headers arrived");
        }
        request.extend_from_slice(&chunk[..read]);
        if request.len() > MAX_HEADER_BYTES {
            write_response(
                &mut stream,
                431,
                "{\"error\":\"Request headers are too large\"}",
                "null",
            )
            .await?;
            return Ok(());
        }
        if let Some(index) = find_bytes(&request, b"\r\n\r\n") {
            break index + 4;
        }
    };

    // Own the header text before extending `request` with the body. Some target
    // borrow checkers conservatively retain the slice borrow across the read loop.
    let headers = std::str::from_utf8(&request[..header_end])?.to_owned();
    let request_line = headers
        .split("\r\n")
        .next()
        .context("Missing HTTP request line")?;
    let request_parts = request_line.split_whitespace().collect::<Vec<_>>();
    if request_parts.len() != 3 || !request_parts[2].starts_with("HTTP/1.") {
        write_response(
            &mut stream,
            400,
            "{\"error\":\"Malformed HTTP request line\"}",
            "null",
        )
        .await?;
        return Ok(());
    }
    let method = request_parts[0].to_owned();
    let path = request_parts[1].to_owned();
    let content_length = match header_value(&headers, "content-length") {
        Some(value) => match value.parse::<usize>() {
            Ok(value) => value,
            Err(_) => {
                write_response(
                    &mut stream,
                    400,
                    "{\"error\":\"Invalid Content-Length\"}",
                    "null",
                )
                .await?;
                return Ok(());
            }
        },
        None => 0,
    };
    let origin = header_value(&headers, "origin").map(ToOwned::to_owned);
    let content_type = header_value(&headers, "content-type").map(ToOwned::to_owned);
    let allowed_origin = std::env::var("MICROSLOP_WEB_ORIGIN")
        .unwrap_or_else(|_| "http://127.0.0.1:18443".to_owned());
    if allowed_origin.contains(['\r', '\n']) {
        anyhow::bail!("MICROSLOP_WEB_ORIGIN contains an invalid header character");
    }
    if content_length > MAX_REQUEST_BYTES.saturating_sub(header_end) {
        write_response(
            &mut stream,
            413,
            "{\"error\":\"Request body is too large\"}",
            &allowed_origin,
        )
        .await?;
        return Ok(());
    }
    let target_len = header_end
        .checked_add(content_length)
        .context("Web bridge request length overflowed")?;
    while request.len() < target_len {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            write_response(
                &mut stream,
                400,
                "{\"error\":\"Request body ended early\"}",
                &allowed_origin,
            )
            .await?;
            return Ok(());
        }
        request.extend_from_slice(&chunk[..read]);
        if request.len() > MAX_REQUEST_BYTES {
            anyhow::bail!("Web bridge request was too large");
        }
    }
    let body = &request[header_end..request.len().min(target_len)];

    if method == "GET" && path == "/health" {
        write_response(&mut stream, 200, "{\"ok\":true}", &allowed_origin).await?;
        return Ok(());
    }
    if method != "POST" && method != "OPTIONS" {
        write_response(
            &mut stream,
            405,
            "{\"error\":\"Method is not allowed\"}",
            &allowed_origin,
        )
        .await?;
        return Ok(());
    }
    if origin.as_deref() != Some(allowed_origin.as_str()) {
        write_response(
            &mut stream,
            403,
            "{\"error\":\"Origin is not allowed\"}",
            &allowed_origin,
        )
        .await?;
        return Ok(());
    }
    if method == "OPTIONS" {
        write_response(&mut stream, 204, "{}", &allowed_origin).await?;
        return Ok(());
    }
    if !is_known_post_path(&path) {
        write_response(
            &mut stream,
            404,
            "{\"error\":\"Unknown web bridge route\"}",
            &allowed_origin,
        )
        .await?;
        return Ok(());
    }
    if !body.is_empty()
        && !content_type.as_deref().is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("application/json"))
        })
    {
        write_response(
            &mut stream,
            415,
            "{\"error\":\"Content-Type must be application/json\"}",
            &allowed_origin,
        )
        .await?;
        return Ok(());
    }

    let body_json = if body.is_empty() {
        json!({})
    } else {
        match serde_json::from_slice(body) {
            Ok(value) => value,
            Err(_) => {
                write_response(
                    &mut stream,
                    400,
                    "{\"error\":\"Invalid JSON request body\"}",
                    &allowed_origin,
                )
                .await?;
                return Ok(());
            }
        }
    };

    if !body_json.is_object() {
        write_response(
            &mut stream,
            400,
            "{\"error\":\"JSON request body must be an object\"}",
            &allowed_origin,
        )
        .await?;
        return Ok(());
    }

    match route(&method, &path, body_json).await {
        Ok(value) => {
            write_response(
                &mut stream,
                200,
                &serde_json::to_string(&value)?,
                &allowed_origin,
            )
            .await?
        }
        Err(error) => {
            let payload = serde_json::to_string(&json!({"error": format!("{error:#}")}))?;
            write_response(&mut stream, 500, &payload, &allowed_origin).await?;
        }
    }
    Ok(())
}

async fn route(method: &str, path: &str, body: Value) -> Result<Value> {
    if method == "GET" && path == "/health" {
        return Ok(json!({"ok": true}));
    }

    match (method, path) {
        ("POST", "/auth/status") => {
            let status = auth::get_auth_status().await?;
            Ok(json!({"signedIn": status.signed_in, "loginInProgress": status.login_in_progress}))
        }
        ("POST", "/auth/restore") => {
            let status = auth::restore_work_session().await?;
            Ok(json!({"signedIn": status.signed_in, "loginInProgress": status.login_in_progress}))
        }
        ("POST", "/auth/begin") => {
            let code = auth::begin_work_login().await?;
            Ok(json!({
                "verificationUri": code.verification_uri,
                "userCode": code.user_code,
                "expiresInSeconds": code.expires_in_seconds,
            }))
        }
        ("POST", "/auth/complete") => {
            let status = auth::complete_work_login().await?;
            restart_message_pump().await;
            Ok(json!({"signedIn": status.signed_in, "loginInProgress": status.login_in_progress}))
        }
        ("POST", "/auth/cancel") => {
            auth::cancel_work_login().await;
            Ok(json!({"ok": true}))
        }
        ("POST", "/auth/logout") => {
            let status = auth::logout().await?;
            restart_message_pump().await;
            Ok(json!({"signedIn": status.signed_in, "loginInProgress": status.login_in_progress}))
        }
        ("POST", "/auth/accounts") => Ok(Value::Array(
            auth::list_saved_accounts()
                .await?
                .into_iter()
                .map(|account| {
                    json!({
                        "id": account.id,
                        "displayName": account.display_name,
                        "username": account.username,
                    })
                })
                .collect(),
        )),
        ("POST", "/auth/active-account") => {
            Ok(json!({"accountId": auth::active_account_id().await?}))
        }
        ("POST", "/auth/switch") => {
            let status =
                auth::switch_saved_account(nonempty_string_field(&body, "accountId")?.to_owned())
                    .await?;
            restart_message_pump().await;
            Ok(json!({"signedIn": status.signed_in, "loginInProgress": status.login_in_progress}))
        }
        ("POST", "/user") => {
            let user = teams::get_user_profile().await?;
            Ok(json!({"id": user.id, "displayName": user.display_name, "email": user.email}))
        }
        ("POST", "/presences") => {
            let user_ids = body
                .get("userIds")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let presences = teams::get_presences(user_ids).await?;
            Ok(Value::Array(
                presences
                    .into_iter()
                    .map(|presence| {
                        json!({
                            "userId": presence.user_id,
                            "availability": presence.availability,
                            "activity": presence.activity,
                        })
                    })
                    .collect(),
            ))
        }
        ("POST", "/chats") => {
            let limit = bounded_usize_field(&body, "limit", 50, 1, 500)?;
            let chats = teams::list_chats(limit).await?;
            Ok(Value::Array(
                chats
                    .into_iter()
                    .map(|chat| {
                        json!({
                            "id": chat.id,
                            "name": chat.name,
                            "isGroup": chat.is_group,
                            "profilePhotoUserId": chat.profile_photo_user_id,
                            "memberUserIds": chat.member_user_ids,
                            "teamId": chat.team_id,
                            "preview": chat.last_message_preview,
                        })
                    })
                    .collect(),
            ))
        }
        ("POST", "/teams") => {
            let list = teams::list_teams().await?;
            Ok(Value::Array(
                list.into_iter()
                    .map(|team| {
                        json!({
                            "id": team.id,
                            "name": team.name,
                            "channels": team.channels.into_iter().map(|channel| json!({
                                "id": channel.id,
                                "name": channel.name,
                            })).collect::<Vec<_>>(),
                        })
                    })
                    .collect(),
            ))
        }
        ("POST", "/messages/image/read") => Ok(teams::load_message_image(
            nonempty_string_field(&body, "url")?.to_owned(),
        )
        .await?
        .map(|image| json!({"contentType": image.content_type, "dataBase64": image.data_base64, "sourceUrl": image.source_url}))
        .unwrap_or(Value::Null)),
        ("POST", "/reaction/user") => Ok(
            json!({"name": teams::reaction_user_name(nonempty_string_field(&body, "id")?.to_owned()).await?}),
        ),
        ("POST", "/messages/read") => {
            let id = nonempty_string_field(&body, "id")?;
            let include_images = body
                .get("includeImages")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let limit = bounded_usize_field(
                &body,
                "limit",
                100,
                1,
                if include_images {
                    500
                } else {
                    i32::MAX as usize
                },
            )?;
            let messages = match string_field(&body, "kind")? {
                "channel" => {
                    teams::read_channel_messages(
                        nonempty_string_field(&body, "teamId")?.to_owned(),
                        id.to_owned(),
                        limit,
                        include_images,
                    )
                    .await?
                }
                "chat" => teams::read_messages(id.to_owned(), limit, include_images).await?,
                other => anyhow::bail!("Unknown conversation kind '{other}'"),
            };
            Ok(Value::Array(
                messages
                    .into_iter()
                    .map(|message| {
                        json!({
                            "id": message.id,
                            "sender": message.sender,
                            "senderId": message.sender_id,
                            "isFromCurrentUser": message.is_from_current_user,
                            "timestamp": message.timestamp,
                            "content": message.content,
                            "quotes": message.quotes.into_iter().map(|quote| json!({
                                "messageId": quote.message_id,
                                "sender": quote.sender,
                                "content": quote.content,
                            })).collect::<Vec<_>>(),
                            "imageUrls": message.image_urls,
                            "images": message.images.into_iter().map(|image| json!({
                                "contentType": image.content_type,
                                "sourceUrl": image.source_url,
                                "dataBase64": image.data_base64,
                            })).collect::<Vec<_>>(),
                            "reactions": message.reactions.into_iter().map(|reaction| json!({
                                "type": reaction.reaction_type,
                                "count": reaction.count,
                                "selected": reaction.selected,
                                "users": reaction.users.into_iter().map(|user| json!({"id": user.id, "name": user.name})).collect::<Vec<_>>(),
                            })).collect::<Vec<_>>(),
                        })
                    })
                    .collect(),
            ))
        }
        ("POST", "/messages/send") => {
            let id = nonempty_string_field(&body, "id")?.to_owned();
            let content = nonempty_string_field(&body, "content")?.to_owned();
            if let Some(team_id) = optional_nonempty_string_field(&body, "teamId")? {
                teams::send_channel_message(team_id.to_owned(), id, content).await?;
            } else {
                teams::send_message(id, content).await?;
            }
            Ok(json!({"ok": true}))
        }
        ("POST", "/messages/image") => {
            let content_type = nonempty_string_field(&body, "contentType")?;
            if !content_type.to_ascii_lowercase().starts_with("image/") {
                anyhow::bail!("Image content type must start with image/");
            }
            let data_base64 = nonempty_string_field(&body, "dataBase64")?;
            if data_base64.len() > 16 * 1024 * 1024 {
                anyhow::bail!("Encoded image is too large");
            }
            teams::send_image_message(
                nonempty_string_field(&body, "id")?.to_owned(),
                optional_nonempty_string_field(&body, "teamId")?.map(ToOwned::to_owned),
                string_field(&body, "caption")?.to_owned(),
                content_type.to_ascii_lowercase(),
                data_base64.to_owned(),
            )
            .await?;
            Ok(json!({"ok": true}))
        }
        ("POST", "/reaction") => {
            let reaction_type = nonempty_string_field(&body, "reactionType")?;
            if !matches!(
                reaction_type,
                "like" | "heart" | "laugh" | "surprised" | "sad" | "angry"
            ) {
                anyhow::bail!("Unsupported reaction type '{reaction_type}'");
            }
            teams::set_reaction(
                nonempty_string_field(&body, "id")?.to_owned(),
                optional_nonempty_string_field(&body, "teamId")?.map(ToOwned::to_owned),
                nonempty_string_field(&body, "messageId")?.to_owned(),
                reaction_type.to_owned(),
                bool_field(&body, "remove")?,
            )
            .await?;
            Ok(json!({"ok": true}))
        }
        ("POST", "/photo/profile") => Ok(json!({
            "dataBase64": teams::get_profile_photo(nonempty_string_field(&body, "userId")?.to_owned()).await?
        })),
        ("POST", "/photo/chat") => Ok(json!({
            "dataBase64": teams::get_chat_photo(nonempty_string_field(&body, "chatId")?.to_owned()).await?
        })),
        ("POST", "/photo/team") => Ok(json!({
            "dataBase64": teams::get_team_photo(nonempty_string_field(&body, "teamId")?.to_owned()).await?
        })),
        ("POST", "/saved-account/notification") => {
            let notification = teams::read_saved_account_notification(
                nonempty_string_field(&body, "accountId")?.to_owned(),
                nonempty_string_field(&body, "conversationId")?.to_owned(),
                bounded_usize_field(&body, "messageLimit", 1, 1, 25)?,
            )
            .await?;
            Ok(json!({
                "notification": notification.map(|notification| {
                    let message = notification.message;
                    json!({
                        "accountId": notification.account_id,
                        "accountName": notification.account_name,
                        "conversationId": notification.conversation_id,
                        "conversationName": notification.conversation_name,
                        "isGroup": notification.is_group,
                        "isChannel": notification.is_channel,
                        "message": {
                            "id": message.id,
                            "sender": message.sender,
                            "senderId": message.sender_id,
                            "isFromCurrentUser": message.is_from_current_user,
                            "timestamp": message.timestamp,
                            "content": message.content,
                            "reactions": message.reactions.into_iter().map(|reaction| json!({
                                "type": reaction.reaction_type,
                                "count": reaction.count,
                                "selected": reaction.selected,
                                "users": reaction.users.into_iter().map(|user| json!({"id": user.id, "name": user.name})).collect::<Vec<_>>(),
                            })).collect::<Vec<_>>(),
                        },
                        "messages": notification.messages.into_iter().map(|message| json!({
                            "id": message.id,
                            "sender": message.sender,
                            "senderId": message.sender_id,
                            "isFromCurrentUser": message.is_from_current_user,
                            "timestamp": message.timestamp,
                            "content": message.content,
                            "reactions": message.reactions.into_iter().map(|reaction| json!({
                                "type": reaction.reaction_type,
                                "count": reaction.count,
                                "selected": reaction.selected,
                                "users": reaction.users.into_iter().map(|user| json!({"id": user.id, "name": user.name})).collect::<Vec<_>>(),
                            })).collect::<Vec<_>>(),
                        })).collect::<Vec<_>>(),
                    })
                }),
            }))
        }
        ("POST", "/events/messages/poll") => {
            let cursor = optional_u64_field(&body, "cursor")?;
            let queue = MESSAGE_EVENTS.lock().await;
            let latest = queue.back().map(|(sequence, _)| *sequence).unwrap_or(0);
            let lost = cursor.is_some_and(|cursor| {
                queue
                    .front()
                    .is_some_and(|(sequence, _)| cursor.saturating_add(1) < *sequence)
            });
            let events = cursor
                .map(|cursor| {
                    queue
                        .iter()
                        .filter(|(sequence, _)| *sequence > cursor)
                        .map(|(_, event)| {
                            json!({
                                "conversationId": event.conversation_id,
                                "accountId": event.account_id,
                                "accountActive": event.account_active,
                                "resourceType": event.resource_type,
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Ok(json!({
                "ready": MESSAGE_PUMP_READY.load(Ordering::SeqCst),
                "cursor": latest,
                "lost": lost,
                "events": events,
            }))
        }
        ("POST", "/events/calls/poll") => {
            let cursor = optional_u64_field(&body, "cursor")?;
            let queue = CALL_EVENTS.lock().await;
            let latest = queue.back().map(|(sequence, _)| *sequence).unwrap_or(0);
            let lost = cursor.is_some_and(|cursor| {
                queue
                    .front()
                    .is_some_and(|(sequence, _)| cursor.saturating_add(1) < *sequence)
            });
            let events = cursor
                .map(|cursor| {
                    queue
                        .iter()
                        .filter(|(sequence, _)| *sequence > cursor)
                        .map(|(_, event)| call_event_json(event))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Ok(json!({"cursor": latest, "lost": lost, "events": events}))
        }
        ("POST", "/call/start") => {
            let conversation_id = nonempty_string_field(&body, "conversationId")?;
            let callee_user_id = body
                .get("calleeUserId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let mut target = conversation_id.to_owned();
            if let Some(callee_user_id) = callee_user_id {
                let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(callee_user_id.as_bytes());
                target = format!("__microslop_callee_{encoded}:{target}");
            }
            if body.get("video").and_then(Value::as_bool).unwrap_or(false) {
                target = format!("__microslop_video_call__{target}");
            }
            calls::start_call(target).await?;
            Ok(json!({"ok": true}))
        }
        ("POST", "/call/hangup") => {
            calls::hang_up().await?;
            Ok(json!({"ok": true}))
        }
        ("POST", "/call/accept") => {
            calls::accept_call(nonempty_string_field(&body, "callId")?.to_owned()).await?;
            Ok(json!({"ok": true}))
        }
        ("POST", "/call/decline") => {
            calls::decline_call(nonempty_string_field(&body, "callId")?.to_owned()).await?;
            Ok(json!({"ok": true}))
        }
        ("POST", "/call/microphone") => {
            let enabled = bool_field(&body, "enabled")?;
            calls::start_call(if enabled {
                "__microslop_call_mic_on__".to_owned()
            } else {
                "__microslop_call_mic_off__".to_owned()
            })
            .await?;
            Ok(json!({"ok": true}))
        }
        ("POST", "/call/speaker") => {
            let enabled = bool_field(&body, "enabled")?;
            calls::start_call(if enabled {
                "__microslop_call_speaker_on__".to_owned()
            } else {
                "__microslop_call_speaker_off__".to_owned()
            })
            .await?;
            Ok(json!({"ok": true}))
        }
        _ => anyhow::bail!("Unknown web bridge route: {method} {path}"),
    }
}

fn call_event_json(event: &calls::CallEvent) -> Value {
    json!({
        "kind": event.kind,
        "callId": event.call_id,
        "conversationId": event.conversation_id,
        "displayName": event.display_name,
        "detail": event.detail,
    })
}

fn string_field<'a>(body: &'a Value, field: &str) -> Result<&'a str> {
    body.get(field)
        .and_then(Value::as_str)
        .with_context(|| format!("Missing string field '{field}'"))
}

fn nonempty_string_field<'a>(body: &'a Value, field: &str) -> Result<&'a str> {
    let value = string_field(body, field)?.trim();
    if value.is_empty() {
        anyhow::bail!("String field '{field}' must not be empty");
    }
    Ok(value)
}

fn optional_nonempty_string_field<'a>(body: &'a Value, field: &str) -> Result<Option<&'a str>> {
    let Some(value) = body.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let value = value
        .as_str()
        .with_context(|| format!("Field '{field}' must be a string or null"))?
        .trim();
    if value.is_empty() {
        return Ok(None);
    }
    Ok(Some(value))
}

fn bounded_usize_field(
    body: &Value,
    field: &str,
    default: usize,
    min: usize,
    max: usize,
) -> Result<usize> {
    let Some(value) = body.get(field) else {
        return Ok(default.clamp(min, max));
    };
    let value = value
        .as_u64()
        .with_context(|| format!("Field '{field}' must be a non-negative integer"))?;
    let value = usize::try_from(value).context("Numeric field was too large")?;
    Ok(value.clamp(min, max))
}

fn bool_field(body: &Value, field: &str) -> Result<bool> {
    body.get(field)
        .and_then(Value::as_bool)
        .with_context(|| format!("Missing boolean field '{field}'"))
}

fn optional_u64_field(body: &Value, field: &str) -> Result<Option<u64>> {
    let Some(value) = body.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_u64()
        .map(Some)
        .with_context(|| format!("Field '{field}' must be a non-negative integer or null"))
}

async fn write_response(
    stream: &mut TcpStream,
    status: u16,
    body: &str,
    allowed_origin: &str,
) -> Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        _ => "Internal Server Error",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: {allowed_origin}\r\nVary: Origin\r\nAccess-Control-Allow-Headers: content-type\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(())
}

fn is_known_post_path(path: &str) -> bool {
    matches!(
        path,
        "/auth/status"
            | "/auth/restore"
            | "/auth/begin"
            | "/auth/complete"
            | "/auth/cancel"
            | "/auth/logout"
            | "/auth/accounts"
            | "/auth/active-account"
            | "/auth/switch"
            | "/user"
            | "/chats"
            | "/teams"
            | "/messages/read"
            | "/messages/send"
            | "/messages/image"
            | "/reaction"
            | "/photo/profile"
            | "/photo/chat"
            | "/photo/team"
            | "/saved-account/notification"
            | "/events/messages/poll"
            | "/events/calls/poll"
            | "/call/start"
            | "/call/hangup"
            | "/call/accept"
            | "/call/decline"
            | "/call/microphone"
            | "/call/speaker"
    )
}

fn header_value<'a>(headers: &'a str, target: &str) -> Option<&'a str> {
    headers
        .split("\r\n")
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case(target))
        .map(|(_, value)| value.trim())
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn message_polling_is_non_destructive_per_cursor() {
        let mut queue = MESSAGE_EVENTS.lock().await;
        queue.clear();
        queue.push_back((
            1,
            trouter::MessageEvent {
                conversation_id: "chat-one".to_owned(),
                account_id: Some("account-secondary".to_owned()),
                account_active: false,
                resource_type: "MessageUpdate".to_owned(),
            },
        ));
        drop(queue);

        let first = route("POST", "/events/messages/poll", json!({"cursor": 0}))
            .await
            .unwrap();
        let second = route("POST", "/events/messages/poll", json!({"cursor": 0}))
            .await
            .unwrap();

        assert_eq!(first["events"], second["events"]);
        assert_eq!(first["events"][0]["conversationId"], "chat-one");
        assert_eq!(first["events"][0]["accountId"], "account-secondary");
        assert_eq!(first["events"][0]["accountActive"], false);
        assert_eq!(first["events"][0]["resourceType"], "MessageUpdate");
        assert_eq!(first["cursor"], 1);
    }

    #[tokio::test]
    async fn message_pump_restart_signals_current_listener() {
        let (stop, mut receiver) = watch::channel(false);
        *MESSAGE_PUMP_STOP.lock().await = Some(stop);

        restart_message_pump().await;
        receiver.changed().await.unwrap();

        assert!(*receiver.borrow());
        assert!(MESSAGE_PUMP_STOP.lock().await.is_none());
    }

    #[test]
    fn multi_account_bridge_paths_are_known() {
        assert!(is_known_post_path("/auth/accounts"));
        assert!(is_known_post_path("/auth/active-account"));
        assert!(is_known_post_path("/auth/switch"));
        assert!(is_known_post_path("/saved-account/notification"));
    }

    #[tokio::test]
    async fn first_poll_subscribes_at_the_current_tail() {
        let mut queue = CALL_EVENTS.lock().await;
        queue.clear();
        queue.push_back((
            7,
            calls::CallEvent {
                kind: "ringing".to_owned(),
                call_id: "call-one".to_owned(),
                conversation_id: Some("chat-one".to_owned()),
                display_name: None,
                detail: None,
            },
        ));
        drop(queue);

        let initial = route("POST", "/events/calls/poll", json!({}))
            .await
            .unwrap();
        assert_eq!(initial["cursor"], 7);
        assert_eq!(initial["events"], json!([]));

        let mut queue = CALL_EVENTS.lock().await;
        queue.push_back((
            8,
            calls::CallEvent {
                kind: "connected".to_owned(),
                call_id: "call-one".to_owned(),
                conversation_id: Some("chat-one".to_owned()),
                display_name: None,
                detail: None,
            },
        ));
        drop(queue);

        let next = route("POST", "/events/calls/poll", json!({"cursor": 7}))
            .await
            .unwrap();
        assert_eq!(next["cursor"], 8);
        assert_eq!(next["events"][0]["kind"], "connected");
    }
}
