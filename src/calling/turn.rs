//! TURN relay client — credential acquisition from FlightProxy and RFC 5766 TURN Allocate.
//!
//! This module provides:
//! 1. `acquire_relay_credentials()` — fetch TURN server credentials from FlightProxy
//! 2. `TurnClient` — send TURN Allocate, CreatePermission, Send/Data indications
//!
//! If credential acquisition fails (auth unknown, network error), callers should
//! fall back to direct/srflx ICE candidates only.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use hmac::{Hmac, Mac};
use ost_microsoft::calling as microsoft_calling;
use sha1::Sha1;
use tokio::net::UdpSocket;

use super::ice::{self, CandidateType, IceCandidate, Transport};

mod wire;

use wire::{
    add_message_integrity_and_fingerprint, append_attr, append_auth_attrs, build_allocate_request,
    build_stun_header, compute_long_term_key, compute_relay_priority, encode_xor_address,
    parse_data_indication, parse_turn_response, TurnResponse,
};
#[cfg(test)]
use wire::{crc32, decode_xor_address, iter_attrs};
pub use wire::{is_channel_data, is_turn_data_message};

type HmacSha1 = Hmac<Sha1>;

/// STUN/TURN magic cookie.
const MAGIC_COOKIE: u32 = 0x2112A442;
const STUN_HEADER_SIZE: usize = 20;

/// TURN message types (RFC 5766).
const ALLOCATE_REQUEST: u16 = 0x0003;
const ALLOCATE_RESPONSE: u16 = 0x0103;
const ALLOCATE_ERROR_RESPONSE: u16 = 0x0113;
const CREATE_PERMISSION_REQUEST: u16 = 0x0008;
const CREATE_PERMISSION_RESPONSE: u16 = 0x0108;
const SEND_INDICATION: u16 = 0x0016;
const DATA_INDICATION: u16 = 0x0017;
const CHANNEL_BIND_REQUEST: u16 = 0x0009;
const CHANNEL_BIND_RESPONSE: u16 = 0x0109;

/// TURN/STUN attribute types.
const ATTR_USERNAME: u16 = 0x0006;
const ATTR_MESSAGE_INTEGRITY: u16 = 0x0008;
const ATTR_ERROR_CODE: u16 = 0x0009;
const ATTR_FINGERPRINT: u16 = 0x8028;
const ATTR_REALM: u16 = 0x0014;
const ATTR_NONCE: u16 = 0x0015;
const ATTR_XOR_RELAYED_ADDRESS: u16 = 0x0016;
const ATTR_REQUESTED_TRANSPORT: u16 = 0x0019;
const ATTR_XOR_PEER_ADDRESS: u16 = 0x0012;
const ATTR_DATA: u16 = 0x0013;
const ATTR_LIFETIME: u16 = 0x000D;
const ATTR_CHANNEL_NUMBER: u16 = 0x000C;

const FINGERPRINT_XOR: u32 = 0x5354554e;

/// Transport protocol number for UDP.
const TRANSPORT_UDP: u8 = 17;

/// Default TURN allocation timeout.
const ALLOCATE_TIMEOUT: Duration = Duration::from_secs(3);

/// A single TURN server entry.
#[derive(Debug, Clone)]
pub struct TurnServer {
    pub host: String,
    pub port: u16,
    pub transport: TurnTransport,
}

/// Transport for a TURN server.
#[derive(Debug, Clone, PartialEq)]
pub enum TurnTransport {
    Udp,
    Tcp,
    Tls,
}

#[derive(Debug, Clone)]
pub struct RelayConfig {
    pub servers: Vec<TurnServer>,
    pub username: String,
    pub credential: String,
    pub ttl: u32,
}

/// Attempt to acquire TURN relay credentials from FlightProxy.
///
/// Tries multiple auth approaches since the exact FlightProxy relay token API
/// is not fully documented. On failure, returns an error — callers should
/// log and continue without relay candidates.
pub async fn acquire_relay_credentials(
    http: &reqwest::Client,
    skype_token: &str,
) -> Result<RelayConfig> {
    // Try the relay token endpoint with X-Skypetoken header
    let resp = http
        .post(microsoft_calling::FLIGHTPROXY_RELAY_URL)
        .header(microsoft_calling::SKYPE_TOKEN_HEADER, skype_token)
        .header("Content-Type", "application/json")
        .body("{}")
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .context("FlightProxy relay token request failed")?;

    let status = resp.status();
    if !status.is_success() {
        let resp2 = http
            .post(microsoft_calling::FLIGHTPROXY_RELAY_URL)
            .header("Authorization", format!("Bearer {}", skype_token))
            .header("Content-Type", "application/json")
            .body("{}")
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .context("FlightProxy relay token request (Bearer) failed")?;

        let status2 = resp2.status();
        if !status2.is_success() {
            let body = resp2.text().await.unwrap_or_default();
            bail!(
                "FlightProxy relay token failed: status={} (X-Skypetoken: {}), body: {}",
                status2,
                status,
                &body[..body.len().min(200)]
            );
        }

        return parse_relay_response(resp2).await;
    }

    parse_relay_response(resp).await
}

async fn parse_relay_response(resp: reqwest::Response) -> Result<RelayConfig> {
    let body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse relay token JSON")?;

    // Shape 1: { "relay": { "token": "...", ... }, "servers": [...] }
    // Shape 2: { "username": "...", "credential": "...", "urls": [...], "ttl": N }
    // Shape 3: { "TurnServerCredentials": [{ ... }] }

    // Try shape 2 (standard TURN credentials format)
    if let Some(username) = body.get("username").and_then(|v| v.as_str()) {
        let credential = body
            .get("credential")
            .or_else(|| body.get("password"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let ttl = body.get("ttl").and_then(|v| v.as_u64()).unwrap_or(3600) as u32;

        let mut servers = Vec::new();
        if let Some(urls) = body.get("urls").and_then(|v| v.as_array()) {
            for url in urls {
                if let Some(url_str) = url.as_str() {
                    if let Some(server) = parse_turn_url(url_str) {
                        servers.push(server);
                    }
                }
            }
        }

        if servers.is_empty() {
            // Default to FlightProxy TURN server
            servers.push(TurnServer {
                host: microsoft_calling::FLIGHTPROXY_TURN_HOST.into(),
                port: microsoft_calling::FLIGHTPROXY_TURN_PORT,
                transport: TurnTransport::Udp,
            });
        }

        return Ok(RelayConfig {
            servers,
            username: username.to_string(),
            credential,
            ttl,
        });
    }

    // Try shape 3 (Teams-specific)
    if let Some(creds) = body.get("TurnServerCredentials").and_then(|v| v.as_array()) {
        if let Some(first) = creds.first() {
            let username = first
                .get("username")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let credential = first
                .get("password")
                .or_else(|| first.get("credential"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let ttl = first.get("ttl").and_then(|v| v.as_u64()).unwrap_or(3600) as u32;

            let mut servers = Vec::new();
            if let Some(urls) = first.get("urls").and_then(|v| v.as_array()) {
                for url in urls {
                    if let Some(url_str) = url.as_str() {
                        if let Some(server) = parse_turn_url(url_str) {
                            servers.push(server);
                        }
                    }
                }
            }
            if servers.is_empty() {
                servers.push(TurnServer {
                    host: microsoft_calling::FLIGHTPROXY_TURN_HOST.into(),
                    port: microsoft_calling::FLIGHTPROXY_TURN_PORT,
                    transport: TurnTransport::Udp,
                });
            }

            return Ok(RelayConfig {
                servers,
                username,
                credential,
                ttl,
            });
        }
    }

    bail!(
        "Unrecognized relay token response format: {}",
        &body.to_string()[..body.to_string().len().min(300)]
    )
}

/// Parse a TURN URL like `turn:host:port?transport=udp` or `turns:host:port`.
fn parse_turn_url(url: &str) -> Option<TurnServer> {
    let (scheme, rest) = if let Some(rest) = url.strip_prefix("turns:") {
        (TurnTransport::Tls, rest)
    } else {
        (TurnTransport::Udp, url.strip_prefix("turn:")?)
    };

    let (host_port, query) = rest.split_once('?').unwrap_or((rest, ""));
    let transport = if query.contains("transport=tcp") {
        TurnTransport::Tcp
    } else if query.contains("transport=tls") {
        TurnTransport::Tls
    } else {
        scheme
    };

    let (host, port) = if let Some((h, p)) = host_port.rsplit_once(':') {
        (h.to_string(), p.parse().unwrap_or(3478))
    } else {
        (host_port.to_string(), 3478)
    };

    Some(TurnServer {
        host,
        port,
        transport,
    })
}

// TURN client (RFC 5766)

/// TURN client that communicates with a TURN server over UDP.
pub struct TurnClient {
    socket: UdpSocket,
    server_addr: SocketAddr,
    username: String,
    credential: String,
    /// Realm and nonce from server (populated after first 401 response).
    realm: Option<String>,
    nonce: Option<String>,
    /// Our allocated relay address (set after successful Allocate).
    relay_addr: Option<SocketAddr>,
    lifetime: u32,
}

impl TurnClient {
    /// Create a new TURN client.
    pub async fn new(server: &TurnServer, username: &str, credential: &str) -> Result<Self> {
        let addr_str = format!("{}:{}", server.host, server.port);
        let server_addr: SocketAddr = tokio::net::lookup_host(&addr_str)
            .await
            .context("Failed to resolve TURN server")?
            .next()
            .context("No address for TURN server")?;

        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .context("Failed to bind TURN client socket")?;

        Ok(TurnClient {
            socket,
            server_addr,
            username: username.to_string(),
            credential: credential.to_string(),
            realm: None,
            nonce: None,
            relay_addr: None,
            lifetime: 600,
        })
    }

    /// The local address of the UDP socket used by this client.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.socket.local_addr().context("local_addr")
    }

    /// The allocated relay address, if any.
    pub fn relay_addr(&self) -> Option<SocketAddr> {
        self.relay_addr
    }

    /// Perform TURN Allocate (RFC 5766 section 6).
    ///
    /// First sends an unauthenticated request to get realm+nonce (401),
    pub async fn allocate(&mut self) -> Result<SocketAddr> {
        let txn1 = ice::generate_transaction_id();
        let req1 = build_allocate_request(&txn1, None, None, None);
        self.socket
            .send_to(&req1, self.server_addr)
            .await
            .context("TURN allocate send")?;

        let resp1 = self.recv_turn_response(&txn1).await;
        match resp1 {
            Ok(TurnResponse::Error {
                code: 401,
                realm,
                nonce,
                ..
            }) => {
                self.realm = realm;
                self.nonce = nonce;
                tracing::debug!("TURN 401 received, realm={:?}", self.realm);
            }
            Ok(TurnResponse::AllocateSuccess {
                relay_addr,
                lifetime,
                ..
            }) => {
                // Server accepted without auth (unlikely but handle it)
                self.relay_addr = Some(relay_addr);
                self.lifetime = lifetime;
                tracing::info!("TURN allocated (no auth): {}", relay_addr);
                return Ok(relay_addr);
            }
            Ok(TurnResponse::Error { code, reason, .. }) => {
                bail!("TURN allocate rejected: {} {}", code, reason);
            }
            Err(e) => {
                bail!("TURN allocate failed (no response): {:#}", e);
            }
            _ => {
                bail!("Unexpected TURN response to initial allocate");
            }
        }

        let realm = self.realm.as_deref().context("No realm from TURN server")?;
        let nonce = self.nonce.as_deref().context("No nonce from TURN server")?;
        let key = compute_long_term_key(&self.username, realm, &self.credential);

        let txn2 = ice::generate_transaction_id();
        let req2 = build_allocate_request(&txn2, Some(&self.username), Some(realm), Some(nonce));
        let req2 = add_message_integrity_and_fingerprint(req2, &key);

        self.socket
            .send_to(&req2, self.server_addr)
            .await
            .context("TURN allocate send (auth)")?;

        match self.recv_turn_response(&txn2).await? {
            TurnResponse::AllocateSuccess {
                relay_addr,
                lifetime,
                ..
            } => {
                self.relay_addr = Some(relay_addr);
                self.lifetime = lifetime;
                tracing::info!(
                    "TURN allocated: relay={}, lifetime={}s",
                    relay_addr,
                    lifetime
                );
                Ok(relay_addr)
            }
            TurnResponse::Error { code, reason, .. } => {
                bail!("TURN allocate failed: {} {}", code, reason);
            }
            _ => {
                bail!("Unexpected TURN allocate response");
            }
        }
    }

    /// Create a TURN permission for a peer address (RFC 5766 section 9).
    pub async fn create_permission(&mut self, peer_addr: SocketAddr) -> Result<()> {
        let key = self.auth_key()?;
        let txn = ice::generate_transaction_id();
        let mut buf = build_stun_header(CREATE_PERMISSION_REQUEST, &txn);

        let xpa = encode_xor_address(peer_addr, &txn);
        append_attr(&mut buf, ATTR_XOR_PEER_ADDRESS, &xpa);

        append_auth_attrs(
            &mut buf,
            &self.username,
            self.realm.as_deref(),
            self.nonce.as_deref(),
        );
        let buf = add_message_integrity_and_fingerprint(buf, &key);

        self.socket.send_to(&buf, self.server_addr).await?;

        match self.recv_turn_response(&txn).await? {
            TurnResponse::Success => {
                tracing::debug!("TURN permission created for {}", peer_addr);
                Ok(())
            }
            TurnResponse::Error { code, reason, .. } => {
                bail!("TURN CreatePermission failed: {} {}", code, reason);
            }
            _ => Ok(()),
        }
    }

    /// Send data through the TURN relay via Send Indication (RFC 5766 section 10).
    pub async fn send_indication(&self, peer_addr: SocketAddr, data: &[u8]) -> Result<()> {
        let txn = ice::generate_transaction_id();
        let mut buf = build_stun_header(SEND_INDICATION, &txn);

        let xpa = encode_xor_address(peer_addr, &txn);
        append_attr(&mut buf, ATTR_XOR_PEER_ADDRESS, &xpa);

        append_attr(&mut buf, ATTR_DATA, data);

        let attr_len = (buf.len() - STUN_HEADER_SIZE) as u16;
        buf[2..4].copy_from_slice(&attr_len.to_be_bytes());

        self.socket.send_to(&buf, self.server_addr).await?;
        Ok(())
    }

    /// Bind a channel number to a peer address for lower-overhead data relay.
    pub async fn channel_bind(&mut self, peer_addr: SocketAddr, channel: u16) -> Result<()> {
        if !(0x4000..=0x7FFE).contains(&channel) {
            bail!("Channel number must be in range 0x4000..0x7FFE");
        }

        let key = self.auth_key()?;
        let txn = ice::generate_transaction_id();
        let mut buf = build_stun_header(CHANNEL_BIND_REQUEST, &txn);

        let mut cn = [0u8; 4];
        cn[0..2].copy_from_slice(&channel.to_be_bytes());
        append_attr(&mut buf, ATTR_CHANNEL_NUMBER, &cn);

        let xpa = encode_xor_address(peer_addr, &txn);
        append_attr(&mut buf, ATTR_XOR_PEER_ADDRESS, &xpa);

        append_auth_attrs(
            &mut buf,
            &self.username,
            self.realm.as_deref(),
            self.nonce.as_deref(),
        );
        let buf = add_message_integrity_and_fingerprint(buf, &key);

        self.socket.send_to(&buf, self.server_addr).await?;

        match self.recv_turn_response(&txn).await? {
            TurnResponse::Success => {
                tracing::debug!("TURN channel {} bound to {}", channel, peer_addr);
                Ok(())
            }
            TurnResponse::Error { code, reason, .. } => {
                bail!("TURN ChannelBind failed: {} {}", code, reason);
            }
            _ => Ok(()),
        }
    }

    /// Receive and parse a Data Indication or ChannelData message.
    ///
    /// Returns `(peer_addr, data)` if a relayed packet is received,
    /// or `None` for non-data messages.
    pub async fn recv_data(&self, timeout: Duration) -> Result<Option<(SocketAddr, Vec<u8>)>> {
        let mut buf = [0u8; 4096];
        match tokio::time::timeout(timeout, self.socket.recv_from(&mut buf)).await {
            Ok(Ok((len, _from))) => {
                let data = &buf[..len];

                if len >= 4 && (data[0] & 0xC0) != 0 {
                    let _channel = u16::from_be_bytes([data[0], data[1]]);
                    let data_len = u16::from_be_bytes([data[2], data[3]]) as usize;
                    if len >= 4 + data_len {
                        // ChannelData doesn't carry peer address; caller must track channel->peer mapping
                        return Ok(Some((self.server_addr, data[4..4 + data_len].to_vec())));
                    }
                }

                // Check for Data Indication (STUN message type 0x0017)
                if len >= STUN_HEADER_SIZE {
                    let msg_type = u16::from_be_bytes([data[0], data[1]]);
                    if msg_type == DATA_INDICATION {
                        let txn_id = &data[8..20];
                        return parse_data_indication(data, txn_id);
                    }
                }

                Ok(None)
            }
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Ok(None),
        }
    }

    /// Build the long-term credential key for message integrity.
    fn auth_key(&self) -> Result<Vec<u8>> {
        let realm = self
            .realm
            .as_deref()
            .context("No realm set (call allocate first)")?;
        Ok(compute_long_term_key(
            &self.username,
            realm,
            &self.credential,
        ))
    }

    /// Receive a TURN response matching the given transaction ID.
    async fn recv_turn_response(&self, expected_txn: &[u8; 12]) -> Result<TurnResponse> {
        let mut buf = [0u8; 2048];
        let deadline = tokio::time::Instant::now() + ALLOCATE_TIMEOUT;

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                bail!("TURN response timeout");
            }

            match tokio::time::timeout(remaining, self.socket.recv_from(&mut buf)).await {
                Ok(Ok((len, _from))) => {
                    let data = &buf[..len];
                    if len < STUN_HEADER_SIZE {
                        continue;
                    }

                    let magic = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
                    if magic != MAGIC_COOKIE {
                        continue;
                    }

                    if &data[8..20] != expected_txn {
                        continue;
                    }

                    let msg_type = u16::from_be_bytes([data[0], data[1]]);
                    return parse_turn_response(data, msg_type);
                }
                Ok(Err(e)) => {
                    bail!("TURN recv error: {}", e);
                }
                Err(_) => {
                    bail!("TURN response timeout");
                }
            }
        }
    }
}

/// Gather a relay candidate by performing a TURN Allocate.
///
/// Returns an `IceCandidate` of type `Relay` with the allocated relay address,
/// and the `TurnClient` for subsequent data relay.
pub async fn gather_relay_candidate(
    relay_config: &RelayConfig,
) -> Option<(IceCandidate, TurnClient)> {
    // Find the first UDP server
    let server = relay_config
        .servers
        .iter()
        .find(|s| s.transport == TurnTransport::Udp)?;

    let mut client =
        match TurnClient::new(server, &relay_config.username, &relay_config.credential).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to create TURN client: {:#}", e);
                return None;
            }
        };

    let relay_addr = match client.allocate().await {
        Ok(addr) => addr,
        Err(e) => {
            tracing::warn!("TURN allocate failed: {:#}", e);
            return None;
        }
    };

    let local_addr = client.local_addr().ok()?;

    let candidate = IceCandidate {
        foundation: "3".into(),
        component: 1,
        transport: Transport::Udp,
        priority: compute_relay_priority(1, 1),
        address: relay_addr.ip().to_string(),
        port: relay_addr.port(),
        candidate_type: CandidateType::Relay,
        raddr: Some(local_addr.ip().to_string()),
        rport: Some(local_addr.port()),
    };

    Some((candidate, client))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_turn_url_udp() {
        let server = parse_turn_url("turn:example.com:3478").unwrap();
        assert_eq!(server.host, "example.com");
        assert_eq!(server.port, 3478);
        assert_eq!(server.transport, TurnTransport::Udp);
    }

    #[test]
    fn test_parse_turn_url_tcp() {
        let server = parse_turn_url("turn:relay.example.com:443?transport=tcp").unwrap();
        assert_eq!(server.host, "relay.example.com");
        assert_eq!(server.port, 443);
        assert_eq!(server.transport, TurnTransport::Tcp);
    }

    #[test]
    fn test_parse_turn_url_tls() {
        let server = parse_turn_url("turns:relay.example.com:443").unwrap();
        assert_eq!(server.host, "relay.example.com");
        assert_eq!(server.port, 443);
        assert_eq!(server.transport, TurnTransport::Tls);
    }

    #[test]
    fn test_parse_turn_url_default_port() {
        let server = parse_turn_url("turn:example.com").unwrap();
        assert_eq!(server.port, 3478);
    }

    #[test]
    fn test_parse_turn_url_invalid() {
        assert!(parse_turn_url("http://example.com").is_none());
        assert!(parse_turn_url("").is_none());
    }

    #[test]
    fn test_relay_priority_lowest() {
        let relay = compute_relay_priority(65535, 1);
        let host = (126u32 << 24) | (65535u32 << 8) | 255;
        assert!(relay < host);
    }

    #[test]
    fn test_build_allocate_request_basic() {
        let txn = [0x42u8; 12];
        let req = build_allocate_request(&txn, None, None, None);

        // Should be a valid STUN message
        assert!(req.len() >= STUN_HEADER_SIZE);
        let msg_type = u16::from_be_bytes([req[0], req[1]]);
        assert_eq!(msg_type, ALLOCATE_REQUEST);
        let magic = u32::from_be_bytes([req[4], req[5], req[6], req[7]]);
        assert_eq!(magic, MAGIC_COOKIE);

        let mut found_transport = false;
        iter_attrs(&req, req.len(), |attr_type, value| {
            if attr_type == ATTR_REQUESTED_TRANSPORT {
                found_transport = true;
                assert_eq!(value[0], TRANSPORT_UDP);
            }
        });
        assert!(found_transport);
    }

    #[test]
    fn test_encode_decode_xor_address_roundtrip() {
        let txn = [0x01u8; 12];
        let addr: SocketAddr = "192.168.1.100:12345".parse().unwrap();
        let encoded = encode_xor_address(addr, &txn);
        let decoded = decode_xor_address(&encoded, &txn).unwrap();
        assert_eq!(decoded, addr);
    }

    #[test]
    fn test_is_turn_data_message() {
        let mut msg = vec![0u8; 24];
        msg[0] = 0x00;
        msg[1] = 0x17;
        msg[4] = 0x21;
        msg[5] = 0x12;
        msg[6] = 0xA4;
        msg[7] = 0x42;
        assert!(is_turn_data_message(&msg));

        // Not a TURN message
        assert!(!is_turn_data_message(&[0u8; 10]));
    }

    #[test]
    fn test_is_channel_data() {
        let msg = [0x40, 0x00, 0x00, 0x04, 0x01, 0x02, 0x03, 0x04];
        assert!(is_channel_data(&msg));

        // STUN message (first two bits = 00)
        let stun = [0x00, 0x01, 0x00, 0x00];
        assert!(!is_channel_data(&stun));
    }

    #[test]
    fn test_relay_config_struct() {
        let config = RelayConfig {
            servers: vec![TurnServer {
                host: "turn.example.com".into(),
                port: 3478,
                transport: TurnTransport::Udp,
            }],
            username: "user".into(),
            credential: "pass".into(),
            ttl: 3600,
        };
        assert_eq!(config.servers.len(), 1);
        assert_eq!(config.ttl, 3600);
    }

    #[test]
    fn test_message_integrity_and_fingerprint() {
        let txn = [0xABu8; 12];
        let req = build_allocate_request(&txn, Some("testuser"), Some("realm"), Some("nonce"));
        let key = b"testpassword";
        let final_msg = add_message_integrity_and_fingerprint(req, key);

        let len = final_msg.len();
        let fp_type = u16::from_be_bytes([final_msg[len - 8], final_msg[len - 7]]);
        assert_eq!(fp_type, ATTR_FINGERPRINT);

        let fp_val = u32::from_be_bytes([
            final_msg[len - 4],
            final_msg[len - 3],
            final_msg[len - 2],
            final_msg[len - 1],
        ]);
        let computed_crc = crc32(&final_msg[..len - 8]);
        assert_eq!(fp_val, computed_crc ^ FINGERPRINT_XOR);
    }
}
