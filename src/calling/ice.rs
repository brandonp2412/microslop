//! ICE implementation — candidate parsing, STUN binding, connectivity checks.
//!
//! Implements enough of RFC 5389 (STUN) and RFC 8445 (ICE) for direct UDP
//! connectivity with the remote peer:
//! 1. Parse ICE candidates from remote SDP offer
//! 2. Build/parse STUN Binding Request/Response with full attributes
//! 3. Perform ICE connectivity checks with MESSAGE-INTEGRITY + FINGERPRINT
//! 4. Gather local host and server-reflexive candidates
//! 5. IceAgent orchestrates the check workflow

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use tokio::net::UdpSocket;

mod candidate;
mod stun;

pub use candidate::{
    parse_candidate, parse_candidates_from_sdp, parse_candidates_from_sdp_section,
    parse_main_video_candidates_from_sdp, select_remote_candidate, CandidateType, IceCandidate,
    IceCredentials, Transport,
};
pub use stun::{
    build_binding_response, build_ice_binding_request, build_stun_binding_request,
    generate_transaction_id, get_transaction_id, is_stun_message, is_stun_request,
    is_stun_response, parse_binding_response, verify_message_integrity,
};

#[cfg(test)]
use stun::{
    crc32, decode_xor_mapped_address, encode_xor_mapped_address, ATTR_FINGERPRINT, FINGERPRINT_XOR,
};

/// Default public STUN server for server-reflexive candidate gathering.
pub const DEFAULT_STUN_SERVER: &str = "stun.l.google.com:19302";

/// ICE connectivity check timeout per attempt.
const CHECK_TIMEOUT: Duration = Duration::from_millis(500);

const CHECK_MAX_RETRIES: u32 = 3;

// ICE candidate gathering

/// Gather host candidates from the local socket's bound address.
///
/// Returns candidates for all local non-loopback IPv4 addresses if bound to 0.0.0.0,
/// otherwise just the bound address.
pub fn gather_host_candidates(local_addr: SocketAddr) -> Vec<IceCandidate> {
    let mut candidates = Vec::new();
    let port = local_addr.port();

    if local_addr.ip().is_unspecified() {
        if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if socket.connect("8.8.8.8:80").is_ok() {
                if let Ok(addr) = socket.local_addr() {
                    let ip = addr.ip().to_string();
                    candidates.push(IceCandidate {
                        foundation: "1".into(),
                        component: 1,
                        transport: Transport::Udp,
                        priority: compute_priority(CandidateType::Host, 1, 1),
                        address: ip,
                        port,
                        candidate_type: CandidateType::Host,
                        raddr: None,
                        rport: None,
                    });
                }
            }
        }
    } else {
        candidates.push(IceCandidate {
            foundation: "1".into(),
            component: 1,
            transport: Transport::Udp,
            priority: compute_priority(CandidateType::Host, 1, 1),
            address: local_addr.ip().to_string(),
            port,
            candidate_type: CandidateType::Host,
            raddr: None,
            rport: None,
        });
    }

    candidates
}

/// Gather a server-reflexive candidate by sending a STUN Binding Request to a public
/// STUN server. Returns None if the STUN server is unreachable.
pub async fn gather_srflx_candidate(socket: &UdpSocket, stun_server: &str) -> Option<IceCandidate> {
    // Resolve STUN server address
    let server_addr: SocketAddr = match tokio::net::lookup_host(stun_server).await {
        Ok(mut addrs) => addrs.next()?,
        Err(e) => {
            tracing::debug!("Failed to resolve STUN server {}: {}", stun_server, e);
            return None;
        }
    };

    let txn_id = generate_transaction_id();
    let request = build_stun_binding_request(&txn_id);

    for attempt in 0..2 {
        if let Err(e) = socket.send_to(&request, server_addr).await {
            tracing::debug!("STUN send to {} failed: {}", server_addr, e);
            return None;
        }

        let mut buf = [0u8; 256];
        match tokio::time::timeout(Duration::from_secs(2), socket.recv_from(&mut buf)).await {
            Ok(Ok((len, _from))) => {
                let data = &buf[..len];
                if is_stun_response(data) {
                    if let Some(resp_txn) = get_transaction_id(data) {
                        if resp_txn == txn_id {
                            if let Some(mapped_addr) = parse_binding_response(data) {
                                let local_addr = socket.local_addr().ok()?;
                                return Some(IceCandidate {
                                    foundation: "2".into(),
                                    component: 1,
                                    transport: Transport::Udp,
                                    priority: compute_priority(
                                        CandidateType::ServerReflexive,
                                        1,
                                        1,
                                    ),
                                    address: mapped_addr.ip().to_string(),
                                    port: mapped_addr.port(),
                                    candidate_type: CandidateType::ServerReflexive,
                                    raddr: Some(local_addr.ip().to_string()),
                                    rport: Some(local_addr.port()),
                                });
                            }
                        }
                    }
                }
            }
            Ok(Err(e)) => {
                tracing::debug!("STUN recv error (attempt {}): {}", attempt, e);
            }
            Err(_) => {
                tracing::debug!("STUN timeout (attempt {})", attempt);
            }
        }
    }

    None
}

/// Compute ICE candidate priority per RFC 8445 section 5.1.2.1.
fn compute_priority(ctype: CandidateType, local_preference: u16, component: u8) -> u32 {
    let type_preference: u32 = match ctype {
        CandidateType::Host => 126,
        CandidateType::ServerReflexive => 100,
        CandidateType::Relay => 0,
    };
    (type_preference << 24) | ((local_preference as u32) << 8) | (256 - component as u32)
}

// ICE connectivity checks

/// Perform a single STUN connectivity check against a remote candidate.
///
/// Sends a STUN Binding Request with USERNAME, MESSAGE-INTEGRITY, and FINGERPRINT.
/// Returns the XOR-MAPPED-ADDRESS from the response (our address as seen by the peer).
pub async fn check_candidate(
    socket: &UdpSocket,
    candidate_addr: SocketAddr,
    local_creds: &IceCredentials,
    remote_creds: &IceCredentials,
    local_priority: u32,
    controlling: bool,
) -> Result<SocketAddr> {
    let username = format!("{}:{}", remote_creds.ufrag, local_creds.ufrag);
    let tie_breaker = {
        let id = uuid::Uuid::new_v4();
        let b = id.as_bytes();
        u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
    };

    for attempt in 0..CHECK_MAX_RETRIES {
        let txn_id = generate_transaction_id();
        let request = build_ice_binding_request(
            &txn_id,
            &username,
            remote_creds.pwd.as_bytes(),
            local_priority,
            controlling,
            tie_breaker,
        );

        socket
            .send_to(&request, candidate_addr)
            .await
            .with_context(|| format!("STUN send to {} failed", candidate_addr))?;

        tracing::debug!(
            "ICE check #{} sent to {} (txn {:02x}{:02x}{:02x}{:02x}...)",
            attempt + 1,
            candidate_addr,
            txn_id[0],
            txn_id[1],
            txn_id[2],
            txn_id[3]
        );

        // Wait for response on the socket
        let mut buf = [0u8; 512];
        let deadline = tokio::time::Instant::now() + CHECK_TIMEOUT;

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }

            match tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await {
                Ok(Ok((len, from))) => {
                    let data = &buf[..len];

                    if is_stun_request(data) {
                        if !verify_message_integrity(data, local_creds.pwd.as_bytes()) {
                            tracing::debug!(
                                "Ignoring unauthenticated STUN request from {} during ICE check",
                                from
                            );
                            continue;
                        }
                        if let Some(request_txn) = get_transaction_id(data) {
                            let response = build_binding_response(
                                &request_txn,
                                from,
                                Some(local_creds.pwd.as_bytes()),
                            );
                            socket.send_to(&response, from).await?;
                        }
                        continue;
                    }

                    if is_stun_response(data) {
                        if let Some(resp_txn) = get_transaction_id(data) {
                            if resp_txn == txn_id
                                && verify_message_integrity(data, remote_creds.pwd.as_bytes())
                            {
                                if let Some(mapped) = parse_binding_response(data) {
                                    tracing::debug!(
                                        "ICE check success: {} -> mapped {}",
                                        candidate_addr,
                                        mapped
                                    );
                                    return Ok(mapped);
                                }
                            }
                        }
                    }

                    // Could be a STUN request from peer (they check us too) — ignore for now,
                    // the IceAgent handles those separately.
                }
                Ok(Err(e)) => {
                    tracing::debug!("recv error during ICE check: {}", e);
                    break;
                }
                Err(_) => {
                    break;
                }
            }
        }

        tracing::debug!("ICE check #{} to {} timed out", attempt + 1, candidate_addr);
    }

    bail!(
        "ICE connectivity check failed after {} attempts to {}",
        CHECK_MAX_RETRIES,
        candidate_addr
    )
}

// ICE Agent

/// Result of ICE connectivity checks.
#[derive(Debug, Clone)]
pub struct IceCheckResult {
    /// The verified remote address to send media to.
    pub remote_addr: SocketAddr,
    /// Our address as seen by the remote peer (from XOR-MAPPED-ADDRESS).
    pub mapped_addr: SocketAddr,
}

/// ICE agent that performs connectivity checks and handles incoming STUN requests.
pub struct IceAgent {
    /// Local ICE credentials.
    pub local_creds: IceCredentials,
    /// Remote ICE credentials.
    pub remote_creds: IceCredentials,
    /// Whether we are the controlling agent (answerer = controlled for incoming calls).
    pub controlling: bool,
}

impl IceAgent {
    /// Create a new ICE agent.
    pub fn new(
        local_creds: IceCredentials,
        remote_creds: IceCredentials,
        controlling: bool,
    ) -> Self {
        Self {
            local_creds,
            remote_creds,
            controlling,
        }
    }

    /// Run connectivity checks against remote candidates in priority order.
    ///
    /// Returns the first candidate that responds successfully. Peer STUN checks
    /// are answered inline on the same socket so no competing reader can consume
    /// our connectivity-check responses or early media.
    pub async fn check_connectivity(
        &self,
        socket: Arc<UdpSocket>,
        remote_candidates: &[IceCandidate],
    ) -> Result<IceCheckResult> {
        let mut candidates: Vec<&IceCandidate> = remote_candidates
            .iter()
            .filter(|c| c.transport == Transport::Udp && c.component == 1)
            .collect();
        candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.priority));

        if candidates.is_empty() {
            bail!("No UDP candidates to check");
        }

        tracing::info!(
            "Starting ICE connectivity checks ({} candidates, local_ufrag={}, remote_ufrag={})",
            candidates.len(),
            self.local_creds.ufrag,
            self.remote_creds.ufrag
        );

        let local_priority = compute_priority(CandidateType::Host, 1, 1);
        let mut last_error = None;

        for candidate in &candidates {
            let addr_str = format!("{}:{}", candidate.address, candidate.port);
            let candidate_addr: SocketAddr = match addr_str.parse() {
                Ok(a) => a,
                Err(e) => {
                    tracing::debug!("Skipping unparseable candidate {}: {}", addr_str, e);
                    continue;
                }
            };

            match check_candidate(
                &socket,
                candidate_addr,
                &self.local_creds,
                &self.remote_creds,
                local_priority,
                self.controlling,
            )
            .await
            {
                Ok(mapped) => {
                    return Ok(IceCheckResult {
                        remote_addr: candidate_addr,
                        mapped_addr: mapped,
                    });
                }
                Err(e) => {
                    tracing::debug!("ICE check to {} failed: {}", candidate_addr, e);
                    last_error = Some(e);
                }
            }
        }

        bail!(
            "All ICE connectivity checks failed. Last error: {}",
            last_error
                .map(|e| format!("{:#}", e))
                .unwrap_or_else(|| "unknown".into())
        )
    }

    /// Handle a single incoming STUN binding request and send a response.
    /// Returns the source address if it was a valid STUN request.
    pub async fn handle_stun_request(
        &self,
        socket: &UdpSocket,
        data: &[u8],
        from: SocketAddr,
    ) -> Option<SocketAddr> {
        if !is_stun_request(data) {
            return None;
        }

        let txn_id = get_transaction_id(data)?;

        if !verify_message_integrity(data, self.local_creds.pwd.as_bytes()) {
            tracing::debug!("STUN request from {} failed MESSAGE-INTEGRITY check", from);
            return None;
        }
        let response = build_binding_response(&txn_id, from, Some(self.local_creds.pwd.as_bytes()));

        if let Err(e) = socket.send_to(&response, from).await {
            tracing::debug!("Failed to send STUN response to {}: {}", from, e);
        } else {
            tracing::debug!("Sent STUN binding response to {} (mapped: {})", from, from);
        }

        Some(from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_host_candidate() {
        let line = "a=candidate:1 1 UDP 2130706431 10.0.0.1 21730 typ host";
        let c = parse_candidate(line).unwrap();
        assert_eq!(c.foundation, "1");
        assert_eq!(c.component, 1);
        assert_eq!(c.transport, Transport::Udp);
        assert_eq!(c.priority, 2130706431);
        assert_eq!(c.address, "10.0.0.1");
        assert_eq!(c.port, 21730);
        assert_eq!(c.candidate_type, CandidateType::Host);
        assert!(c.raddr.is_none());
    }

    #[test]
    fn test_parse_relay_candidate() {
        let line =
            "a=candidate:3 1 UDP 184548351 52.114.0.1 27882 typ relay raddr 10.0.0.1 rport 11632";
        let c = parse_candidate(line).unwrap();
        assert_eq!(c.candidate_type, CandidateType::Relay);
        assert_eq!(c.raddr.as_deref(), Some("10.0.0.1"));
        assert_eq!(c.rport, Some(11632));
    }

    #[test]
    fn test_parse_srflx_candidate() {
        let line =
            "a=candidate:6 1 UDP 1694234111 203.0.113.1 11632 typ srflx raddr 10.0.0.1 rport 11632";
        let c = parse_candidate(line).unwrap();
        assert_eq!(c.candidate_type, CandidateType::ServerReflexive);
    }

    #[test]
    fn test_select_best_candidate() {
        let candidates = vec![
            IceCandidate {
                foundation: "3".into(),
                component: 1,
                transport: Transport::Udp,
                priority: 184548351,
                address: "52.114.0.1".into(),
                port: 27882,
                candidate_type: CandidateType::Relay,
                raddr: Some("10.0.0.1".into()),
                rport: Some(11632),
            },
            IceCandidate {
                foundation: "1".into(),
                component: 1,
                transport: Transport::Udp,
                priority: 2130706431,
                address: "10.0.0.1".into(),
                port: 21730,
                candidate_type: CandidateType::Host,
                raddr: None,
                rport: None,
            },
        ];

        let selected = select_remote_candidate(&candidates).unwrap();
        assert_eq!(selected.to_string(), "10.0.0.1:21730");
    }

    #[test]
    fn test_parse_candidates_from_sdp() {
        let sdp = "\
v=0\r\n\
o=- 0 0 IN IP4 10.0.0.1\r\n\
s=session\r\n\
t=0 0\r\n\
m=audio 21730 RTP/SAVP 0\r\n\
a=candidate:1 1 UDP 2130706431 10.0.0.1 21730 typ host\r\n\
a=candidate:1 2 UDP 2130705918 10.0.0.1 21731 typ host\r\n\
a=candidate:3 1 UDP 184548351 52.0.0.1 27882 typ relay raddr 10.0.0.1 rport 11632\r\n\
m=video 14606 RTP/SAVP 122\r\n\
a=candidate:1 1 UDP 2130706431 10.0.0.1 14606 typ host\r\n";

        let candidates = parse_candidates_from_sdp(sdp);
        assert_eq!(candidates.len(), 3);
    }

    #[test]
    fn test_stun_binding_request() {
        let txn_id = [1u8; 12];
        let req = build_stun_binding_request(&txn_id);
        assert_eq!(req.len(), 20);
        assert_eq!(&req[0..2], &[0x00, 0x01]);
        assert_eq!(&req[4..8], &[0x21, 0x12, 0xA4, 0x42]);
    }

    #[test]
    fn test_is_stun_response() {
        let mut resp = vec![0u8; 20];
        resp[0] = 0x01;
        resp[1] = 0x01;
        resp[4] = 0x21;
        resp[5] = 0x12;
        resp[6] = 0xA4;
        resp[7] = 0x42;
        assert!(is_stun_response(&resp));
        assert!(!is_stun_response(&[0u8; 20]));
        assert!(!is_stun_response(&[0u8; 5]));
    }

    #[test]
    fn test_crc32_known_values() {
        assert_eq!(crc32(b"123456789"), 0xCBF43926);
        assert_eq!(crc32(b""), 0x00000000);
    }

    #[test]
    fn test_xor_mapped_address_roundtrip() {
        let addr: SocketAddr = "192.168.1.100:12345".parse().unwrap();
        let txn_id = [0x01u8; 12];
        let encoded = encode_xor_mapped_address(addr, &txn_id);
        let decoded = decode_xor_mapped_address(&encoded, &txn_id).unwrap();
        assert_eq!(decoded, addr);
    }

    #[test]
    fn test_build_binding_response_parseable() {
        let txn_id = [0xABu8; 12];
        let addr: SocketAddr = "10.0.0.1:5000".parse().unwrap();
        let response = build_binding_response(&txn_id, addr, None);

        assert!(is_stun_response(&response));
        let parsed = parse_binding_response(&response).unwrap();
        assert_eq!(parsed, addr);
    }

    #[tokio::test]
    async fn test_connectivity_check_handles_peer_stun_request() {
        let local = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let local_addr = local.local_addr().unwrap();
        let remote = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let remote_addr = remote.local_addr().unwrap();
        let peer = tokio::spawn(async move {
            let mut buf = [0u8; 512];
            let (len, from) = remote.recv_from(&mut buf).await.unwrap();
            let original_txn = get_transaction_id(&buf[..len]).unwrap();

            let peer_txn = generate_transaction_id();
            let peer_request = build_ice_binding_request(
                &peer_txn,
                "local:remote",
                b"local-password",
                100,
                false,
                0x1234,
            );
            remote.send_to(&peer_request, from).await.unwrap();
            let (len, _) = remote.recv_from(&mut buf).await.unwrap();
            assert!(is_stun_response(&buf[..len]));
            assert_eq!(get_transaction_id(&buf[..len]), Some(peer_txn));
            assert!(verify_message_integrity(&buf[..len], b"local-password"));

            let unauthenticated = build_binding_response(&original_txn, local_addr, None);
            remote.send_to(&unauthenticated, from).await.unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
            let response =
                build_binding_response(&original_txn, local_addr, Some(b"remote-password"));
            remote.send_to(&response, from).await.unwrap();
        });

        let local_creds = IceCredentials {
            ufrag: "local".into(),
            pwd: "local-password".into(),
        };
        let remote_creds = IceCredentials {
            ufrag: "remote".into(),
            pwd: "remote-password".into(),
        };
        let mapped = check_candidate(&local, remote_addr, &local_creds, &remote_creds, 100, true)
            .await
            .unwrap();
        assert_eq!(mapped, local_addr);
        peer.await.unwrap();
    }

    #[test]
    fn test_ice_binding_request_has_integrity_and_fingerprint() {
        let txn_id = [0x42u8; 12];
        let key = b"testpassword";
        let request = build_ice_binding_request(&txn_id, "remote:local", key, 100, true, 0xDEAD);

        // Should be a valid STUN message
        assert!(is_stun_message(&request));
        assert!(is_stun_request(&request));

        assert!(verify_message_integrity(&request, key));

        assert!(!verify_message_integrity(&request, b"wrongkey"));
    }

    #[test]
    fn test_ice_binding_request_fingerprint() {
        let txn_id = [0x42u8; 12];
        let key = b"testpassword";
        let request = build_ice_binding_request(&txn_id, "remote:local", key, 100, true, 0xDEAD);

        let len = request.len();
        assert!(len >= 28); // at least header + some attrs + fingerprint
        let fp_type = u16::from_be_bytes([request[len - 8], request[len - 7]]);
        assert_eq!(fp_type, ATTR_FINGERPRINT);

        let fp_val = u32::from_be_bytes([
            request[len - 4],
            request[len - 3],
            request[len - 2],
            request[len - 1],
        ]);

        let crc = crc32(&request[..len - 8]);
        assert_eq!(fp_val, crc ^ FINGERPRINT_XOR);
    }

    #[test]
    fn test_candidate_to_sdp_line() {
        let c = IceCandidate {
            foundation: "1".into(),
            component: 1,
            transport: Transport::Udp,
            priority: 2130706431,
            address: "10.0.0.1".into(),
            port: 21730,
            candidate_type: CandidateType::Host,
            raddr: None,
            rport: None,
        };
        let line = c.to_sdp_line();
        assert_eq!(line, "candidate:1 1 UDP 2130706431 10.0.0.1 21730 typ host");
    }

    #[test]
    fn test_candidate_to_sdp_line_srflx() {
        let c = IceCandidate {
            foundation: "2".into(),
            component: 1,
            transport: Transport::Udp,
            priority: 1694498815,
            address: "203.0.113.1".into(),
            port: 11632,
            candidate_type: CandidateType::ServerReflexive,
            raddr: Some("10.0.0.1".into()),
            rport: Some(21730),
        };
        let line = c.to_sdp_line();
        assert!(line.contains("typ srflx"));
        assert!(line.contains("raddr 10.0.0.1 rport 21730"));
    }

    #[test]
    fn test_gather_host_candidates() {
        let addr: SocketAddr = "192.168.1.100:5000".parse().unwrap();
        let candidates = gather_host_candidates(addr);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].address, "192.168.1.100");
        assert_eq!(candidates[0].port, 5000);
        assert_eq!(candidates[0].candidate_type, CandidateType::Host);
    }

    #[test]
    fn test_compute_priority() {
        let host = compute_priority(CandidateType::Host, 65535, 1);
        let srflx = compute_priority(CandidateType::ServerReflexive, 65535, 1);
        let relay = compute_priority(CandidateType::Relay, 65535, 1);
        assert!(host > srflx);
        assert!(srflx > relay);
    }
}
