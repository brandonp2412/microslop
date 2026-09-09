//! Media session orchestrator — binds UDP socket, sends/receives SRTP audio.
//!
//! The `MediaSession` ties together RTP encoding, SRTP encryption, and UDP
//! transport into a running audio stream. For now it sends silence (PCMU)
//! and logs received packet statistics.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::time;

use super::{ice, rtcp, rtp, srtp, video};

mod video_session;

pub use video_session::VideoMediaSession;

#[cfg(feature = "audio")]
use super::audio;

/// Statistics for received media packets.
#[derive(Debug, Default)]
pub struct MediaStats {
    pub packets_sent: u64,
    pub packets_received: u64,
    pub bytes_received: u64,
    pub last_seq: u16,
    pub last_timestamp: u32,
}

/// A running media session for a single audio stream.
pub struct MediaSession {
    socket: Arc<UdpSocket>,
    stats: Arc<Mutex<MediaStats>>,
    send_handle: Option<tokio::task::JoinHandle<()>>,
    recv_handle: Option<tokio::task::JoinHandle<()>>,
    /// Audio capture handle (kept alive so the stream stays open).
    #[cfg(feature = "audio")]
    _audio_capture: Option<audio::AudioCapture>,
    /// Audio playback handle (kept alive so the stream stays open).
    #[cfg(feature = "audio")]
    _audio_playback: Option<audio::AudioPlayback>,
}

impl MediaSession {
    /// Create and start a new media session with ICE connectivity checks.
    ///
    /// Binds a UDP socket, runs ICE checks against remote candidates, then starts
    /// the send/recv loops to the verified remote address.
    pub async fn start_with_ice(
        local_port: u16,
        remote_candidates: &[ice::IceCandidate],
        local_creds: &ice::IceCredentials,
        remote_creds: &ice::IceCredentials,
        local_material: &srtp::SrtpKeyingMaterial,
        remote_material: &srtp::SrtpKeyingMaterial,
    ) -> Result<Self> {
        let bind_addr = format!("0.0.0.0:{}", local_port);
        let socket = UdpSocket::bind(&bind_addr)
            .await
            .with_context(|| format!("Failed to bind UDP socket on {}", bind_addr))?;

        Self::start_with_bound_ice(
            socket,
            remote_candidates,
            local_creds,
            remote_creds,
            local_material,
            remote_material,
        )
        .await
    }

    pub async fn start_with_bound_ice(
        socket: UdpSocket,
        remote_candidates: &[ice::IceCandidate],
        local_creds: &ice::IceCredentials,
        remote_creds: &ice::IceCredentials,
        local_material: &srtp::SrtpKeyingMaterial,
        remote_material: &srtp::SrtpKeyingMaterial,
    ) -> Result<Self> {
        let local_addr = socket.local_addr()?;
        tracing::info!("Media session bound to {} for ICE checks", local_addr);

        let socket = Arc::new(socket);

        let agent = ice::IceAgent::new(local_creds.clone(), remote_creds.clone(), false);

        let ice_result = agent
            .check_connectivity(socket.clone(), remote_candidates)
            .await;

        let remote_addr = match ice_result {
            Ok(result) => {
                tracing::info!(
                    "ICE check succeeded: remote={}, mapped={}",
                    result.remote_addr,
                    result.mapped_addr
                );
                result.remote_addr
            }
            Err(e) => {
                tracing::warn!("ICE checks failed ({}), falling back to best candidate", e);
                ice::select_remote_candidate(remote_candidates)
                    .context("No candidate available for fallback")?
            }
        };

        let srtp_ctx = srtp::create_context(local_material, remote_material)?;
        let ssrc = {
            let id = uuid::Uuid::new_v4();
            let bytes = id.as_bytes();
            u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        };

        let srtp_ctx = Arc::new(Mutex::new(srtp_ctx));
        let stats = Arc::new(Mutex::new(MediaStats::default()));
        let dynamic_remote_addr = Arc::new(Mutex::new(remote_addr));

        // Initialize audio devices (optional — graceful fallback to silence).
        #[cfg(feature = "audio")]
        let (audio_capture, mic_rx) = match audio::AudioCapture::start() {
            Some((cap, rx)) => (Some(cap), Some(rx)),
            None => (None, None),
        };
        #[cfg(not(feature = "audio"))]
        let mic_rx: Option<std::sync::mpsc::Receiver<Vec<i16>>> = None;

        #[cfg(feature = "audio")]
        let (audio_playback, spk_tx) = match audio::AudioPlayback::start() {
            Some((pb, tx)) => (Some(pb), Some(tx)),
            None => (None, None),
        };
        #[cfg(not(feature = "audio"))]
        let spk_tx: Option<std::sync::mpsc::SyncSender<Vec<i16>>> = None;

        let send_handle = {
            let socket = socket.clone();
            let srtp_ctx = srtp_ctx.clone();
            let stats = stats.clone();
            tokio::spawn(send_loop(
                socket,
                dynamic_remote_addr.clone(),
                srtp_ctx,
                ssrc,
                stats,
                mic_rx,
            ))
        };

        let local_pwd = local_creds.pwd.clone();
        let recv_handle = {
            let socket = socket.clone();
            let srtp_ctx = srtp_ctx.clone();
            let stats = stats.clone();
            tokio::spawn(recv_loop_with_stun(
                socket,
                srtp_ctx,
                stats,
                local_pwd,
                dynamic_remote_addr.clone(),
                spk_tx,
            ))
        };

        Ok(MediaSession {
            socket,
            stats,
            send_handle: Some(send_handle),
            recv_handle: Some(recv_handle),
            #[cfg(feature = "audio")]
            _audio_capture: audio_capture,
            #[cfg(feature = "audio")]
            _audio_playback: audio_playback,
        })
    }

    /// Create and start a new media session (no ICE checks, direct send to remote).
    ///
    /// - `local_port`: UDP port to bind on (0 for auto-assign).
    /// - `remote_addr`: Remote peer's RTP address (from ICE candidate selection).
    /// - `local_material`: Our SRTP keying material (from our SDP answer).
    /// - `remote_material`: Remote SRTP keying material (from their SDP offer).
    pub async fn start(
        local_port: u16,
        remote_addr: SocketAddr,
        local_material: &srtp::SrtpKeyingMaterial,
        remote_material: &srtp::SrtpKeyingMaterial,
    ) -> Result<Self> {
        let bind_addr = format!("0.0.0.0:{}", local_port);
        let socket = UdpSocket::bind(&bind_addr)
            .await
            .with_context(|| format!("Failed to bind UDP socket on {}", bind_addr))?;

        let local_addr = socket.local_addr()?;
        tracing::info!(
            "Media session bound to {}, remote: {}",
            local_addr,
            remote_addr
        );

        let srtp_ctx = srtp::create_context(local_material, remote_material)?;

        let ssrc = {
            let id = uuid::Uuid::new_v4();
            let bytes = id.as_bytes();
            u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        };

        let socket = Arc::new(socket);
        let srtp_ctx = Arc::new(Mutex::new(srtp_ctx));
        let stats = Arc::new(Mutex::new(MediaStats::default()));
        let dynamic_remote_addr = Arc::new(Mutex::new(remote_addr));

        // Initialize audio devices (optional — graceful fallback to silence).
        #[cfg(feature = "audio")]
        let (audio_capture, mic_rx) = match audio::AudioCapture::start() {
            Some((cap, rx)) => (Some(cap), Some(rx)),
            None => (None, None),
        };
        #[cfg(not(feature = "audio"))]
        let mic_rx: Option<std::sync::mpsc::Receiver<Vec<i16>>> = None;

        #[cfg(feature = "audio")]
        let (audio_playback, spk_tx) = match audio::AudioPlayback::start() {
            Some((pb, tx)) => (Some(pb), Some(tx)),
            None => (None, None),
        };
        #[cfg(not(feature = "audio"))]
        let spk_tx: Option<std::sync::mpsc::SyncSender<Vec<i16>>> = None;

        let send_handle = {
            let socket = socket.clone();
            let srtp_ctx = srtp_ctx.clone();
            let stats = stats.clone();
            tokio::spawn(send_loop(
                socket,
                dynamic_remote_addr,
                srtp_ctx,
                ssrc,
                stats,
                mic_rx,
            ))
        };

        let recv_handle = {
            let socket = socket.clone();
            let srtp_ctx = srtp_ctx.clone();
            let stats = stats.clone();
            tokio::spawn(recv_loop(socket, srtp_ctx, stats, spk_tx))
        };

        Ok(MediaSession {
            socket,
            stats,
            send_handle: Some(send_handle),
            recv_handle: Some(recv_handle),
            #[cfg(feature = "audio")]
            _audio_capture: audio_capture,
            #[cfg(feature = "audio")]
            _audio_playback: audio_playback,
        })
    }

    /// Get the local port this session is bound to.
    pub fn local_port(&self) -> Result<u16> {
        Ok(self.socket.local_addr()?.port())
    }

    /// Get current media statistics.
    pub async fn stats(&self) -> MediaStats {
        let s = self.stats.lock().await;
        MediaStats {
            packets_sent: s.packets_sent,
            packets_received: s.packets_received,
            bytes_received: s.bytes_received,
            last_seq: s.last_seq,
            last_timestamp: s.last_timestamp,
        }
    }

    /// Stop the media session.
    pub async fn stop(&mut self) {
        if let Some(h) = self.send_handle.take() {
            h.abort();
            let _ = h.await;
        }
        if let Some(h) = self.recv_handle.take() {
            h.abort();
            let _ = h.await;
        }
        let stats = self.stats.lock().await;
        tracing::info!(
            "Media session stopped. Sent: {}, Received: {} ({} bytes)",
            stats.packets_sent,
            stats.packets_received,
            stats.bytes_received
        );
    }
}

impl Drop for MediaSession {
    fn drop(&mut self) {
        if let Some(h) = self.send_handle.take() {
            h.abort();
        }
        if let Some(h) = self.recv_handle.take() {
            h.abort();
        }
    }
}

/// Send loop: every 20ms, encode audio (or silence) as PCMU, encrypt with SRTP, send.
async fn send_loop(
    socket: Arc<UdpSocket>,
    remote_addr: Arc<Mutex<SocketAddr>>,
    srtp_ctx: Arc<Mutex<srtp::SrtpContext>>,
    ssrc: u32,
    stats: Arc<Mutex<MediaStats>>,
    mic_rx: Option<std::sync::mpsc::Receiver<Vec<i16>>>,
) {
    let mut seq: u16 = 0;
    let mut timestamp: u32 = 0;
    let mut interval = time::interval(Duration::from_millis(rtp::PACKET_INTERVAL_MS));
    let mut rtp_packet = Vec::with_capacity(rtp::RTP_HEADER_SIZE + rtp::SAMPLES_PER_PACKET);

    let has_mic = mic_rx.is_some();
    tracing::info!(
        "Media send loop started (SSRC: {:#010x}, mic: {})",
        ssrc,
        if has_mic { "live" } else { "silence" }
    );

    loop {
        interval.tick().await;

        let captured = mic_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        let mut encoded;
        let payload: &[u8] = if super::call_test::microphone_enabled() {
            match captured {
                Some(samples) => {
                    encoded = Vec::with_capacity(samples.len());
                    for &s in &samples {
                        encoded.push(rtp::linear_to_ulaw(s));
                    }
                    encoded.resize(rtp::SAMPLES_PER_PACKET, 0xFF);
                    &encoded
                }
                None => &rtp::SILENCE_PAYLOAD,
            }
        } else {
            &rtp::SILENCE_PAYLOAD
        };
        rtp::encode_into(&mut rtp_packet, rtp::PT_PCMU, seq, timestamp, ssrc, payload);

        let srtp_packet = {
            let mut ctx = srtp_ctx.lock().await;
            match srtp::protect(&mut ctx, &rtp_packet) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!("SRTP protect failed: {:#}", e);
                    continue;
                }
            }
        };

        let target = *remote_addr.lock().await;
        match socket.send_to(&srtp_packet, target).await {
            Ok(_) => {
                let mut s = stats.lock().await;
                s.packets_sent += 1;
            }
            Err(e) => {
                tracing::warn!("UDP send failed: {:#}", e);
            }
        }

        seq = seq.wrapping_add(1);
        timestamp = timestamp.wrapping_add(rtp::TIMESTAMP_INCREMENT);
    }
}

/// Receive loop: receive UDP packets, try to decrypt SRTP, decode and play audio.
async fn recv_loop(
    socket: Arc<UdpSocket>,
    srtp_ctx: Arc<Mutex<srtp::SrtpContext>>,
    stats: Arc<Mutex<MediaStats>>,
    spk_tx: Option<std::sync::mpsc::SyncSender<Vec<i16>>>,
) {
    let mut buf = [0u8; 2048];

    tracing::info!("Media recv loop started");

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, from)) => {
                let data = &buf[..len];

                // Skip STUN packets
                if len >= 20 && super::ice::is_stun_response(data) {
                    tracing::debug!("Received STUN response from {}", from);
                    continue;
                }

                // Try SRTP decrypt
                let mut ctx = srtp_ctx.lock().await;
                match srtp::unprotect(&mut ctx, data) {
                    Ok(rtp_data) => {
                        if let Ok(pkt) = rtp::decode(&rtp_data) {
                            let mut s = stats.lock().await;
                            s.packets_received += 1;
                            s.bytes_received += rtp_data.len() as u64;
                            s.last_seq = pkt.sequence_number;
                            s.last_timestamp = pkt.timestamp;

                            if s.packets_received % 250 == 1 {
                                tracing::info!(
                                    "Recv audio: seq={}, ts={}, pt={}, payload={} bytes (total: {} pkts)",
                                    pkt.sequence_number,
                                    pkt.timestamp,
                                    pkt.payload_type,
                                    pkt.payload.len(),
                                    s.packets_received
                                );
                            }
                            drop(s);

                            // Decode PCMU and send to speaker unless local output is muted.
                            if super::call_test::speaker_enabled() {
                                if let Some(ref tx) = spk_tx {
                                    let Some(samples) = rtp::decode_pcmu(&pkt) else {
                                        continue;
                                    };
                                    let _ = tx.try_send(samples);
                                }
                            }
                        }
                    }
                    Err(_) => {
                        tracing::trace!("Could not decrypt packet from {} ({} bytes)", from, len);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("UDP recv error: {:#}", e);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

/// Receive loop that also responds to incoming STUN binding requests.
///
/// This is used when ICE is active — the peer continues sending STUN checks
/// during the media session and expects responses to keep the path alive.
async fn recv_loop_with_stun(
    socket: Arc<UdpSocket>,
    srtp_ctx: Arc<Mutex<srtp::SrtpContext>>,
    stats: Arc<Mutex<MediaStats>>,
    local_pwd: String,
    dynamic_remote_addr: Arc<Mutex<SocketAddr>>,
    spk_tx: Option<std::sync::mpsc::SyncSender<Vec<i16>>>,
) {
    let mut buf = [0u8; 2048];

    tracing::info!("Media recv loop started (STUN-aware)");

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, from)) => {
                let data = &buf[..len];

                // Handle STUN messages
                if len >= 20 && ice::is_stun_message(data) {
                    if ice::is_stun_request(data) {
                        let authenticated =
                            ice::verify_message_integrity(data, local_pwd.as_bytes());
                        // Respond to STUN binding requests for compatibility, but only let an
                        // authenticated ICE check change where we send encrypted call media.
                        if let Some(txn_id) = ice::get_transaction_id(data) {
                            let response = ice::build_binding_response(
                                &txn_id,
                                from,
                                Some(local_pwd.as_bytes()),
                            );
                            let _ = socket.send_to(&response, from).await;
                            tracing::debug!("Responded to STUN request from {} during media", from);
                        }
                        if authenticated {
                            let mut target = dynamic_remote_addr.lock().await;
                            if *target != from {
                                tracing::info!(
                                    "Updating media remote addr: {} -> {} (authenticated peer ICE check)",
                                    *target,
                                    from
                                );
                                *target = from;
                            }
                        } else {
                            tracing::debug!(
                                "Ignoring unauthenticated STUN request from {} for media routing",
                                from
                            );
                        }
                    } else {
                        tracing::debug!("Received STUN response from {}", from);
                    }
                    continue;
                }

                // Try SRTP decrypt
                let mut ctx = srtp_ctx.lock().await;
                match srtp::unprotect(&mut ctx, data) {
                    Ok(rtp_data) => {
                        if let Ok(pkt) = rtp::decode(&rtp_data) {
                            let mut s = stats.lock().await;
                            s.packets_received += 1;
                            s.bytes_received += rtp_data.len() as u64;
                            s.last_seq = pkt.sequence_number;
                            s.last_timestamp = pkt.timestamp;

                            if s.packets_received % 250 == 1 {
                                tracing::info!(
                                    "Recv audio: seq={}, ts={}, pt={}, payload={} bytes (total: {} pkts)",
                                    pkt.sequence_number,
                                    pkt.timestamp,
                                    pkt.payload_type,
                                    pkt.payload.len(),
                                    s.packets_received
                                );
                            }
                            drop(s);

                            // Decode PCMU and send to speaker unless local output is muted.
                            if super::call_test::speaker_enabled() {
                                if let Some(ref tx) = spk_tx {
                                    let Some(samples) = rtp::decode_pcmu(&pkt) else {
                                        continue;
                                    };
                                    let _ = tx.try_send(samples);
                                }
                            }
                        }
                    }
                    Err(_) => {
                        tracing::trace!("Could not decrypt packet from {} ({} bytes)", from, len);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("UDP recv error: {:#}", e);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_material() -> srtp::SrtpKeyingMaterial {
        let mut key = [0u8; 16];
        let mut salt = [0u8; 14];
        for (i, byte) in key.iter_mut().enumerate() {
            *byte = (i + 1) as u8;
        }
        for (i, byte) in salt.iter_mut().enumerate() {
            *byte = (i + 17) as u8;
        }
        srtp::SrtpKeyingMaterial {
            master_key: key,
            master_salt: salt,
            tag: 2,
        }
    }

    #[tokio::test]
    async fn test_media_session_binds() {
        let remote: SocketAddr = "192.0.2.1:9999".parse().unwrap();
        let mat = test_material();

        let mut session = MediaSession::start(0, remote, &mat, &mat).await.unwrap();
        let port = session.local_port().unwrap();
        assert!(port > 0);

        tokio::time::sleep(Duration::from_millis(50)).await;

        let stats = session.stats().await;
        assert!(stats.packets_sent >= 1, "sent: {}", stats.packets_sent);

        session.stop().await;
    }

    #[tokio::test]
    async fn video_receive_with_bound_ice_reuses_the_advertised_socket() {
        #[cfg(target_os = "windows")]
        crate::calling::external_display::clear();
        let local = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let advertised_port = local.local_addr().unwrap().port();
        let remote = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let remote_addr = remote.local_addr().unwrap();
        let remote_candidate = ice::IceCandidate {
            foundation: "1".into(),
            component: 1,
            transport: ice::Transport::Udp,
            priority: 2130706431,
            address: remote_addr.ip().to_string(),
            port: remote_addr.port(),
            candidate_type: ice::CandidateType::Host,
            raddr: None,
            rport: None,
        };
        let local_creds = ice::IceCredentials {
            ufrag: "local".into(),
            pwd: "local-password".into(),
        };
        let remote_creds = ice::IceCredentials {
            ufrag: "remote".into(),
            pwd: "remote-password".into(),
        };
        let peer = tokio::spawn(async move {
            let mut buf = [0u8; 512];
            let (len, from) = remote.recv_from(&mut buf).await.unwrap();
            let txn = ice::get_transaction_id(&buf[..len]).unwrap();
            let response = ice::build_binding_response(&txn, from, Some(b"remote-password"));
            remote.send_to(&response, from).await.unwrap();
            remote
        });
        let mat = test_material();
        let mut session = VideoMediaSession::start_receive_with_bound_ice(
            local,
            &[remote_candidate],
            &local_creds,
            &remote_creds,
            &mat,
            &mat,
        )
        .await
        .unwrap();

        assert_eq!(session.local_port().unwrap(), advertised_port);
        let remote = peer.await.unwrap();
        let mut remote_srtp = srtp::create_context(&mat, &mat).unwrap();
        let mut packetizer = video::VideoPacketizer::new(0x10203040);
        #[cfg(feature = "video-codec")]
        let nals = {
            let width = 320u32;
            let height = 240u32;
            let y_size = (width * height) as usize;
            let uv_size = y_size / 4;
            let mut i420 = vec![16; y_size];
            i420.extend(std::iter::repeat_n(128, uv_size * 2));
            let mut encoder =
                crate::calling::codec::H264Encoder::new(width, height, 15.0, 256).unwrap();
            encoder.encode(&i420).unwrap()
        };
        #[cfg(not(feature = "video-codec"))]
        let nals = video::generate_black_iframe();
        for packet in packetizer.packetize_frame(&nals) {
            let encrypted = srtp::protect(&mut remote_srtp, &packet).unwrap();
            remote
                .send_to(&encrypted, ("127.0.0.1", advertised_port))
                .await
                .unwrap();
        }
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let stats = session.stats().await;
                if stats.packets_received > 0 && stats.frames_received > 0 {
                    #[cfg(feature = "video-codec")]
                    if stats.frames_decoded == 0 {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        continue;
                    }
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();

        #[cfg(all(feature = "video-codec", target_os = "windows"))]
        {
            let frame = crate::calling::external_display::latest_frame()
                .expect("decoded Windows video frame was not published");
            assert_eq!((frame.width, frame.height), (320, 240));
            assert_eq!(frame.data.len(), 320 * 240 * 3 / 2);
            crate::calling::external_display::clear();
        }

        session.stop().await;
    }

    #[tokio::test]
    async fn start_with_bound_ice_reuses_the_advertised_socket() {
        let local = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let advertised_port = local.local_addr().unwrap().port();
        let remote = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let remote_addr = remote.local_addr().unwrap();
        let remote_candidate = ice::IceCandidate {
            foundation: "1".into(),
            component: 1,
            transport: ice::Transport::Udp,
            priority: 2130706431,
            address: remote_addr.ip().to_string(),
            port: remote_addr.port(),
            candidate_type: ice::CandidateType::Host,
            raddr: None,
            rport: None,
        };
        let peer = tokio::spawn(async move {
            let mut buf = [0u8; 512];
            let (len, from) = remote.recv_from(&mut buf).await.unwrap();
            let txn = ice::get_transaction_id(&buf[..len]).unwrap();
            let response = ice::build_binding_response(&txn, from, None);
            remote.send_to(&response, from).await.unwrap();
        });
        let local_creds = ice::IceCredentials {
            ufrag: "local".into(),
            pwd: "local-password".into(),
        };
        let remote_creds = ice::IceCredentials {
            ufrag: "remote".into(),
            pwd: "remote-password".into(),
        };
        let mat = test_material();

        let mut session = MediaSession::start_with_bound_ice(
            local,
            &[remote_candidate],
            &local_creds,
            &remote_creds,
            &mat,
            &mat,
        )
        .await
        .unwrap();

        assert_eq!(session.local_port().unwrap(), advertised_port);
        peer.await.unwrap();

        // Teams can nominate a different MCU relay after the SDP exchange. A valid
        // peer ICE check from that address must move the encrypted media send target.
        let migrated = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let peer_txn = ice::generate_transaction_id();
        let peer_check = ice::build_ice_binding_request(
            &peer_txn,
            "local:remote",
            b"local-password",
            100,
            true,
            0x5678,
        );
        migrated
            .send_to(&peer_check, ("127.0.0.1", advertised_port))
            .await
            .unwrap();

        let mut buf = [0u8; 2048];
        let (len, _) = tokio::time::timeout(Duration::from_secs(1), migrated.recv_from(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert!(ice::is_stun_response(&buf[..len]));
        assert_eq!(ice::get_transaction_id(&buf[..len]), Some(peer_txn));
        assert!(ice::verify_message_integrity(
            &buf[..len],
            b"local-password"
        ));

        let (len, _) = tokio::time::timeout(Duration::from_secs(1), migrated.recv_from(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert!(!ice::is_stun_message(&buf[..len]));

        session.stop().await;
    }
}
