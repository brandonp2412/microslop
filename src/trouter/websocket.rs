//! Trouter WebSocket connection and frame handling

use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use ost_microsoft::calling as microsoft_calling;
use tokio::{time, time::Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use super::session::SessionResponse;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub struct TrouterSocket {
    stream: WsStream,
    heartbeat: time::Interval,
}

impl TrouterSocket {
    /// Connect to the Trouter WebSocket endpoint.
    ///
    /// Auth is handled by the session ID in the URL (obtained via authenticated GET).
    /// No auth headers or messages needed on the WebSocket itself.
    pub async fn connect(session: &SessionResponse, session_id: &str, epid: &str) -> Result<Self> {
        let ws_url = session.ws_url(session_id, epid);
        let ws_url = ws_url
            .replace("https://", "wss://")
            .replace("http://", "ws://");

        tracing::info!("Connecting WebSocket to {}", ws_url);

        let (stream, response) = connect_async(&ws_url)
            .await
            .context("WebSocket connection failed")?;

        tracing::info!("WebSocket connected (status={})", response.status());

        Ok(Self {
            stream,
            heartbeat: time::interval_at(
                time::Instant::now() + Duration::from_secs(30),
                Duration::from_secs(30),
            ),
        })
    }

    pub async fn send_text(&mut self, msg: &str) -> Result<()> {
        tracing::debug!("WS send: {}", msg);
        self.stream
            .send(Message::Text(msg.to_string()))
            .await
            .context("Failed to send WebSocket message")
    }

    pub async fn authenticate(
        &mut self,
        access_token: &str,
        session: &SessionResponse,
    ) -> Result<()> {
        let authentication =
            microsoft_calling::authentication_payload(access_token, &session.connectparams);
        self.send_text(&format!("5:::{authentication}")).await?;
        self.send_text(microsoft_calling::ACTIVE_FRAME).await
    }

    ///
    /// Automatically sends HTTP 200 responses for Trouter data frame deliveries.
    /// Trouter uses HTTP-over-WebSocket: each `3:::` data frame contains an `"id"` field.
    /// The client MUST respond with `3:::{"id":N,"status":200}` to confirm delivery.
    /// Without this, Trouter returns 504 to the sender (e.g. Call Controller),
    /// which kills calls with error 430/10065.
    pub async fn recv_frame(&mut self) -> Result<Option<String>> {
        loop {
            let next = tokio::select! {
                frame = self.stream.next() => frame,
                _ = self.heartbeat.tick() => {
                    self.stream
                        .send(Message::Text("2::".to_string()))
                        .await
                        .context("Heartbeat send failed")?;
                    continue;
                }
            };
            match next {
                Some(Ok(Message::Text(text))) => {
                    tracing::debug!("WS recv: {}", text);

                    if let Some(req_id) = extract_trouter_request_id(&text) {
                        let resp = format!("3:::{{\"id\":{},\"status\":200}}", req_id);
                        tracing::debug!("Trouter response: {}", resp);
                        if let Err(e) = self.stream.send(Message::Text(resp)).await {
                            tracing::warn!("Failed to send Trouter response: {:#}", e);
                        }
                    }

                    // Without acks, the server retries indefinitely and blocks new events.
                    if let Some(ack) = microsoft_calling::socketio_acknowledgement(&text) {
                        tracing::debug!("Socket.IO ack: {}", ack);
                        if let Err(e) = self.stream.send(Message::Text(ack)).await {
                            tracing::warn!("Failed to send Socket.IO ack: {:#}", e);
                        }
                    }

                    return Ok(Some(text));
                }
                Some(Ok(Message::Ping(data))) => {
                    self.stream
                        .send(Message::Pong(data))
                        .await
                        .context("Failed to send pong")?;
                }
                Some(Ok(Message::Close(frame))) => {
                    tracing::info!("WebSocket closed: {:?}", frame);
                    return Ok(None);
                }
                Some(Ok(other)) => {
                    tracing::debug!("WS frame (ignored): {:?}", other);
                }
                Some(Err(e)) => {
                    return Err(e).context("WebSocket receive error");
                }
                None => {
                    return Ok(None);
                }
            }
        }
    }
}

/// Extract the Trouter request ID from a `3:::` data frame.
///
/// Trouter HTTP-over-WS data frames (`3:::`) contain `"id":NNN` at the JSON top level.
/// Only `3:::` frames use this mechanism; `5:` event frames use Socket.IO acks instead.
fn extract_trouter_request_id(frame: &str) -> Option<i64> {
    let json_str = frame.strip_prefix("3:::")?;
    let v: serde_json::Value = serde_json::from_str(json_str).ok()?;
    v.get("id").and_then(|id| id.as_i64())
}
