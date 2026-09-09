use std::sync::Arc;

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use ost_core::auth::AuthService;
use tokio::sync::{mpsc, watch, Mutex};

use super::auth;
use crate::frb_generated::StreamSink;

#[derive(Clone)]
pub struct MessageEvent {
    pub conversation_id: String,
    pub account_id: Option<String>,
    pub account_active: bool,
    pub resource_type: String,
}

static STOP_MESSAGE_WATCH: Lazy<Mutex<Option<watch::Sender<bool>>>> =
    Lazy::new(|| Mutex::new(None));

pub(crate) async fn listen_message_events_to(
    sender: mpsc::Sender<MessageEvent>,
    receiver: watch::Receiver<bool>,
) -> Result<()> {
    let active_account_id = auth::service().active_account_id().await?;
    let saved_accounts = auth::service().saved_accounts().await?;
    let (account_sender, mut account_events) =
        mpsc::channel::<(Option<String>, bool, ost_core::trouter::MessageEvent)>(64);
    let mut listeners = Vec::new();

    if saved_accounts.is_empty() {
        let event_sender = account_sender.clone();
        let service = Arc::clone(auth::service());
        let stop_receiver = receiver.clone();
        listeners.push(tokio::spawn(async move {
            let (sender, mut events) = mpsc::channel(32);
            let forward = tokio::spawn(async move {
                while let Some(event) = events.recv().await {
                    if event_sender.send((None, true, event)).await.is_err() {
                        break;
                    }
                }
            });
            let result = ost_core::trouter::TrouterMessageService::new(&service)
                .listen(sender, stop_receiver)
                .await;
            let _ = forward.await;
            result
        }));
    } else {
        for account in saved_accounts {
            let account_id = account.id.clone();
            let account_active = active_account_id.as_deref() == Some(account_id.as_str());
            let service = if account_active {
                Arc::clone(auth::service())
            } else {
                let service = Arc::new(AuthService::ephemeral());
                if let Err(error) = service.restore_saved_account(&account_id).await {
                    tracing::warn!(
                        "Could not start notifications for saved account {}: {error:#}",
                        account.display_name
                    );
                    continue;
                }
                service
            };
            let event_sender = account_sender.clone();
            let stop_receiver = receiver.clone();
            listeners.push(tokio::spawn(async move {
                let (sender, mut events) = mpsc::channel(32);
                let forward_account_id = account_id.clone();
                let forward = tokio::spawn(async move {
                    while let Some(event) = events.recv().await {
                        if event_sender
                            .send((Some(forward_account_id.clone()), account_active, event))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                });
                let result = ost_core::trouter::TrouterMessageService::new(&service)
                    .listen(sender, stop_receiver)
                    .await;
                let _ = forward.await;
                result
            }));
        }
    }
    drop(account_sender);

    while let Some((account_id, account_active, event)) = account_events.recv().await {
        if sender
            .send(MessageEvent {
                conversation_id: event.conversation_id,
                account_id,
                account_active,
                resource_type: event.resource_type,
            })
            .await
            .is_err()
        {
            break;
        }
    }

    for listener in listeners {
        match listener.await.context("Trouter listener task failed")? {
            Ok(()) => {}
            Err(error) => tracing::warn!("Trouter account listener stopped: {error:#}"),
        }
    }
    Ok(())
}

pub async fn listen_message_events(sink: StreamSink<MessageEvent>) -> Result<()> {
    let (stop, receiver) = watch::channel(false);
    if let Some(previous) = STOP_MESSAGE_WATCH.lock().await.replace(stop.clone()) {
        let _ = previous.send(true);
    }

    let (sender, mut events) = mpsc::channel(64);
    let listener = tokio::spawn(listen_message_events_to(sender, receiver));
    while let Some(event) = events.recv().await {
        if sink.add(event).is_err() {
            let _ = stop.send(true);
            break;
        }
    }

    let _ = stop.send(true);
    match listener.await.context("Trouter listener task failed")? {
        Ok(()) => {}
        Err(error) => tracing::warn!("Trouter account listener stopped: {error:#}"),
    }
    Ok(())
}

pub async fn stop_message_events() {
    if let Some(stop) = STOP_MESSAGE_WATCH.lock().await.take() {
        let _ = stop.send(true);
    }
}
