use super::*;

/// Statistics for received video packets.
#[derive(Debug, Default)]
pub struct VideoStats {
    pub packets_sent: u64,
    pub packets_received: u64,
    pub bytes_received: u64,
    pub frames_sent: u64,
    pub frames_received: u64,
    pub frames_decoded: u64,
}

/// A running media session for a video stream.
pub struct VideoMediaSession {
    socket: Arc<UdpSocket>,
    stats: Arc<Mutex<VideoStats>>,
    send_handle: Option<tokio::task::JoinHandle<()>>,
    recv_handle: Option<tokio::task::JoinHandle<()>>,
    rtcp_handle: Option<tokio::task::JoinHandle<()>>,
}

impl VideoMediaSession {
    pub async fn start_receive_with_bound_ice(
        socket: UdpSocket,
        remote_candidates: &[ice::IceCandidate],
        local_creds: &ice::IceCredentials,
        remote_creds: &ice::IceCredentials,
        local_material: &srtp::SrtpKeyingMaterial,
        remote_material: &srtp::SrtpKeyingMaterial,
    ) -> Result<Self> {
        let local_addr = socket.local_addr()?;
        tracing::info!("Video session bound to {} for ICE checks", local_addr);
        let socket = Arc::new(socket);
        let agent = ice::IceAgent::new(local_creds.clone(), remote_creds.clone(), false);
        let remote_addr = match agent
            .check_connectivity(socket.clone(), remote_candidates)
            .await
        {
            Ok(result) => result.remote_addr,
            Err(error) => {
                tracing::warn!("Video ICE checks failed ({error}), falling back to best candidate");
                ice::select_remote_candidate(remote_candidates)
                    .context("No video candidate available for fallback")?
            }
        };
        let srtp_ctx = Arc::new(Mutex::new(srtp::create_context(
            local_material,
            remote_material,
        )?));
        let stats = Arc::new(Mutex::new(VideoStats::default()));
        let recv_stats = Arc::new(Mutex::new(rtcp::RtpRecvStats::default()));
        let remote_ssrc = Arc::new(Mutex::new(0u32));
        let dynamic_remote_addr = Arc::new(Mutex::new(remote_addr));
        let recv_handle = tokio::spawn(video_recv_loop_with_stun(
            socket.clone(),
            srtp_ctx.clone(),
            stats.clone(),
            recv_stats.clone(),
            remote_ssrc.clone(),
            local_creds.pwd.clone(),
            dynamic_remote_addr.clone(),
        ));
        let local_ssrc = video::generate_ssrc();
        let cname = format!("microslop-{}", uuid::Uuid::new_v4());
        let rtcp_handle = tokio::spawn(video_rtcp_receive_report_loop(
            socket.clone(),
            srtp_ctx,
            recv_stats,
            remote_ssrc,
            dynamic_remote_addr,
            local_ssrc,
            cname,
        ));

        Ok(VideoMediaSession {
            socket,
            stats,
            send_handle: None,
            recv_handle: Some(recv_handle),
            rtcp_handle: Some(rtcp_handle),
        })
    }

    /// Create and start a new video media session.
    pub async fn start(
        local_port: u16,
        remote_addr: SocketAddr,
        local_material: &srtp::SrtpKeyingMaterial,
        remote_material: &srtp::SrtpKeyingMaterial,
    ) -> Result<Self> {
        let bind_addr = format!("0.0.0.0:{}", local_port);
        let socket = UdpSocket::bind(&bind_addr)
            .await
            .with_context(|| format!("Failed to bind video UDP socket on {}", bind_addr))?;

        let local_addr = socket.local_addr()?;
        tracing::info!(
            "Video session bound to {}, remote: {}",
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
        let stats = Arc::new(Mutex::new(VideoStats::default()));

        let send_handle = {
            let socket = socket.clone();
            let srtp_ctx = srtp_ctx.clone();
            let stats = stats.clone();
            tokio::spawn(video_send_loop(socket, remote_addr, srtp_ctx, ssrc, stats))
        };

        let recv_handle = {
            let socket = socket.clone();
            let srtp_ctx = srtp_ctx.clone();
            let stats = stats.clone();
            tokio::spawn(video_recv_loop(socket, srtp_ctx, stats))
        };

        Ok(VideoMediaSession {
            socket,
            stats,
            send_handle: Some(send_handle),
            recv_handle: Some(recv_handle),
            rtcp_handle: None,
        })
    }

    /// Get the local port this session is bound to.
    pub fn local_port(&self) -> Result<u16> {
        Ok(self.socket.local_addr()?.port())
    }

    /// Get current video statistics.
    pub async fn stats(&self) -> VideoStats {
        let s = self.stats.lock().await;
        VideoStats {
            packets_sent: s.packets_sent,
            packets_received: s.packets_received,
            bytes_received: s.bytes_received,
            frames_sent: s.frames_sent,
            frames_received: s.frames_received,
            frames_decoded: s.frames_decoded,
        }
    }

    /// Stop the video session.
    pub async fn stop(&mut self) {
        if let Some(h) = self.send_handle.take() {
            h.abort();
            let _ = h.await;
        }
        if let Some(h) = self.recv_handle.take() {
            h.abort();
            let _ = h.await;
        }
        if let Some(h) = self.rtcp_handle.take() {
            h.abort();
            let _ = h.await;
        }
        let stats = self.stats.lock().await;
        tracing::info!(
            "Video session stopped. Sent: {} pkts/{} frames, Received: {} pkts/{} frames ({} bytes)",
            stats.packets_sent,
            stats.frames_sent,
            stats.packets_received,
            stats.frames_received,
            stats.bytes_received
        );
    }
}

impl Drop for VideoMediaSession {
    fn drop(&mut self) {
        if let Some(h) = self.send_handle.take() {
            h.abort();
        }
        if let Some(h) = self.recv_handle.take() {
            h.abort();
        }
        if let Some(h) = self.rtcp_handle.take() {
            h.abort();
        }
    }
}

/// Video send loop: periodically send black H.264 I-frames.
async fn video_send_loop(
    socket: Arc<UdpSocket>,
    remote_addr: SocketAddr,
    srtp_ctx: Arc<Mutex<srtp::SrtpContext>>,
    ssrc: u32,
    stats: Arc<Mutex<VideoStats>>,
) {
    let mut packetizer = video::VideoPacketizer::new(ssrc);
    let mut interval = time::interval(Duration::from_millis(video::FRAME_INTERVAL_MS));

    tracing::info!(
        "Video send loop started (SSRC: {:#010x}, 30fps black frames)",
        ssrc
    );

    loop {
        interval.tick().await;

        let rtp_packets = packetizer.packetize_frame(video::black_iframe_nals());

        for rtp_pkt in &rtp_packets {
            let srtp_packet = {
                let mut ctx = srtp_ctx.lock().await;
                match srtp::protect(&mut ctx, rtp_pkt) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!("Video SRTP protect failed: {:#}", e);
                        continue;
                    }
                }
            };

            match socket.send_to(&srtp_packet, remote_addr).await {
                Ok(_) => {
                    let mut s = stats.lock().await;
                    s.packets_sent += 1;
                }
                Err(e) => {
                    tracing::warn!("Video UDP send failed: {:#}", e);
                }
            }
        }

        let mut s = stats.lock().await;
        s.frames_sent += 1;
    }
}

/// Video receive loop: receive and depacketize H.264 NAL units.
async fn video_recv_loop(
    socket: Arc<UdpSocket>,
    srtp_ctx: Arc<Mutex<srtp::SrtpContext>>,
    stats: Arc<Mutex<VideoStats>>,
) {
    let mut buf = [0u8; 2048];
    let mut depacketizer = video::VideoDepacketizer::new();

    tracing::info!("Video recv loop started");

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, from)) => {
                let data = &buf[..len];

                if len >= 20 && super::ice::is_stun_response(data) {
                    tracing::debug!("Video: STUN response from {}", from);
                    continue;
                }

                let mut ctx = srtp_ctx.lock().await;
                match srtp::unprotect(&mut ctx, data) {
                    Ok(rtp_data) => {
                        if let Ok(pkt) = rtp::decode(&rtp_data) {
                            if pkt.payload_type != video::PT_H264 {
                                continue;
                            }
                            let mut s = stats.lock().await;
                            s.packets_received += 1;
                            s.bytes_received += rtp_data.len() as u64;

                            let marker = pkt.marker;
                            drop(s);

                            match depacketizer.depacketize(pkt.payload, marker) {
                                Ok(Some(nal)) => {
                                    let mut s = stats.lock().await;
                                    s.frames_received = depacketizer.frames_received;
                                    if depacketizer.frames_received % 50 == 1 {
                                        tracing::info!(
                                            "Recv video NAL: type={}, size={} bytes (frames: {})",
                                            nal[0] & 0x1F,
                                            nal.len(),
                                            depacketizer.frames_received
                                        );
                                    }
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    tracing::debug!("Video depacketize error: {:#}", e);
                                }
                            }
                        }
                    }
                    Err(_) => {
                        tracing::trace!(
                            "Could not decrypt video packet from {} ({} bytes)",
                            from,
                            len
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Video UDP recv error: {:#}", e);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

async fn video_recv_loop_with_stun(
    socket: Arc<UdpSocket>,
    srtp_ctx: Arc<Mutex<srtp::SrtpContext>>,
    stats: Arc<Mutex<VideoStats>>,
    recv_stats: Arc<Mutex<rtcp::RtpRecvStats>>,
    remote_ssrc: Arc<Mutex<u32>>,
    local_pwd: String,
    dynamic_remote_addr: Arc<Mutex<SocketAddr>>,
) {
    let mut buf = [0u8; 2048];
    let mut depacketizer = video::VideoDepacketizer::new();
    #[cfg(feature = "video-codec")]
    let mut decoder = crate::calling::codec::H264Decoder::new().ok();

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, from)) => {
                let data = &buf[..len];
                if len >= 20 && ice::is_stun_message(data) {
                    if ice::is_stun_request(data)
                        && ice::verify_message_integrity(data, local_pwd.as_bytes())
                    {
                        if let Some(txn_id) = ice::get_transaction_id(data) {
                            let response = ice::build_binding_response(
                                &txn_id,
                                from,
                                Some(local_pwd.as_bytes()),
                            );
                            let _ = socket.send_to(&response, from).await;
                        }
                        let mut target = dynamic_remote_addr.lock().await;
                        if *target != from {
                            tracing::info!("Updating video remote addr: {} -> {}", *target, from);
                            *target = from;
                        }
                    }
                    continue;
                }

                let srtcp = {
                    let mut ctx = srtp_ctx.lock().await;
                    srtp::unprotect_rtcp(&mut ctx, data).ok()
                };
                if let Some(rtcp_data) = srtcp {
                    let blocks = rtcp::parse_rtcp(&rtcp_data);
                    let mut recv = recv_stats.lock().await;
                    for block in blocks {
                        if let rtcp::RtcpBlock::SenderReport { ntp_timestamp, .. } = block {
                            recv.last_sr_ntp = ((ntp_timestamp >> 16) & 0xffff_ffff) as u32;
                            recv.last_sr_recv_time = Some(std::time::Instant::now());
                        }
                    }
                    continue;
                }

                let rtp_data = {
                    let mut ctx = srtp_ctx.lock().await;
                    srtp::unprotect(&mut ctx, data)
                };
                if let Ok(rtp_data) = rtp_data {
                    if let Ok(pkt) = rtp::decode(&rtp_data) {
                        if pkt.payload_type != video::PT_H264 {
                            continue;
                        }
                        {
                            let mut s = stats.lock().await;
                            s.packets_received += 1;
                            s.bytes_received += rtp_data.len() as u64;
                        }
                        {
                            let mut recv = recv_stats.lock().await;
                            recv.packets_received += 1;
                            if pkt.sequence_number as u32 > recv.highest_seq {
                                recv.highest_seq = pkt.sequence_number as u32;
                            }
                        }
                        {
                            let mut source = remote_ssrc.lock().await;
                            if *source == 0 {
                                *source = pkt.ssrc;
                            }
                        }
                        match depacketizer.depacketize(pkt.payload, pkt.marker) {
                            Ok(Some(_nal)) => {
                                {
                                    let mut s = stats.lock().await;
                                    s.frames_received = depacketizer.frames_received;
                                }
                                #[cfg(feature = "video-codec")]
                                if let Some(ref mut decoder) = decoder {
                                    if let Ok(Some(_frame)) = decoder.decode(&_nal) {
                                        stats.lock().await.frames_decoded += 1;
                                        #[cfg(any(target_os = "android", target_os = "windows"))]
                                        crate::calling::external_display::push_frame(
                                            _frame.width,
                                            _frame.height,
                                            _frame.data,
                                        );
                                    }
                                }
                            }
                            Ok(None) => {}
                            Err(error) => tracing::debug!("Video depacketize error: {error:#}"),
                        }
                    }
                }
            }
            Err(error) => {
                tracing::warn!("Video UDP recv error: {error:#}");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

async fn video_rtcp_receive_report_loop(
    socket: Arc<UdpSocket>,
    srtp_ctx: Arc<Mutex<srtp::SrtpContext>>,
    recv_stats: Arc<Mutex<rtcp::RtpRecvStats>>,
    remote_ssrc: Arc<Mutex<u32>>,
    dynamic_remote_addr: Arc<Mutex<SocketAddr>>,
    local_ssrc: u32,
    cname: String,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    interval.tick().await;
    loop {
        interval.tick().await;
        let recv = recv_stats.lock().await.clone();
        let source = *remote_ssrc.lock().await;
        let report = rtcp::build_receiver_report(local_ssrc, &recv, source, &cname);
        let encrypted = {
            let mut ctx = srtp_ctx.lock().await;
            srtp::protect_rtcp(&mut ctx, &report)
        };
        if let Ok(encrypted) = encrypted {
            let target = *dynamic_remote_addr.lock().await;
            if let Err(error) = socket.send_to(&encrypted, target).await {
                tracing::debug!("Video RTCP send failed: {error:#}");
            }
        }
    }
}
