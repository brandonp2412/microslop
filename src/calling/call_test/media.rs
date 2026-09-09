use super::*;

#[cfg(all(test, feature = "video-codec"))]
#[path = "media_tests.rs"]
mod tests;

pub(super) struct MediaLegSetup<'a> {
    pub local_audio_crypto_line: &'a str,
    pub local_video_crypto_line: &'a str,
    pub local_audio_ufrag: &'a str,
    pub local_audio_pwd: &'a str,
    pub local_video_ufrag: &'a str,
    pub local_video_pwd: &'a str,
    pub remote_sdp: &'a str,
    pub audio_socket: tokio::net::UdpSocket,
    pub video_socket: tokio::net::UdpSocket,
    pub label: &'a str,
    pub controlling: bool,
    pub audio_ssrc: u32,
    pub video_ssrc: u32,
}

pub(super) async fn setup_media_leg(setup: MediaLegSetup<'_>) -> Result<MediaLeg> {
    let MediaLegSetup {
        local_audio_crypto_line,
        local_video_crypto_line,
        local_audio_ufrag,
        local_audio_pwd,
        local_video_ufrag,
        local_video_pwd,
        remote_sdp,
        audio_socket,
        video_socket,
        label,
        controlling,
        audio_ssrc,
        video_ssrc,
    } = setup;
    // Decompress and log the full SDP for debugging
    let decompressed = crate::calling::sdp_compress::decompress_sdp(remote_sdp)
        .unwrap_or_else(|_| remote_sdp.to_string());
    tracing::info!(
        "[{}] Remote SDP ({} bytes):\n{}",
        label,
        decompressed.len(),
        decompressed
    );

    let remote_offer = sdp::parse_sdp_offer(remote_sdp)
        .with_context(|| format!("Failed to parse {} remote SDP", label))?;

    // Audio SRTP
    let local_material = srtp::parse_crypto_line(local_audio_crypto_line)?;
    let remote_material = remote_offer
        .crypto_lines
        .iter()
        .find_map(|l| srtp::parse_crypto_line(l).ok())
        .with_context(|| format!("No audio crypto line in {} remote SDP", label))?;
    let audio_srtp_ctx = Arc::new(Mutex::new(srtp::create_context(
        &local_material,
        &remote_material,
    )?));

    // Video SRTP
    let video_srtp_ctx = if let Some(ref vid) = remote_offer.video {
        let local_vid_material = srtp::parse_crypto_line(local_video_crypto_line)?;
        let remote_vid_material = vid
            .crypto_lines
            .iter()
            .find_map(|l| srtp::parse_crypto_line(l).ok())
            .with_context(|| format!("No video crypto line in {} remote SDP", label))?;
        Some(Arc::new(Mutex::new(srtp::create_context(
            &local_vid_material,
            &remote_vid_material,
        )?)))
    } else {
        tracing::info!("[{}] Remote SDP has no video section", label);
        None
    };

    // Audio ICE
    let remote_candidates = ice::parse_candidates_from_sdp(remote_sdp);
    let remote_creds = ice::IceCredentials {
        ufrag: remote_offer.ice_ufrag.clone(),
        pwd: remote_offer.ice_pwd.clone(),
    };
    let local_creds = ice::IceCredentials {
        ufrag: local_audio_ufrag.to_string(),
        pwd: local_audio_pwd.to_string(),
    };

    let audio_socket = Arc::new(audio_socket);
    let agent = ice::IceAgent::new(local_creds, remote_creds, controlling);
    let audio_remote_addr = match agent
        .check_connectivity(audio_socket.clone(), &remote_candidates)
        .await
    {
        Ok(result) => {
            tracing::info!(
                "[{}] Audio ICE succeeded: remote={}",
                label,
                result.remote_addr
            );
            result.remote_addr
        }
        Err(e) => {
            tracing::warn!("[{}] Audio ICE failed: {:#}, falling back", label, e);
            ice::select_remote_candidate(&remote_candidates)
                .with_context(|| format!("No fallback audio candidate for {}", label))?
        }
    };

    // Video ICE
    let video_socket = Arc::new(video_socket);
    let video_remote_addr = if let Some(ref vid) = remote_offer.video {
        let vid_candidates = ice::parse_main_video_candidates_from_sdp(remote_sdp);
        tracing::info!(
            "[{}] Video ICE: {} candidates from SDP, remote video ufrag={}, pwd_len={}",
            label,
            vid_candidates.len(),
            vid.ice_ufrag,
            vid.ice_pwd.len()
        );
        let vid_remote_creds = ice::IceCredentials {
            ufrag: vid.ice_ufrag.clone(),
            pwd: vid.ice_pwd.clone(),
        };
        let vid_local_creds = ice::IceCredentials {
            ufrag: local_video_ufrag.to_string(),
            pwd: local_video_pwd.to_string(),
        };
        let vid_agent = ice::IceAgent::new(vid_local_creds, vid_remote_creds, controlling);
        match vid_agent
            .check_connectivity(video_socket.clone(), &vid_candidates)
            .await
        {
            Ok(result) => {
                tracing::info!(
                    "[{}] Video ICE succeeded: remote={}",
                    label,
                    result.remote_addr
                );
                Some(result.remote_addr)
            }
            Err(e) => {
                tracing::warn!("[{}] Video ICE failed: {:#}, falling back", label, e);
                ice::select_remote_candidate(&vid_candidates)
            }
        }
    } else {
        None
    };

    Ok(MediaLeg {
        label: label.to_string(),
        audio_socket,
        audio_srtp_ctx,
        audio_remote_addr,
        audio_local_pwd: local_audio_pwd.to_string(),
        audio_ssrc,
        video_socket,
        video_srtp_ctx,
        video_remote_addr,
        video_local_crypto_line: local_video_crypto_line.to_string(),
        video_local_ufrag: local_video_ufrag.to_string(),
        video_local_pwd: local_video_pwd.to_string(),
        video_controlling: controlling,
        video_ssrc,
        #[cfg(feature = "video-codec")]
        camera_rx: None,
        #[cfg(feature = "video-capture")]
        display_tx: None,
        decode_remote_video: false,
    })
}

pub(super) async fn update_video_transport(
    handles: &MediaLegHandles,
    remote_sdp: &str,
) -> Result<()> {
    let remote_offer =
        sdp::parse_sdp_offer(remote_sdp).context("Failed to parse renegotiated remote SDP")?;
    let video = remote_offer
        .video
        .context("Renegotiated SDP has no video section")?;
    let remote_material = video
        .crypto_lines
        .iter()
        .find_map(|line| srtp::parse_crypto_line(line).ok())
        .context("Renegotiated video SDP has no SRTP crypto line")?;
    let local_material = srtp::parse_crypto_line(&handles.video_local_crypto_line)?;
    let context = srtp::create_context(&local_material, &remote_material)?;
    let remote_creds = ice::IceCredentials {
        ufrag: video.ice_ufrag,
        pwd: video.ice_pwd,
    };
    let local_creds = ice::IceCredentials {
        ufrag: handles.video_local_ufrag.clone(),
        pwd: handles.video_local_pwd.clone(),
    };
    let candidates = ice::parse_main_video_candidates_from_sdp(remote_sdp);
    let agent = ice::IceAgent::new(local_creds, remote_creds, handles.video_controlling);
    let remote_addr = match agent
        .check_connectivity(handles.video_socket.clone(), &candidates)
        .await
    {
        Ok(result) => result.remote_addr,
        Err(error) => {
            tracing::warn!("Renegotiated video ICE failed: {error:#}");
            ice::select_remote_candidate(&candidates)
                .context("Renegotiated video SDP has no usable candidate")?
        }
    };
    let srtp = handles
        .video_srtp_ctx
        .as_ref()
        .context("Video transport was not active before renegotiation")?;
    *srtp.lock().await = context;
    *handles.video_dynamic_remote_addr.lock().await = Some(remote_addr);
    tracing::info!("Renegotiated video transport applied: remote={remote_addr}");
    Ok(())
}

/// Spawn all media send/recv/rtcp loops for a single leg.
///
/// If `recorder` is Some, received audio is decoded and recorded (for echo detection).
/// If `loopback` is true, received audio is decoded and looped back as the send source
/// instead of generating a test tone — used on the incoming leg for echo verification.
/// Returns handles and shared stat counters.
pub(super) fn next_audio_frame(rx: &std::sync::mpsc::Receiver<Vec<i16>>) -> Option<Vec<i16>> {
    rx.try_recv().ok()
}

pub(super) struct MediaLegSpawn<'a> {
    pub leg: MediaLeg,
    pub cname: &'a str,
    pub recorder: Option<Arc<Mutex<test_tone::AudioRecorder>>>,
    pub loopback: bool,
    pub speaker_tx: Option<std::sync::mpsc::SyncSender<Vec<i16>>>,
    pub mic_rx: Option<std::sync::mpsc::Receiver<Vec<i16>>>,
    pub tone_mode: bool,
    pub progress: Option<tokio::sync::mpsc::UnboundedSender<CallProgress>>,
}

pub(super) fn spawn_media_leg(spawn: MediaLegSpawn<'_>) -> MediaLegHandles {
    let MediaLegSpawn {
        leg,
        cname,
        recorder,
        loopback,
        speaker_tx,
        mic_rx,
        tone_mode,
        progress,
    } = spawn;
    #[cfg(any(feature = "video-codec", feature = "video-capture"))]
    let mut leg = leg;
    let label = leg.label.clone();
    let mut handles = Vec::new();

    let ssrc = leg.audio_ssrc;

    let send_stats = Arc::new(Mutex::new(rtcp::RtpSendStats {
        ssrc,
        ..Default::default()
    }));
    let recv_stats = Arc::new(Mutex::new(rtcp::RtpRecvStats::default()));
    let remote_ssrc = Arc::new(Mutex::new(0u32));

    let video_ssrc = leg.video_ssrc;
    let video_initial_remote_addr = leg.video_remote_addr;
    let video_dynamic_remote_addr = Arc::new(Mutex::new(leg.video_remote_addr));
    let video_send_stats = Arc::new(Mutex::new(rtcp::RtpSendStats {
        ssrc: video_ssrc,
        ..Default::default()
    }));
    let video_recv_stats = Arc::new(Mutex::new(rtcp::RtpRecvStats::default()));
    let video_remote_ssrc = Arc::new(Mutex::new(0u32));
    let camera_frames_sent = Arc::new(AtomicU32::new(0));
    let raw_audio_datagrams = Arc::new(AtomicU32::new(0));
    let audio_stun_datagrams = Arc::new(AtomicU32::new(0));
    let audio_srtp_decrypt_failures = Arc::new(AtomicU32::new(0));
    let raw_video_datagrams = Arc::new(AtomicU32::new(0));
    let video_stun_datagrams = Arc::new(AtomicU32::new(0));
    let video_srtcp_datagrams = Arc::new(AtomicU32::new(0));
    let video_srtp_decrypt_failures = Arc::new(AtomicU32::new(0));
    let video_vsr_requests_sent = Arc::new(AtomicU32::new(0));
    let video_keyframe_requests_received = Arc::new(AtomicU32::new(0));
    let video_receiver_reports_for_local_ssrc = Arc::new(AtomicU32::new(0));
    let video_rr_fraction_lost = Arc::new(AtomicU32::new(0));
    let video_rr_cumulative_lost = Arc::new(std::sync::atomic::AtomicI32::new(0));
    let video_rr_extended_highest_seq = Arc::new(AtomicU32::new(0));
    let video_rr_jitter = Arc::new(AtomicU32::new(0));
    let video_first_sequence_sent = Arc::new(AtomicU32::new(0));
    let video_last_sequence_sent = Arc::new(AtomicU32::new(0));
    let video_force_keyframe = Arc::new(AtomicBool::new(false));
    let video_first_rtp_source = Arc::new(Mutex::new(None));
    let microphone_frames = Arc::new(AtomicU32::new(0));
    let microphone_non_silent_frames = Arc::new(AtomicU32::new(0));
    let microphone_peak = Arc::new(AtomicU32::new(0));

    let (loopback_tx, loopback_rx) = if loopback {
        let (tx, rx) = tokio::sync::mpsc::channel::<Vec<i16>>(64);
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

    // Dynamic remote address — updated when we receive STUN checks from a new peer.
    // In Teams, the MCU's actual relay address may differ from the SDP candidate.
    let audio_initial_remote_addr = leg.audio_remote_addr;
    let dynamic_remote_addr = Arc::new(Mutex::new(leg.audio_remote_addr));
    let first_rtp_source = Arc::new(Mutex::new(None));

    // Audio send loop
    {
        let socket = leg.audio_socket.clone();
        let srtp_ctx = leg.audio_srtp_ctx.clone();
        let send_stats = send_stats.clone();
        let dynamic_remote = dynamic_remote_addr.clone();
        let label = label.clone();
        let mic_frames = microphone_frames.clone();
        let mic_non_silent_frames = microphone_non_silent_frames.clone();
        let mic_peak = microphone_peak.clone();
        handles.push(tokio::spawn(async move {
            let mut tone = test_tone::ToneGenerator::new();
            let mut seq: u16 = 0;
            let mut timestamp: u32 = 0;
            let mut interval = tokio::time::interval(Duration::from_millis(20));
            let mut rtp_packet = Vec::with_capacity(rtp::RTP_HEADER_SIZE + rtp::SAMPLES_PER_PACKET);
            let mut loopback_rx = loopback_rx;
            let mic_rx = mic_rx;

            loop {
                interval.tick().await;

                let samples = if let Some(ref mut rx) = loopback_rx {
                    rx.try_recv().ok()
                } else if let Some(ref rx) = mic_rx {
                    if MICROPHONE_ENABLED.load(Ordering::Relaxed) {
                        next_audio_frame(rx).inspect(|samples| {
                            mic_frames.fetch_add(1, Ordering::Relaxed);
                            let peak = samples
                                .iter()
                                .map(|&sample| (sample as i32).unsigned_abs())
                                .max()
                                .unwrap_or(0);
                            mic_peak.fetch_max(peak, Ordering::Relaxed);
                            if peak >= 500 {
                                mic_non_silent_frames.fetch_add(1, Ordering::Relaxed);
                            }
                        })
                    } else {
                        while rx.try_recv().is_ok() {}
                        None
                    }
                } else if tone_mode {
                    Some(tone.next_frame())
                } else {
                    None
                };

                let mut encoded;
                let payload: &[u8] = if let Some(samples) = samples {
                    encoded = Vec::with_capacity(samples.len());
                    for &sample in &samples {
                        encoded.push(rtp::linear_to_ulaw(sample));
                    }
                    &encoded
                } else {
                    &rtp::SILENCE_PAYLOAD
                };
                rtp::encode_into(&mut rtp_packet, rtp::PT_PCMU, seq, timestamp, ssrc, payload);

                let srtp_packet = {
                    let mut ctx = srtp_ctx.lock().await;
                    match srtp::protect(&mut ctx, &rtp_packet) {
                        Ok(p) => p,
                        Err(e) => {
                            tracing::warn!("[{}] SRTP protect: {:#}", label, e);
                            continue;
                        }
                    }
                };

                let remote_addr = *dynamic_remote.lock().await;
                if socket.send_to(&srtp_packet, remote_addr).await.is_ok() {
                    let mut s = send_stats.lock().await;
                    s.packets_sent += 1;
                    s.bytes_sent += payload.len() as u32;
                    s.last_rtp_timestamp = timestamp;
                }

                seq = seq.wrapping_add(1);
                timestamp = timestamp.wrapping_add(160);
            }
        }));
    }

    // Audio recv loop
    {
        let socket = leg.audio_socket.clone();
        let srtp_ctx = leg.audio_srtp_ctx.clone();
        let recv_stats = recv_stats.clone();
        let remote_ssrc = remote_ssrc.clone();
        let recorder = recorder.clone();
        let loopback_tx = loopback_tx.clone();
        let local_pwd = leg.audio_local_pwd.clone();
        let label = label.clone();
        let dynamic_remote = dynamic_remote_addr.clone();
        let raw_datagrams = raw_audio_datagrams.clone();
        let stun_datagrams = audio_stun_datagrams.clone();
        let decrypt_failures = audio_srtp_decrypt_failures.clone();
        let first_rtp = first_rtp_source.clone();
        let progress = progress.clone();
        handles.push(tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            loop {
                match socket.recv_from(&mut buf).await {
                    Ok((len, from)) => {
                        raw_datagrams.fetch_add(1, Ordering::Relaxed);
                        let data = &buf[..len];

                        if len >= 20 && ice::is_stun_message(data) {
                            stun_datagrams.fetch_add(1, Ordering::Relaxed);
                            if ice::is_stun_request(data) {
                                let authenticated =
                                    ice::verify_message_integrity(data, local_pwd.as_bytes());
                                if let Some(txn_id) = ice::get_transaction_id(data) {
                                    let resp = ice::build_binding_response(
                                        &txn_id,
                                        from,
                                        Some(local_pwd.as_bytes()),
                                    );
                                    let _ = socket.send_to(&resp, from).await;
                                }
                                // The MCU's actual relay may differ from the SDP candidate, but
                                // only an authenticated ICE check is allowed to change the target.
                                if authenticated {
                                    let mut dr = dynamic_remote.lock().await;
                                    if *dr != from {
                                        tracing::info!("[{}] Updating remote addr: {} -> {} (authenticated peer ICE check)", label, *dr, from);
                                        *dr = from;
                                    }
                                } else {
                                    tracing::debug!("[{}] Ignoring unauthenticated STUN request from {} for media routing", label, from);
                                }
                            }
                            continue;
                        }

                        let srtcp_result = {
                            let mut ctx = srtp_ctx.lock().await;
                            srtp::unprotect_rtcp(&mut ctx, data).ok()
                        };
                        if let Some(rtcp_data) = srtcp_result {
                            tracing::debug!(
                                "[{}] Received SRTCP ({} bytes)",
                                label,
                                rtcp_data.len()
                            );
                            let blocks = rtcp::parse_rtcp(&rtcp_data);
                            let mut rs = recv_stats.lock().await;
                            for block in &blocks {
                                if let rtcp::RtcpBlock::SenderReport { ssrc, ntp_timestamp, sender_packet_count, sender_octet_count, .. } = block {
                                    tracing::info!(
                                        "[{}] RTCP SR from SSRC={:#010x}: pkt_count={} oct_count={}",
                                        label, ssrc, sender_packet_count, sender_octet_count
                                    );
                                    rs.last_sr_ntp = ((*ntp_timestamp >> 16) & 0xFFFF_FFFF) as u32;
                                    rs.last_sr_recv_time = Some(std::time::Instant::now());
                                }
                            }
                            continue;
                        }

                        let srtp_result = {
                            let mut ctx = srtp_ctx.lock().await;
                            srtp::unprotect(&mut ctx, data)
                        };
                        match srtp_result {
                            Ok(rtp_data) => {
                                if let Ok(pkt) = rtp::decode(&rtp_data) {
                                    let mut first = first_rtp.lock().await;
                                    if first.is_none() {
                                        *first = Some(from);
                                        tracing::info!("[{}] First inbound RTP from {}", label, from);
                                        if let Some(progress) = &progress {
                                            let _ = progress.send(CallProgress::Connected);
                                        }
                                    }
                                    drop(first);

                                    let mut rs = recv_stats.lock().await;
                                    rs.packets_received += 1;
                                    if pkt.sequence_number as u32 > rs.highest_seq {
                                        rs.highest_seq = pkt.sequence_number as u32;
                                    }
                                    drop(rs);

                                    let mut rssrc = remote_ssrc.lock().await;
                                    if *rssrc == 0 {
                                        *rssrc = pkt.ssrc;
                                        tracing::info!(
                                            "[{}] Remote audio SSRC: {:#010x}",
                                            label,
                                            pkt.ssrc
                                        );
                                    }
                                    drop(rssrc);

                                    // Dump raw PCMU payload to file (before decode)
                                    if let Ok(mut f) = std::fs::OpenOptions::new()
                                        .create(true).append(true)
                                        .open("/tmp/received_audio.ulaw")
                                    {
                                        use std::io::Write;
                                        let _ = f.write_all(pkt.payload);
                                    }

                                    // Decode PCMU to linear PCM
                                    let Some(samples) = rtp::decode_pcmu(&pkt) else {
                                        continue;
                                    };

                                    if let Some(ref rec) = recorder {
                                        let mut rec = rec.lock().await;
                                        rec.push_frame(&samples);
                                    }

                                    // Feed speaker output (non-blocking) unless locally muted.
                                    if SPEAKER_ENABLED.load(Ordering::Relaxed) {
                                        if let Some(ref tx) = speaker_tx {
                                            match tx.try_send(samples.clone()) {
                                                Ok(()) => {}
                                                Err(std::sync::mpsc::TrySendError::Full(_)) => {
                                                    tracing::debug!("[{}] Speaker channel full, dropping frame", label);
                                                }
                                                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                                                    tracing::warn!("[{}] Speaker channel disconnected!", label);
                                                }
                                            }
                                        }
                                    }

                                    if let Some(ref tx) = loopback_tx {
                                        let _ = tx.try_send(samples);
                                    }
                                }
                            }
                            Err(_) => {
                                decrypt_failures.fetch_add(1, Ordering::Relaxed);
                                tracing::trace!(
                                    "[{}] Cannot decrypt from {} ({} bytes)",
                                    label,
                                    from,
                                    len
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("[{}] UDP recv error: {:#}", label, e);
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        }));
    }

    {
        let socket = leg.audio_socket.clone();
        let srtp_ctx = leg.audio_srtp_ctx.clone();
        let send_stats = send_stats.clone();
        let recv_stats = recv_stats.clone();
        let remote_ssrc = remote_ssrc.clone();
        let dynamic_remote = dynamic_remote_addr.clone();
        let cname = cname.to_string();
        let label = label.clone();
        handles.push(tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            interval.tick().await;
            loop {
                interval.tick().await;
                let ss = send_stats.lock().await;
                let rs = recv_stats.lock().await;
                let rssrc = *remote_ssrc.lock().await;

                let rtcp_packet = if ss.packets_sent > 0 {
                    rtcp::build_sender_report(&ss, &rs, rssrc, &cname)
                } else {
                    rtcp::build_receiver_report(ss.ssrc, &rs, rssrc, &cname)
                };
                drop(ss);
                drop(rs);

                let mut ctx = srtp_ctx.lock().await;
                match srtp::protect_rtcp(&mut ctx, &rtcp_packet) {
                    Ok(srtcp) => {
                        let remote_addr = *dynamic_remote.lock().await;
                        if let Err(e) = socket.send_to(&srtcp, remote_addr).await {
                            tracing::warn!("[{}] Failed to send SRTCP: {:#}", label, e);
                        }
                    }
                    Err(e) => tracing::warn!("[{}] SRTCP protect failed: {:#}", label, e),
                }
            }
        }));
    }

    // Video send loop
    if let (Some(ref vid_srtp), Some(_)) = (&leg.video_srtp_ctx, leg.video_remote_addr) {
        let socket = leg.video_socket.clone();
        let vid_srtp = vid_srtp.clone();
        let vid_remote_addr = video_dynamic_remote_addr.clone();
        let vid_send_stats = video_send_stats.clone();
        let first_sequence_sent = video_first_sequence_sent.clone();
        let last_sequence_sent = video_last_sequence_sent.clone();
        #[cfg(feature = "video-codec")]
        let captured_camera_frames = camera_frames_sent.clone();
        #[cfg(feature = "video-codec")]
        let force_keyframe = video_force_keyframe.clone();
        let label = label.clone();

        #[cfg(feature = "video-codec")]
        let camera_rx = leg.camera_rx.take();
        #[cfg(not(feature = "video-codec"))]
        let camera_rx: Option<()> = None;

        handles.push(tokio::spawn(async move {
            let mut packetizer = video::VideoPacketizer::new(video_ssrc);
            let mut interval =
                tokio::time::interval(Duration::from_millis(video::FRAME_INTERVAL_MS));
            let mut traced_first_h264uc_frame = false;

            #[cfg(feature = "video-codec")]
            let mut encoder: Option<(u32, u32, codec::H264Encoder)> = None;
            #[cfg(feature = "video-codec")]
            let mut latest_camera_frame: Option<video::YuvFrame> = None;

            tracing::info!(
                "[{}] Video send loop started (SSRC: {:#010x}, camera: {})",
                label,
                video_ssrc,
                if camera_rx.is_some() { "live" } else { "black" },
            );

            loop {
                interval.tick().await;

                let nal_units = {
                    #[cfg(feature = "video-codec")]
                    {
                        if let Some(ref rx) = camera_rx {
                            if let Some(frame) = rx.try_iter().last() {
                                crate::calling::external_display::push_local_frame(
                                    frame.width,
                                    frame.height,
                                    frame.data.clone(),
                                );
                                latest_camera_frame = Some(frame);
                            }
                            match latest_camera_frame.as_ref() {
                                Some(frame) if frame.width > 0 => {
                                    if let (Ok(width), Ok(height)) =
                                        (u16::try_from(frame.width), u16::try_from(frame.height))
                                    {
                                        packetizer.set_frame_dimensions(width, height);
                                    }
                                    let replace = match encoder.as_ref() {
                                        Some((width, height, _)) => {
                                            *width != frame.width || *height != frame.height
                                        }
                                        None => true,
                                    };
                                    if replace {
                                        encoder = codec::H264Encoder::new(
                                            frame.width,
                                            frame.height,
                                            30.0,
                                            256,
                                        )
                                        .map(|encoder| (frame.width, frame.height, encoder))
                                        .inspect_err(|error| {
                                            tracing::warn!("[{}] Camera encoder initialization failed: {:#}", label, error);
                                        })
                                        .ok();
                                    }
                                    match encoder.as_mut() {
                                        Some((_, _, encoder)) => {
                                            if force_keyframe.swap(false, Ordering::Relaxed) {
                                                encoder.force_intra_frame();
                                            }
                                            match encoder.encode(&frame.data) {
                                                Ok(nals) => {
                                                    if !nals.is_empty() {
                                                        captured_camera_frames
                                                            .fetch_add(1, Ordering::Relaxed);
                                                    }
                                                    nals
                                                }
                                                Err(error) => {
                                                    tracing::warn!("[{}] Camera encode failed: {:#}", label, error);
                                                    encoder.force_intra_frame();
                                                    Vec::new()
                                                }
                                            }
                                        }
                                        None => Vec::new(),
                                    }
                                }
                                _ => Vec::new(),
                            }
                        } else {
                            video::generate_black_iframe()
                        }
                    }
                    #[cfg(not(feature = "video-codec"))]
                    {
                        let _ = &camera_rx;
                        video::generate_black_iframe()
                    }
                };

                let rtp_packets = packetizer.packetize_frame(&nal_units);

                if !traced_first_h264uc_frame && !rtp_packets.is_empty() {
                    traced_first_h264uc_frame = true;
                    let nal_summary = nal_units
                        .iter()
                        .filter_map(|nal| nal.first().map(|header| (header & 0x1f, nal.len())))
                        .map(|(nal_type, len)| format!("{nal_type}:{len}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    let first = &rtp_packets[0];
                    let last = rtp_packets.last().unwrap();
                    let seq_start = u16::from_be_bytes([first[2], first[3]]);
                    let seq_end = u16::from_be_bytes([last[2], last[3]]);
                    let timestamp = u32::from_be_bytes([first[4], first[5], first[6], first[7]]);
                    let pacsi_header = first
                        .get(rtp::RTP_HEADER_SIZE)
                        .copied()
                        .unwrap_or_default();
                    tracing::info!(
                        "[{}] First H264UC access unit: nals=[{}] packets={} seq={}..{} ts={} pacsi=0x{:02x} marker={}",
                        label,
                        nal_summary,
                        rtp_packets.len(),
                        seq_start,
                        seq_end,
                        timestamp,
                        pacsi_header,
                        last[1] & 0x80 != 0,
                    );
                    if let Ok(path) = std::env::var("MICROSLOP_CALL_TRACE_PATH") {
                        if let Ok(mut file) = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(path)
                        {
                            use std::io::Write as _;
                            let _ = writeln!(
                                file,
                                "# h264uc first_access_unit nals=[{}] packets={} seq={}..{} ts={} pacsi=0x{:02x} marker={}",
                                nal_summary,
                                rtp_packets.len(),
                                seq_start,
                                seq_end,
                                timestamp,
                                pacsi_header,
                                last[1] & 0x80 != 0,
                            );
                        }
                    }
                }

                for rtp_pkt in &rtp_packets {
                    let payload_len = rtp_pkt.len().saturating_sub(rtp::RTP_HEADER_SIZE);
                    let last_ts = if rtp_pkt.len() >= 8 {
                        u32::from_be_bytes([rtp_pkt[4], rtp_pkt[5], rtp_pkt[6], rtp_pkt[7]])
                    } else {
                        0
                    };
                    let srtp_packet = {
                        let mut ctx = vid_srtp.lock().await;
                        match srtp::protect(&mut ctx, rtp_pkt) {
                            Ok(p) => p,
                            Err(e) => {
                                tracing::warn!("[{}] Video SRTP protect: {:#}", label, e);
                                continue;
                            }
                        }
                    };
                    if let Some(vid_addr) = *vid_remote_addr.lock().await {
                        if socket.send_to(&srtp_packet, vid_addr).await.is_ok() {
                            let sequence = u16::from_be_bytes([rtp_pkt[2], rtp_pkt[3]]) as u32;
                            let _ = first_sequence_sent.compare_exchange(
                                0,
                                sequence,
                                Ordering::Relaxed,
                                Ordering::Relaxed,
                            );
                            last_sequence_sent.store(sequence, Ordering::Relaxed);
                            let mut s = vid_send_stats.lock().await;
                            s.packets_sent += 1;
                            s.bytes_sent += payload_len as u32;
                            s.last_rtp_timestamp = last_ts;
                        }
                    }
                }
            }
        }));
    }

    // Video recv loop
    if let (Some(ref vid_srtp), Some(_)) = (&leg.video_srtp_ctx, leg.video_remote_addr) {
        let socket = leg.video_socket.clone();
        let vid_srtp = vid_srtp.clone();
        let vid_recv_stats = video_recv_stats.clone();
        let vid_remote_ssrc = video_remote_ssrc.clone();
        let vid_local_pwd = leg.video_local_pwd.clone();
        let vid_dynamic_remote = video_dynamic_remote_addr.clone();
        let raw_datagrams = raw_video_datagrams.clone();
        let stun_datagrams = video_stun_datagrams.clone();
        let srtcp_datagrams = video_srtcp_datagrams.clone();
        let decrypt_failures = video_srtp_decrypt_failures.clone();
        let first_rtp = video_first_rtp_source.clone();
        let force_keyframe = video_force_keyframe.clone();
        let keyframe_requests = video_keyframe_requests_received.clone();
        let receiver_reports_for_local_ssrc = video_receiver_reports_for_local_ssrc.clone();
        let rr_fraction_lost = video_rr_fraction_lost.clone();
        let rr_cumulative_lost = video_rr_cumulative_lost.clone();
        let rr_extended_highest_seq = video_rr_extended_highest_seq.clone();
        let rr_jitter = video_rr_jitter.clone();
        let label = label.clone();

        #[cfg(feature = "video-capture")]
        let display_tx = leg.display_tx.take();
        #[cfg(feature = "video-codec")]
        let decode_remote_video = leg.decode_remote_video;

        handles.push(tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            let mut depacketizer = video::VideoDepacketizer::new();

            #[cfg(feature = "video-codec")]
            let mut decoder = decode_remote_video
                .then(codec::H264Decoder::new)
                .transpose()
                .ok()
                .flatten();

            loop {
                match socket.recv_from(&mut buf).await {
                    Ok((len, from)) => {
                        raw_datagrams.fetch_add(1, Ordering::Relaxed);
                        let data = &buf[..len];

                        if len >= 20 && ice::is_stun_message(data) {
                            stun_datagrams.fetch_add(1, Ordering::Relaxed);
                            if ice::is_stun_request(data) {
                                let authenticated =
                                    ice::verify_message_integrity(data, vid_local_pwd.as_bytes());
                                if let Some(txn_id) = ice::get_transaction_id(data) {
                                    let resp = ice::build_binding_response(
                                        &txn_id,
                                        from,
                                        Some(vid_local_pwd.as_bytes()),
                                    );
                                    let _ = socket.send_to(&resp, from).await;
                                }
                                if authenticated {
                                    let mut target = vid_dynamic_remote.lock().await;
                                    if *target != Some(from) {
                                        tracing::info!(
                                            "[{}] Updating video remote addr: {:?} -> {} (authenticated peer ICE check)",
                                            label,
                                            *target,
                                            from
                                        );
                                        *target = Some(from);
                                    }
                                } else {
                                    tracing::debug!(
                                        "[{}] Ignoring unauthenticated STUN request from {} for video routing",
                                        label,
                                        from
                                    );
                                }
                            }
                            continue;
                        }

                        let srtcp_result = {
                            let mut ctx = vid_srtp.lock().await;
                            srtp::unprotect_rtcp(&mut ctx, data).ok()
                        };
                        if let Some(rtcp_data) = srtcp_result {
                            srtcp_datagrams.fetch_add(1, Ordering::Relaxed);
                            tracing::debug!(
                                "[{}] Received video SRTCP ({} bytes)",
                                label,
                                rtcp_data.len()
                            );
                            if let Ok(path) = std::env::var("MICROSLOP_CALL_TRACE_PATH") {
                                if let Ok(mut file) = std::fs::OpenOptions::new()
                                    .create(true)
                                    .append(true)
                                    .open(path)
                                {
                                    use std::io::Write as _;
                                    let hex = rtcp_data
                                        .iter()
                                        .map(|byte| format!("{byte:02x}"))
                                        .collect::<String>();
                                    let _ = writeln!(
                                        file,
                                        "# video_rtcp source={} bytes={} hex={}",
                                        from,
                                        rtcp_data.len(),
                                        hex,
                                    );
                                }
                            }
                            if let Some(report) =
                                rtcp::find_receiver_report_block(&rtcp_data, video_ssrc)
                            {
                                rr_fraction_lost
                                    .store(report.fraction_lost as u32, Ordering::Relaxed);
                                rr_cumulative_lost
                                    .store(report.cumulative_lost, Ordering::Relaxed);
                                rr_extended_highest_seq.store(
                                    report.extended_highest_sequence_number,
                                    Ordering::Relaxed,
                                );
                                rr_jitter.store(report.interarrival_jitter, Ordering::Relaxed);
                                tracing::info!(
                                    "[{}] Remote video RR metrics for our SSRC {:#010x}: fraction_lost={}/256 cumulative_lost={} highest_seq={} jitter={}",
                                    label,
                                    video_ssrc,
                                    report.fraction_lost,
                                    report.cumulative_lost,
                                    report.extended_highest_sequence_number,
                                    report.interarrival_jitter,
                                );
                            }
                            let blocks = rtcp::parse_rtcp(&rtcp_data);
                            let mut rs = vid_recv_stats.lock().await;
                            for block in &blocks {
                                if let rtcp::RtcpBlock::SenderReport { ntp_timestamp, .. } = block {
                                    rs.last_sr_ntp = ((*ntp_timestamp >> 16) & 0xFFFF_FFFF) as u32;
                                    rs.last_sr_recv_time = Some(std::time::Instant::now());
                                }
                            }
                            drop(rs);

                            for block in &blocks {
                                if let rtcp::RtcpBlock::ReceiverReport {
                                    ssrc: reporter_ssrc,
                                    report_ssrcs,
                                } = block
                                {
                                    if report_ssrcs.contains(&video_ssrc) {
                                        let count = receiver_reports_for_local_ssrc
                                            .fetch_add(1, Ordering::Relaxed)
                                            + 1;
                                        tracing::info!(
                                            "[{}] Remote video RR #{} reports our SSRC {:#010x} (reporter={:#010x})",
                                            label,
                                            count,
                                            video_ssrc,
                                            reporter_ssrc,
                                        );
                                        if let Ok(path) = std::env::var("MICROSLOP_CALL_TRACE_PATH") {
                                            if let Ok(mut file) = std::fs::OpenOptions::new()
                                                .create(true)
                                                .append(true)
                                                .open(path)
                                            {
                                                use std::io::Write as _;
                                                let _ = writeln!(
                                                    file,
                                                    "# video_receiver_report local_ssrc={:#010x} reporter_ssrc={:#010x} count={}",
                                                    video_ssrc,
                                                    reporter_ssrc,
                                                    count,
                                                );
                                            }
                                        }
                                    }
                                }
                                let requested = match block {
                                    rtcp::RtcpBlock::PictureLossIndication {
                                        sender_ssrc,
                                        media_ssrc,
                                    } => {
                                        tracing::info!(
                                            "[{}] Remote video PLI: sender_ssrc={:#010x} media_ssrc={:#010x}",
                                            label,
                                            sender_ssrc,
                                            media_ssrc,
                                        );
                                        true
                                    }
                                    rtcp::RtcpBlock::VideoSourceRequest {
                                        sender_ssrc,
                                        requested_msi,
                                        request_id,
                                        keyframe_requested,
                                    } => {
                                        tracing::info!(
                                            "[{}] Remote video VSR: sender_ssrc={:#010x} requested_msi={:#010x} request_id={} keyframe={}",
                                            label,
                                            sender_ssrc,
                                            requested_msi,
                                            request_id,
                                            keyframe_requested,
                                        );
                                        *keyframe_requested
                                    }
                                    rtcp::RtcpBlock::Unknown(pt) => {
                                        tracing::debug!(
                                            "[{}] Unhandled remote video RTCP packet type {}",
                                            label,
                                            pt,
                                        );
                                        false
                                    }
                                    _ => false,
                                };
                                if requested {
                                    keyframe_requests.fetch_add(1, Ordering::Relaxed);
                                    force_keyframe.store(true, Ordering::Relaxed);
                                }
                            }
                            continue;
                        }

                        let result = {
                            let mut ctx = vid_srtp.lock().await;
                            srtp::unprotect(&mut ctx, data)
                        };
                        match result {
                            Ok(rtp_data) => {
                                if let Ok(pkt) = rtp::decode(&rtp_data) {
                                    if pkt.payload_type != video::PT_H264 {
                                        continue;
                                    }
                                    let mut first = first_rtp.lock().await;
                                    if first.is_none() {
                                        *first = Some(from);
                                        tracing::info!("[{}] First inbound video RTP from {}", label, from);
                                    }
                                    drop(first);

                                    let mut rs = vid_recv_stats.lock().await;
                                    rs.packets_received += 1;
                                    if pkt.sequence_number as u32 > rs.highest_seq {
                                        rs.highest_seq = pkt.sequence_number as u32;
                                    }
                                    drop(rs);

                                    let mut rssrc = vid_remote_ssrc.lock().await;
                                    if *rssrc == 0 {
                                        *rssrc = pkt.ssrc;
                                        tracing::info!(
                                            "[{}] Remote video SSRC: {:#010x}",
                                            label,
                                            pkt.ssrc
                                        );
                                    }
                                    drop(rssrc);

                                    let marker = pkt.marker;
                                    match depacketizer.depacketize(pkt.payload, marker) {
                                        Ok(Some(_nal)) =>
                                        {
                                            #[cfg(feature = "video-codec")]
                                            if let Some(ref mut dec) = decoder {
                                                match dec.decode(&_nal) {
                                                    Ok(Some(frame)) => {
                                                        #[cfg(any(
                                                            target_os = "android",
                                                            target_os = "windows"
                                                        ))]
                                                        crate::calling::external_display::push_frame(
                                                            frame.width,
                                                            frame.height,
                                                            frame.data,
                                                        );
                                                        #[cfg(feature = "video-capture")]
                                                        if let Some(ref tx) = display_tx {
                                                            let _ = tx.try_send(
                                                                display::DisplayFrame {
                                                                    width: frame.width,
                                                                    height: frame.height,
                                                                    data: frame.data,
                                                                },
                                                            );
                                                        }
                                                    }
                                                    Ok(None) => {}
                                                    Err(e) => {
                                                        tracing::debug!(
                                                            "[{}] Decode error: {:#}",
                                                            label,
                                                            e
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                        Ok(None) => {}
                                        Err(e) => {
                                            tracing::debug!(
                                                "[{}] Depacketize error: {:#}",
                                                label,
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                            Err(_) => {
                                decrypt_failures.fetch_add(1, Ordering::Relaxed);
                                tracing::trace!(
                                    "[{}] Cannot decrypt video from {} ({} bytes)",
                                    label,
                                    from,
                                    len
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("[{}] Video UDP recv error: {:#}", label, e);
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        }));
    }

    if let (Some(ref vid_srtp), Some(_)) = (&leg.video_srtp_ctx, leg.video_remote_addr) {
        let socket = leg.video_socket.clone();
        let vid_srtp = vid_srtp.clone();
        let vid_remote_addr = video_dynamic_remote_addr.clone();
        let requests_sent = video_vsr_requests_sent.clone();
        let label = label.clone();
        handles.push(tokio::spawn(async move {
            let request_id = (rtcp::ntp_timestamp() & 0xffff) as u16;
            for attempt in 0..10 {
                if attempt > 0 {
                    let delay = if attempt <= 4 {
                        Duration::from_millis(190)
                    } else {
                        Duration::from_secs(3)
                    };
                    tokio::time::sleep(delay).await;
                }

                let request = ost_microsoft::calling::video_source_request(video_ssrc, request_id);
                let protected = {
                    let mut ctx = vid_srtp.lock().await;
                    srtp::protect_rtcp(&mut ctx, &request)
                };
                match protected {
                    Ok(srtcp) => {
                        if let Some(vid_addr) = *vid_remote_addr.lock().await {
                            match socket.send_to(&srtcp, vid_addr).await {
                                Ok(_) => {
                                    requests_sent.fetch_add(1, Ordering::Relaxed);
                                    tracing::debug!(
                                        "[{}] Sent video source request {}/10 to {}",
                                        label,
                                        attempt + 1,
                                        vid_addr
                                    );
                                }
                                Err(e) => tracing::warn!(
                                    "[{}] Failed to send video source request: {:#}",
                                    label,
                                    e
                                ),
                            }
                        }
                    }
                    Err(e) => tracing::warn!(
                        "[{}] Video source request SRTCP protect failed: {:#}",
                        label,
                        e
                    ),
                }
            }
        }));
    }

    // Video RTCP send loop
    if let (Some(ref vid_srtp), Some(_)) = (&leg.video_srtp_ctx, leg.video_remote_addr) {
        let socket = leg.video_socket.clone();
        let vid_srtp = vid_srtp.clone();
        let vid_remote_addr = video_dynamic_remote_addr.clone();
        let vid_send_stats = video_send_stats.clone();
        let vid_recv_stats = video_recv_stats.clone();
        let vid_remote_ssrc = video_remote_ssrc.clone();
        let cname = cname.to_string();
        let label = label.clone();
        handles.push(tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            interval.tick().await;
            loop {
                interval.tick().await;
                let ss = vid_send_stats.lock().await;
                let rs = vid_recv_stats.lock().await;
                let rssrc = *vid_remote_ssrc.lock().await;

                let rtcp_packet = if ss.packets_sent > 0 {
                    rtcp::build_sender_report(&ss, &rs, rssrc, &cname)
                } else {
                    rtcp::build_receiver_report(ss.ssrc, &rs, rssrc, &cname)
                };
                drop(ss);
                drop(rs);

                let mut ctx = vid_srtp.lock().await;
                match srtp::protect_rtcp(&mut ctx, &rtcp_packet) {
                    Ok(srtcp) => {
                        if let Some(vid_addr) = *vid_remote_addr.lock().await {
                            if let Err(e) = socket.send_to(&srtcp, vid_addr).await {
                                tracing::warn!("[{}] Failed to send video SRTCP: {:#}", label, e);
                            }
                        }
                    }
                    Err(e) => tracing::warn!("[{}] Video SRTCP protect failed: {:#}", label, e),
                }
            }
        }));
    }

    MediaLegHandles {
        send_stats,
        recv_stats,
        video_send_stats,
        video_recv_stats,
        video_socket: leg.video_socket,
        video_srtp_ctx: leg.video_srtp_ctx,
        video_dynamic_remote_addr,
        video_local_crypto_line: leg.video_local_crypto_line,
        video_local_ufrag: leg.video_local_ufrag,
        video_local_pwd: leg.video_local_pwd,
        video_controlling: leg.video_controlling,
        camera_frames_sent,
        raw_audio_datagrams,
        audio_stun_datagrams,
        audio_srtp_decrypt_failures,
        audio_initial_remote_addr,
        audio_dynamic_remote_addr: dynamic_remote_addr,
        audio_first_rtp_source: first_rtp_source,
        raw_video_datagrams,
        video_stun_datagrams,
        video_srtcp_datagrams,
        video_srtp_decrypt_failures,
        video_vsr_requests_sent,
        video_keyframe_requests_received,
        video_receiver_reports_for_local_ssrc,
        video_rr_fraction_lost,
        video_rr_cumulative_lost,
        video_rr_extended_highest_seq,
        video_rr_jitter,
        video_first_sequence_sent,
        video_last_sequence_sent,
        video_initial_remote_addr,
        video_first_rtp_source,
        microphone_frames,
        microphone_non_silent_frames,
        microphone_peak,
        handles,
    }
}
