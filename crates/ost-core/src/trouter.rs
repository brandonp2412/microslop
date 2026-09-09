use std::{
    io::Read,
    sync::OnceLock,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use flate2::read::GzDecoder;
use futures::{SinkExt, StreamExt};
use ost_microsoft::calling as microsoft_calling;
use tokio::{
    sync::{mpsc, watch},
    time,
};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::auth::AuthService;

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn http_client() -> reqwest::Client {
    HTTP_CLIENT.get_or_init(reqwest::Client::new).clone()
}

#[derive(Clone, Debug)]
pub struct MessageEvent {
    pub conversation_id: String,
    pub resource_type: String,
}

pub struct TrouterMessageService<'a> {
    auth: &'a AuthService,
    http: reqwest::Client,
}

impl<'a> TrouterMessageService<'a> {
    pub fn new(auth: &'a AuthService) -> Self {
        Self {
            auth,
            http: http_client(),
        }
    }

    pub async fn listen(
        &self,
        events: mpsc::Sender<MessageEvent>,
        mut stop: watch::Receiver<bool>,
    ) -> Result<()> {
        let mut backoff = Duration::from_secs(1);

        loop {
            if *stop.borrow() {
                return Ok(());
            }

            let connected_at = Instant::now();
            match self.listen_once(&events, &mut stop).await {
                Ok(()) => return Ok(()),
                Err(error) => {
                    if connected_at.elapsed() >= Duration::from_secs(30) {
                        backoff = Duration::from_secs(1);
                    }
                    tracing::warn!("Trouter message listener disconnected: {error:#}");
                    tokio::select! {
                        _ = time::sleep(backoff) => backoff = (backoff * 2).min(Duration::from_secs(64)),
                        _ = stop.changed() => return Ok(()),
                    }
                }
            }
        }
    }

    async fn listen_once(
        &self,
        events: &mpsc::Sender<MessageEvent>,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<()> {
        let token = self.auth.skype_token().await?;
        let access_token = self.auth.teams_access_token().await?;
        let endpoint_id = uuid::Uuid::new_v4().to_string();
        let session = self.negotiate(&token, &endpoint_id).await?;
        let session_id = self.session_id(&session, &token, &endpoint_id).await?;
        let websocket_url = session.websocket_url(&session_id, &endpoint_id);
        let (mut socket, _) = connect_async(websocket_url)
            .await
            .context("Could not connect to Trouter")?;

        match socket.next().await {
            Some(Ok(Message::Text(frame))) if frame.starts_with("1::") => {}
            Some(Ok(frame)) => anyhow::bail!("Unexpected Trouter handshake frame: {frame:?}"),
            Some(Err(error)) => {
                return Err(error).context("Could not receive the Trouter handshake");
            }
            None => anyhow::bail!("Trouter closed before the handshake"),
        }
        let authentication =
            microsoft_calling::authentication_payload(&access_token, &session.connectparams);
        socket
            .send(Message::Text(format!("5:::{authentication}")))
            .await
            .context("Could not authenticate the Trouter session")?;
        socket
            .send(Message::Text(microsoft_calling::ACTIVE_FRAME.to_owned()))
            .await
            .context("Could not mark the Trouter session active")?;
        self.register(&session, &token, &endpoint_id).await?;
        if events
            .send(MessageEvent {
                conversation_id: String::new(),
                resource_type: String::new(),
            })
            .await
            .is_err()
        {
            return Ok(());
        }

        let mut registration_refresh = time::interval(Duration::from_secs(
            microsoft_calling::MESSAGE_REGISTRATION_TTL_SECONDS - 30,
        ));
        registration_refresh.tick().await;

        loop {
            tokio::select! {
                _ = stop.changed() => return Ok(()),
                _ = registration_refresh.tick() => self.register(&session, &token, &endpoint_id).await?,
                frame = time::timeout(Duration::from_secs(30), socket.next()) => match frame {
                    Ok(Some(Ok(Message::Text(frame)))) => {
                        if let Some(event) = parse_message_event(&frame)
                            && events.send(event).await.is_err()
                        {
                            return Ok(());
                        }
                        acknowledge(&mut socket, &frame).await?;
                        acknowledge_socketio_event(&mut socket, &frame).await?;
                    }
                    Ok(Some(Ok(Message::Ping(payload)))) => socket.send(Message::Pong(payload)).await.context("Could not respond to Trouter ping")?,
                    Ok(Some(Ok(Message::Close(_)))) | Ok(None) => anyhow::bail!("Trouter closed the WebSocket"),
                    Ok(Some(Err(error))) => return Err(error).context("Could not receive a Trouter frame"),
                    Ok(Some(Ok(_))) => {},
                    Err(_) => socket.send(Message::Text("2::".to_owned())).await.context("Could not send Trouter heartbeat")?,
                },
            }
        }
    }

    async fn negotiate(&self, token: &str, endpoint_id: &str) -> Result<Session> {
        self.http
            .get(microsoft_calling::trouter_negotiation_url(endpoint_id))
            .header(microsoft_calling::SKYPE_TOKEN_HEADER, token)
            .send()
            .await
            .context("Could not negotiate a Trouter session")?
            .error_for_status()
            .context("Trouter rejected session negotiation")?
            .json()
            .await
            .context("Could not parse the Trouter session")
    }

    async fn session_id(
        &self,
        session: &Session,
        token: &str,
        endpoint_id: &str,
    ) -> Result<String> {
        let response = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?
            .get(session.session_url(endpoint_id))
            .header(microsoft_calling::SKYPE_TOKEN_HEADER, token)
            .send()
            .await
            .context("Could not request a Trouter socket session")?
            .error_for_status()
            .context("Trouter rejected the socket session")?
            .text()
            .await?;
        response
            .split(':')
            .next()
            .map(ToOwned::to_owned)
            .context("Trouter returned an empty socket session")
    }

    async fn register(&self, session: &Session, token: &str, endpoint_id: &str) -> Result<()> {
        let url = session
            .registrar_url
            .as_deref()
            .unwrap_or(microsoft_calling::DEFAULT_REGISTRAR_URL);
        self.http
            .post(url)
            .header(microsoft_calling::SKYPE_TOKEN_HEADER, token)
            .json(&microsoft_calling::message_registration_payload(
                endpoint_id,
                &session.surl,
            ))
            .send()
            .await
            .context("Could not register for Trouter messages")?
            .error_for_status()
            .context("Trouter rejected the message registration")?;
        Ok(())
    }
}

type Session = microsoft_calling::TrouterSession;

async fn acknowledge<S>(socket: &mut S, frame: &str) -> Result<()>
where
    S: futures::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    if let Some(id) = frame
        .strip_prefix("3:::")
        .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok())
        .and_then(|body| body.get("id").and_then(serde_json::Value::as_i64))
    {
        socket
            .send(Message::Text(format!(r#"3:::{{"id":{id},"status":200}}"#)))
            .await
            .context("Could not acknowledge Trouter delivery")?;
    }
    Ok(())
}

async fn acknowledge_socketio_event<S>(socket: &mut S, frame: &str) -> Result<()>
where
    S: futures::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    if let Some(acknowledgement) = microsoft_calling::socketio_acknowledgement(frame) {
        socket
            .send(Message::Text(acknowledgement))
            .await
            .context("Could not acknowledge Trouter Socket.IO event")?;
    }
    Ok(())
}

pub fn parse_message_event(frame: &str) -> Option<MessageEvent> {
    if frame
        .strip_prefix("5:")
        .and_then(|value| value.split_once("::").map(|(_, body)| body))
        .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok())
        .is_some_and(|event| {
            event.get("name").and_then(serde_json::Value::as_str) == Some("trouter.message_loss")
        })
    {
        return Some(MessageEvent {
            conversation_id: String::new(),
            resource_type: "MessageLoss".to_owned(),
        });
    }
    let request = serde_json::from_str::<serde_json::Value>(frame.strip_prefix("3:::")?).ok()?;
    if !request.get("url")?.as_str()?.ends_with("/messaging") {
        return None;
    }
    let body = request.get("body")?.as_str()?;
    let body = if request
        .pointer("/headers/X-Microsoft-Skype-Content-Encoding")
        .and_then(serde_json::Value::as_str)
        == Some("gzip")
    {
        let mut decoded = String::new();
        GzDecoder::new(STANDARD.decode(body).ok()?.as_slice())
            .read_to_string(&mut decoded)
            .ok()?;
        decoded
    } else {
        body.to_owned()
    };
    let event = serde_json::from_str::<serde_json::Value>(&body).ok()?;
    let resource_type = event.get("resourceType")?.as_str()?;
    if event.get("type")?.as_str()? != "EventMessage"
        || !matches!(resource_type, "NewMessage" | "MessageUpdate")
    {
        return None;
    }
    let link = event.pointer("/resource/conversationLink")?.as_str()?;
    let id = link.split("/conversations/").nth(1)?.split('/').next()?;
    let conversation_id = url::form_urlencoded::parse(format!("id={id}").as_bytes())
        .next()?
        .1
        .into_owned();
    Some(MessageEvent {
        conversation_id,
        resource_type: resource_type.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::parse_message_event;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;

    #[test]
    fn parses_a_message_notification_for_its_conversation() {
        let frame = r#"3:::{"id":1,"url":"/v4/f/token/messaging","body":"{\"type\":\"EventMessage\",\"resourceType\":\"NewMessage\",\"resource\":{\"conversationLink\":\"https://example.test/v1/users/ME/conversations/19%3Achat%40thread.skype/messages\"}}"}"#;
        assert_eq!(
            parse_message_event(frame).unwrap().conversation_id,
            "19:chat@thread.skype"
        );
    }

    #[test]
    fn parses_a_message_loss_signal() {
        assert_eq!(
            parse_message_event(r#"5:8+::{"name":"trouter.message_loss"}"#)
                .unwrap()
                .conversation_id,
            ""
        );
    }

    #[test]
    fn parses_a_gzip_compressed_message_notification() {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(br#"{"type":"EventMessage","resourceType":"MessageUpdate","resource":{"conversationLink":"https://example.test/v1/users/ME/conversations/19%3Achat%40thread.skype/messages"}}"#).unwrap();
        let frame = serde_json::json!({
            "id": 1,
            "url": "/v4/f/token/messaging",
            "headers": { "X-Microsoft-Skype-Content-Encoding": "gzip" },
            "body": STANDARD.encode(encoder.finish().unwrap()),
        });

        assert_eq!(
            parse_message_event(&format!("3:::{frame}"))
                .unwrap()
                .conversation_id,
            "19:chat@thread.skype"
        );
    }
}
