use super::*;

pub(super) async fn wait_for_retry_or_stop(stop: &watch::Receiver<bool>, delay: Duration) -> bool {
    if *stop.borrow() {
        return false;
    }
    let mut stop = stop.clone();
    tokio::select! {
        _ = tokio::time::sleep(delay) => !*stop.borrow(),
        _ = stop.changed() => false,
    }
}

pub(super) async fn build_call_config() -> Result<teams_cli::config::Config> {
    let credentials = auth::service().call_auth().await?;
    let mut config = teams_cli::config::Config {
        access_token: Some(teams_cli::auth::StoredToken::new(
            credentials.teams_access_token,
            None,
        )),
        tenant_id: Some(credentials.tenant_id),
        ..Default::default()
    };
    config.set_skype_token(credentials.skype_token, None);
    config.set_ic3_token(credentials.ic3_token, None);
    if let Some(graph_token) = credentials.graph_token {
        config.set_graph_token(graph_token, None);
    }
    config.set_region_gtms(credentials.region_gtms);
    Ok(config)
}

pub(super) async fn ensure_incoming_listener() -> Result<()> {
    if INCOMING_STOP.lock().await.is_some() {
        return Ok(());
    }
    let (stop_sender, stop_receiver) = watch::channel(false);
    *INCOMING_STOP.lock().await = Some(stop_sender);
    tokio::spawn(async move {
        incoming_listener(stop_receiver).await;
    });
    Ok(())
}

async fn incoming_listener(mut stop: watch::Receiver<bool>) {
    let mut backoff = Duration::from_secs(1);
    loop {
        if *stop.borrow() {
            return;
        }
        let connected_at = Instant::now();
        match incoming_listener_once(&mut stop).await {
            Ok(()) => return,
            Err(error) => {
                if connected_at.elapsed() >= Duration::from_secs(30) {
                    backoff = Duration::from_secs(1);
                }
                tracing::warn!("Incoming call listener disconnected: {error:#}");
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {
                        backoff = (backoff * 2).min(Duration::from_secs(60));
                    }
                    _ = stop.changed() => return,
                }
            }
        }
    }
}

async fn incoming_listener_once(stop: &mut watch::Receiver<bool>) -> Result<()> {
    let credentials = auth::service().call_auth().await?;
    let http = reqwest::Client::new();
    let (session, endpoint_id) =
        teams_cli::trouter::session::negotiate(&http, &credentials.skype_token).await?;
    let session_id = teams_cli::trouter::session::get_session_id(
        &http,
        &session,
        &credentials.skype_token,
        &endpoint_id,
    )
    .await?;
    let mut socket =
        teams_cli::trouter::websocket::TrouterSocket::connect(&session, &session_id, &endpoint_id)
            .await?;
    let handshake = socket
        .recv_frame()
        .await?
        .context("Trouter closed before the incoming-call handshake")?;
    if !handshake.starts_with("1::") {
        tracing::warn!("Unexpected incoming-call Trouter handshake: {handshake}");
    }
    if let Some(registrar_url) = session.registrar_url.as_deref() {
        teams_cli::trouter::registrar::register(
            &http,
            &credentials.skype_token,
            registrar_url,
            &session.surl,
        )
        .await?;
    }

    let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
    heartbeat.tick().await;
    loop {
        tokio::select! {
            _ = stop.changed() => return Ok(()),
            _ = heartbeat.tick() => socket.send_text("2::").await?,
            frame = socket.recv_frame() => {
                let Some(frame) = frame? else {
                    anyhow::bail!("Incoming-call Trouter socket closed");
                };
                let Some(json_start) = frame.find('{') else {
                    continue;
                };
                let Some(notification) = microsoft_calling::parse_call_event(&frame[json_start..]) else {
                    continue;
                };
                register_incoming_call(notification).await;
            }
        }
    }
}

pub(super) fn notification_call_id(notification: &CallNotification) -> String {
    let debug = notification.debug_content.as_ref();
    debug
        .and_then(|debug| debug.call_id.as_deref())
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            debug
                .and_then(|debug| debug.operation_id.as_deref())
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(|id| format!("operation:{id}"))
        })
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}

pub(super) async fn clear_pending_call(call_id: &str) {
    let mut pending = PENDING_INCOMING.lock().await;
    if pending
        .as_ref()
        .is_some_and(|pending| pending.call_id == call_id)
    {
        *pending = None;
    }
}

fn decline_incoming_in_background(notification: CallNotification) {
    tokio::spawn(async move {
        let result = async {
            let call_auth = auth::service().call_auth().await?;
            calling::signaling::end_call(
                &reqwest::Client::new(),
                &call_auth.skype_token,
                &notification,
            )
            .await
        }
        .await;
        if let Err(error) = result {
            tracing::warn!("Could not decline ignored incoming call: {error:#}");
        }
    });
}

pub(super) async fn register_incoming_call(notification: CallNotification) {
    let call_id = notification_call_id(&notification);
    if let Some(ended) = notification
        .call_end
        .as_ref()
        .or(notification.conversation_end.as_ref())
    {
        let Ok(_accept_permit) = ACCEPT_INCOMING_GATE.acquire().await else {
            return;
        };
        let mut pending = PENDING_INCOMING.lock().await;
        let mut active = ACTIVE_INCOMING.lock().await;
        let pending_matches = pending.as_ref().is_some_and(|call| call.call_id == call_id);
        let active_matches = active.as_ref().is_some_and(|call| call.call_id == call_id);
        if !pending_matches && !active_matches {
            return;
        }
        if pending_matches {
            *pending = None;
        }
        let call = if active_matches { active.take() } else { None };
        drop(active);
        drop(pending);
        if let Some(mut call) = call {
            call.media.stop().await;
            if let Some(video) = call.video.as_mut() {
                video.stop().await;
                calling::external_display::clear();
            }
        }
        emit(CallEvent {
            kind: "ended".to_owned(),
            call_id,
            conversation_id: None,
            display_name: None,
            detail: Some(
                ended
                    .phrase
                    .clone()
                    .unwrap_or_else(|| "Remote call ended".to_owned()),
            ),
        });
        return;
    }
    let display_name = notification
        .participants
        .as_ref()
        .and_then(|participants| participants.from.as_ref())
        .and_then(|participant| participant.display_name.as_deref())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("Incoming Teams call")
        .to_owned();
    let conversation_id = notification
        .group_chat
        .as_ref()
        .and_then(|group| group.thread_id.as_deref())
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned);
    let caller_id = notification
        .participants
        .as_ref()
        .and_then(|participants| participants.from.as_ref())
        .and_then(|participant| participant.id.as_deref())
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned);
    let video = notification
        .call_invitation
        .as_ref()
        .and_then(|invitation| invitation.call_modalities.as_ref())
        .is_some_and(|modalities| {
            modalities
                .iter()
                .any(|modality| modality.eq_ignore_ascii_case("video"))
        });
    let expired = {
        let mut pending = PENDING_INCOMING.lock().await;
        match pending.as_ref() {
            Some(existing) if existing.call_id == call_id => return,
            Some(existing) if existing.received_at.elapsed() < PENDING_CALL_TTL => {
                drop(pending);
                decline_incoming_in_background(notification);
                return;
            }
            Some(_) => pending.take(),
            None => None,
        }
    };
    if let Some(expired) = expired {
        decline_incoming_in_background(expired.notification);
        emit(CallEvent {
            kind: "ended".to_owned(),
            call_id: expired.call_id,
            conversation_id: None,
            display_name: None,
            detail: Some("expired".to_owned()),
        });
    }

    if ACCEPT_INCOMING_GATE.available_permits() == 0
        || OUTGOING_STOP.lock().await.is_some()
        || ACTIVE_INCOMING.lock().await.is_some()
    {
        decline_incoming_in_background(notification);
        return;
    }

    let mut pending = PENDING_INCOMING.lock().await;
    if pending.is_some() {
        drop(pending);
        decline_incoming_in_background(notification);
        return;
    }
    *pending = Some(PendingIncomingCall {
        call_id: call_id.clone(),
        notification,
        received_at: Instant::now(),
    });
    drop(pending);
    emit(CallEvent {
        kind: "incoming".to_owned(),
        call_id,
        conversation_id: conversation_id.or(caller_id),
        display_name: Some(display_name),
        detail: video.then_some("video=true".to_owned()),
    });
}

pub(crate) async fn subscribe_events_for_web() -> Result<broadcast::Receiver<CallEvent>> {
    ensure_incoming_listener().await?;
    Ok(CALL_EVENTS.subscribe())
}

pub(super) fn emit(event: CallEvent) {
    let _ = CALL_EVENTS.send(event);
}
