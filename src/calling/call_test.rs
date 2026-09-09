//! Outgoing call test — places a call to the av-test channel via two-phase
//! conversation API (epconv + conversationController).

use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc,
};
use std::time::Duration;

use anyhow::{Context, Result};
use base64::Engine;
use tokio::sync::Mutex;

use crate::auth::TokenStore;
#[cfg(any(feature = "video-capture", feature = "video-capture-windows"))]
use crate::calling::camera;
#[cfg(feature = "video-codec")]
use crate::calling::codec;
#[cfg(feature = "video-capture")]
use crate::calling::display;
#[cfg(all(feature = "video-codec", target_os = "android"))]
use crate::calling::external_camera;
use crate::calling::{ice, recording, rtcp, rtp, sdp, signaling, srtp, test_tone, video};
use crate::config::Config;
use crate::trouter::{registrar, session, websocket};

mod acceptance;
mod media;
mod support;

pub use acceptance::extract_call_payload;
use acceptance::{
    await_with_call_signaling, end_call_by_url, extract_callee_oid_from_thread,
    extract_mri_from_skype_token, wait_for_call_acceptance, wait_for_call_end,
};
use media::{
    setup_media_leg, spawn_media_leg, update_video_transport, MediaLegSetup, MediaLegSpawn,
};
use support::{derive_epconv_url, fetch_me, CallAcceptanceResponse, MediaLeg, MediaLegHandles};

#[cfg(test)]
mod tests {
    use super::{callee_mri_for_thread, caller_id_for_mri, media::next_audio_frame};

    #[test]
    fn microphone_callback_batches_preserve_every_audio_frame() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(vec![1]).unwrap();
        tx.send(vec![2]).unwrap();

        assert_eq!(next_audio_frame(&rx), Some(vec![1]));
        assert_eq!(next_audio_frame(&rx), Some(vec![2]));
        assert_eq!(next_audio_frame(&rx), None);
    }

    #[test]
    fn thread_peer_identity_wins_over_stale_cached_identity() {
        assert_eq!(
            callee_mri_for_thread(
                "19:caller_thread-peer@unq.gbl.spaces",
                "caller",
                Some("stale-peer"),
            ),
            Some("8:orgid:thread-peer".to_owned()),
        );
    }

    #[test]
    fn caller_identity_supports_work_and_personal_accounts() {
        assert_eq!(caller_id_for_mri("8:orgid:abc"), Some("abc"));
        assert_eq!(caller_id_for_mri("8:live:testuser"), Some("live:testuser"));
    }

    #[test]
    fn cached_identity_is_used_when_thread_does_not_identify_the_caller() {
        assert_eq!(
            callee_mri_for_thread(
                "19:other-one_other-two@unq.gbl.spaces",
                "caller",
                Some("real-peer"),
            ),
            Some("8:orgid:real-peer".to_owned()),
        );
    }
}

#[derive(Debug)]
pub struct CallTestResult {
    pub call_placed: bool,
    pub call_accepted: bool,
    pub rejection_reason: Option<String>,
    pub packets_sent: u32,
    pub packets_received: u32,
    pub raw_audio_datagrams: u32,
    pub audio_stun_datagrams: u32,
    pub audio_srtp_decrypt_failures: u32,
    pub audio_initial_remote_addr: Option<std::net::SocketAddr>,
    pub audio_final_remote_addr: Option<std::net::SocketAddr>,
    pub audio_first_rtp_source: Option<std::net::SocketAddr>,
    pub microphone_frames: u32,
    pub microphone_non_silent_frames: u32,
    pub microphone_peak: u32,
    pub microphone_frames_produced: u32,
    pub microphone_frames_enqueued: u32,
    pub microphone_frames_dropped: u32,
    pub microphone_receiver_disconnects: u32,
    pub microphone_stream_errors: u32,
    pub speaker_frames_received: u32,
    pub speaker_samples_rendered: u32,
    pub speaker_stream_errors: u32,
    pub video_packets_sent: u32,
    pub video_packets_received: u32,
    pub raw_video_datagrams: u32,
    pub video_stun_datagrams: u32,
    pub video_srtcp_datagrams: u32,
    pub video_srtp_decrypt_failures: u32,
    pub video_vsr_requests_sent: u32,
    pub video_keyframe_requests_received: u32,
    pub video_receiver_reports_for_local_ssrc: u32,
    pub video_rr_fraction_lost: u32,
    pub video_rr_cumulative_lost: i32,
    pub video_rr_extended_highest_seq: u32,
    pub video_rr_jitter: u32,
    pub video_first_sequence_sent: u32,
    pub video_last_sequence_sent: u32,
    pub video_initial_remote_addr: Option<std::net::SocketAddr>,
    pub video_final_remote_addr: Option<std::net::SocketAddr>,
    pub video_first_rtp_source: Option<std::net::SocketAddr>,
    pub camera_frames_sent: u32,
    pub incoming_audio_pkts_sent: u32,
    pub incoming_audio_pkts_recv: u32,
    pub incoming_video_pkts_sent: u32,
    pub incoming_video_pkts_recv: u32,
    pub echo_detected: bool,
    pub echo_delay_ms: f64,
    pub echo_correlation: f64,
}

impl CallTestResult {
    fn failed(rejection_reason: Option<String>) -> Self {
        Self {
            call_placed: true,
            call_accepted: false,
            rejection_reason,
            packets_sent: 0,
            packets_received: 0,
            raw_audio_datagrams: 0,
            audio_stun_datagrams: 0,
            audio_srtp_decrypt_failures: 0,
            audio_initial_remote_addr: None,
            audio_final_remote_addr: None,
            audio_first_rtp_source: None,
            microphone_frames: 0,
            microphone_non_silent_frames: 0,
            microphone_peak: 0,
            microphone_frames_produced: 0,
            microphone_frames_enqueued: 0,
            microphone_frames_dropped: 0,
            microphone_receiver_disconnects: 0,
            microphone_stream_errors: 0,
            speaker_frames_received: 0,
            speaker_samples_rendered: 0,
            speaker_stream_errors: 0,
            video_packets_sent: 0,
            video_packets_received: 0,
            raw_video_datagrams: 0,
            video_stun_datagrams: 0,
            video_srtcp_datagrams: 0,
            video_srtp_decrypt_failures: 0,
            video_vsr_requests_sent: 0,
            video_keyframe_requests_received: 0,
            video_receiver_reports_for_local_ssrc: 0,
            video_rr_fraction_lost: 0,
            video_rr_cumulative_lost: 0,
            video_rr_extended_highest_seq: 0,
            video_rr_jitter: 0,
            video_first_sequence_sent: 0,
            video_last_sequence_sent: 0,
            video_initial_remote_addr: None,
            video_final_remote_addr: None,
            video_first_rtp_source: None,
            camera_frames_sent: 0,
            incoming_audio_pkts_sent: 0,
            incoming_audio_pkts_recv: 0,
            incoming_video_pkts_sent: 0,
            incoming_video_pkts_recv: 0,
            echo_detected: false,
            echo_delay_ms: 0.0,
            echo_correlation: 0.0,
        }
    }
}

/// Run an outgoing call test.
///
/// - `echo=true`: Call the Echo / Call Quality Tester bot
/// - `thread_override=Some(id)`: Call a specific 1:1 chat thread
/// - Otherwise: Call the av-test channel (requires TEAMS_AV_TEST_THREAD_ID env var)
#[derive(Clone, Debug)]
pub struct CallTestOptions {
    pub duration_secs: u64,
    pub record: bool,
    pub echo: bool,
    pub thread_override: Option<String>,
    pub callee_user_id: Option<String>,
    pub use_camera: bool,
    pub use_display: bool,
    pub tone_mode: bool,
}

pub async fn run_call_test(options: CallTestOptions) -> Result<CallTestResult> {
    let config = Config::load().context("Failed to load config")?;
    run_call_test_inner(config, options, None, None).await
}

#[derive(Clone, Debug)]
pub enum CallProgress {
    Dialing,
    Ringing,
    Connected,
}

static MICROPHONE_ENABLED: AtomicBool = AtomicBool::new(true);
static SPEAKER_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_microphone_enabled(enabled: bool) {
    MICROPHONE_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn set_speaker_enabled(enabled: bool) {
    SPEAKER_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn microphone_enabled() -> bool {
    MICROPHONE_ENABLED.load(Ordering::Relaxed)
}

pub fn speaker_enabled() -> bool {
    SPEAKER_ENABLED.load(Ordering::Relaxed)
}

pub fn reset_media_controls() {
    set_microphone_enabled(true);
    set_speaker_enabled(true);
}

fn caller_id_for_mri(mri: &str) -> Option<&str> {
    mri.strip_prefix("8:orgid:")
        .or_else(|| mri.strip_prefix("8:"))
        .filter(|id| !id.is_empty())
}

#[cfg(feature = "consumer-webrtc")]
fn synthetic_video_frame(width: usize, height: usize, frame_index: usize) -> Vec<u8> {
    let y_size = width * height;
    let uv_size = (width / 2) * (height / 2);
    let mut frame = vec![128; y_size + uv_size * 2];
    let moving_x = (frame_index * 7) % width.max(1);
    for y in 0..height {
        for x in 0..width {
            let checker = ((x / 32) + (y / 32)) % 2 == 0;
            let in_bar = x.abs_diff(moving_x) < 10;
            frame[y * width + x] = if in_bar {
                235
            } else if checker {
                48
            } else {
                176
            };
        }
    }
    frame
}

fn callee_mri_for_thread(
    thread_id: &str,
    caller_oid: &str,
    callee_user_id: Option<&str>,
) -> Option<String> {
    let id = extract_callee_oid_from_thread(thread_id, caller_oid).or_else(|| {
        callee_user_id
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(ToOwned::to_owned)
    })?;
    Some(if id.contains(':') {
        id
    } else {
        format!("8:orgid:{id}")
    })
}

pub async fn run_call_until_stop(
    options: CallTestOptions,
    stop: tokio::sync::watch::Receiver<bool>,
    progress: tokio::sync::mpsc::UnboundedSender<CallProgress>,
) -> Result<CallTestResult> {
    let config = Config::load().context("Failed to load config")?;
    run_call_until_stop_with_config(config, options, stop, progress).await
}

pub async fn run_call_until_stop_with_config(
    config: Config,
    mut options: CallTestOptions,
    stop: tokio::sync::watch::Receiver<bool>,
    progress: tokio::sync::mpsc::UnboundedSender<CallProgress>,
) -> Result<CallTestResult> {
    let is_test_call = options.thread_override.as_deref() == Some("__microslop_test_call__");
    options.duration_secs = if is_test_call { 90 } else { 24 * 60 * 60 };
    options.echo = is_test_call;
    if is_test_call {
        options.thread_override = None;
    }
    run_call_test_inner(config, options, Some(stop), Some(progress)).await
}

async fn run_call_test_inner(
    config: Config,
    options: CallTestOptions,
    mut stop: Option<tokio::sync::watch::Receiver<bool>>,
    progress: Option<tokio::sync::mpsc::UnboundedSender<CallProgress>>,
) -> Result<CallTestResult> {
    let CallTestOptions {
        duration_secs,
        record,
        echo,
        thread_override,
        callee_user_id,
        use_camera,
        use_display,
        tone_mode,
    } = options;
    if let Some(progress) = &progress {
        let _ = progress.send(CallProgress::Dialing);
    }
    let skype_token = config
        .get_skype_token()
        .context("No skype token. Run `teams-cli login` first.")?;
    anyhow::ensure!(
        !skype_token.is_expired(),
        "Skype token expired. Run `teams-cli login`."
    );
    let skype_token_str = &skype_token.token;

    let teams_access_token = config
        .get_access_token()
        .context("No Teams access token. Run `teams-cli login` first.")?;
    anyhow::ensure!(
        !teams_access_token.is_expired(),
        "Teams access token expired. Run `teams-cli login`."
    );

    let ic3_token = if config.personal {
        None
    } else {
        let token = config
            .get_ic3_token()
            .context("No IC3 token. Run `teams-cli login` first.")?;
        anyhow::ensure!(
            !token.is_expired(),
            "IC3 token expired. Run `teams-cli login`."
        );
        Some(token)
    };
    let call_token_str = ic3_token
        .as_ref()
        .map(|token| token.token.as_str())
        .unwrap_or(skype_token_str);

    let recorder_token_str = if record {
        let recorder_token = config
            .get_recorder_token()
            .context("No recorder token. Run `teams-cli login` first.")?;
        anyhow::ensure!(
            !recorder_token.is_expired(),
            "Recorder token expired. Run `teams-cli login`."
        );
        Some(recorder_token.token)
    } else {
        None
    };

    let graph_token = config.get_graph_token();
    let region_gtms = config
        .get_region_gtms()
        .context("No region_gtms in config. Run `teams-cli login` first.")?;
    let http = reqwest::Client::new();

    let caller_mri = extract_mri_from_skype_token(skype_token_str)
        .context("Cannot extract MRI from skype token")?;
    tracing::info!("Caller MRI: {}", caller_mri);

    let caller_oid = caller_id_for_mri(&caller_mri)
        .context("Caller MRI does not have the expected 8: prefix")?;

    let (thread_id, callee_mri) = if echo {
        (signaling::echo_thread_id(caller_oid), None)
    } else if let Some(ref tid) = thread_override {
        (
            tid.clone(),
            callee_mri_for_thread(tid, caller_oid, callee_user_id.as_deref()),
        )
    } else {
        let tid = std::env::var("TEAMS_AV_TEST_THREAD_ID").context(
            "TEAMS_AV_TEST_THREAD_ID env var not set. Set it to the thread ID of the av-test channel.",
        )?;
        anyhow::ensure!(!tid.is_empty(), "TEAMS_AV_TEST_THREAD_ID is empty");
        (tid, None)
    };

    let is_1to1_call = echo || callee_mri.is_some();

    let tenant_id = config
        .tenant_id
        .as_deref()
        .or(config
            .personal
            .then_some(ost_microsoft::auth::PERSONAL_TENANT_ID))
        .context("No tenant_id in config. Run `teams-cli login` first.")?;

    let (display_name, mail) = match &graph_token {
        Some(gt) if !gt.is_expired() => {
            let me = fetch_me(&http, &gt.token).await.ok();
            (
                me.as_ref()
                    .and_then(|m| m.display_name.clone())
                    .unwrap_or_else(|| "(unknown)".into()),
                me.as_ref()
                    .and_then(|m| m.mail.clone())
                    .unwrap_or_else(|| "(unknown)".into()),
            )
        }
        _ => {
            tracing::warn!("Graph token expired or missing — skipping profile fetch");
            ("(unknown)".into(), "(unknown)".into())
        }
    };

    println!();
    if echo {
        println!("=== Call Test (Echo / Call Quality Tester) ===");
    } else if callee_mri.is_some() {
        println!("=== Call Test (1:1 Call) ===");
    } else {
        println!("=== Call Test (av-test channel) ===");
    }
    println!("Caller:   {} ({})", display_name, mail);
    println!("Thread:   {}", thread_id);
    if let Some(ref mri) = callee_mri {
        println!("Callee:   {}", mri);
    }
    println!("Duration: {}s", duration_secs);
    if record {
        println!("Record:   enabled");
    }

    tracing::info!("Negotiating Trouter session...");
    let (trouter_session, epid) = session::negotiate(&http, skype_token_str).await?;
    let session_id =
        session::get_session_id(&http, &trouter_session, skype_token_str, &epid).await?;
    let mut ws = websocket::TrouterSocket::connect(&trouter_session, &session_id, &epid).await?;

    let frame = ws
        .recv_frame()
        .await?
        .context("WS closed before handshake")?;
    if !frame.starts_with("1::") {
        tracing::warn!("Expected 1:: handshake, got: {}", frame);
    }
    ws.authenticate(&teams_access_token.token, &trouter_session)
        .await
        .context("Could not authenticate outgoing-call Trouter session")?;

    if let Some(ref reg_url) = trouter_session.registrar_url {
        registrar::register(&http, skype_token_str, reg_url, &trouter_session.surl).await?;
    }

    let audio_socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
    let audio_port = audio_socket.local_addr()?.port();
    let video_socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
    let video_port = video_socket.local_addr()?.port();
    let local_ip = sdp::get_local_ip();

    let make_host_candidate = |port: u16| ice::IceCandidate {
        foundation: "1".to_string(),
        component: 1,
        transport: ice::Transport::Udp,
        priority: 2130706431,
        address: local_ip.clone(),
        port,
        candidate_type: ice::CandidateType::Host,
        raddr: None,
        rport: None,
    };

    let mut audio_candidates = vec![make_host_candidate(audio_port)];
    let mut video_candidates = vec![make_host_candidate(video_port)];

    if let Some(srflx) = ice::gather_srflx_candidate(&audio_socket, ice::DEFAULT_STUN_SERVER).await
    {
        tracing::info!(
            "Gathered audio srflx candidate: {}:{}",
            srflx.address,
            srflx.port
        );
        audio_candidates.push(srflx);
    }
    if let Some(srflx) = ice::gather_srflx_candidate(&video_socket, ice::DEFAULT_STUN_SERVER).await
    {
        tracing::info!(
            "Gathered video srflx candidate: {}:{}",
            srflx.address,
            srflx.port
        );
        video_candidates.push(srflx);
    }

    let our_audio_ufrag = sdp::generate_ice_ufrag();
    let our_audio_pwd = sdp::generate_ice_pwd();
    let our_video_ufrag = sdp::generate_ice_ufrag();
    let our_video_pwd = sdp::generate_ice_pwd();
    let video_ssrc = video::generate_ssrc();
    let audio_ssrc = video::generate_ssrc();
    let offer_result = sdp::generate_av_sdp_offer(&sdp::AvSdpParams {
        local_ip: &local_ip,
        include_video: use_camera,
        audio_port,
        video_port,
        audio_ufrag: &our_audio_ufrag,
        audio_pwd: &our_audio_pwd,
        video_ufrag: &our_video_ufrag,
        video_pwd: &our_video_pwd,
        audio_candidates: &audio_candidates,
        video_candidates: &video_candidates,
        video_ssrc_base: video_ssrc,
        audio_ssrc,
    });
    let call_offer_sdp = offer_result.sdp.clone();
    #[cfg(feature = "consumer-webrtc")]
    let mut call_offer_sdp = call_offer_sdp;
    #[cfg(feature = "consumer-webrtc")]
    let mut personal_webrtc = if config.personal {
        let (session, offer) =
            crate::calling::personal_webrtc::PersonalWebRtcSession::create(use_camera)
                .await
                .context("Failed to create Teams Free WebRTC offer")?;
        call_offer_sdp = offer;
        Some(session)
    } else {
        None
    };

    tracing::info!(
        "AV SDP offer ({} bytes):\n{}",
        call_offer_sdp.len(),
        call_offer_sdp
    );

    #[cfg(any(feature = "video-capture", feature = "video-capture-windows"))]
    let (_outgoing_camera_capture, mut outgoing_camera_rx) = if use_camera && !echo {
        crate::calling::external_display::clear_local();
        match camera::CameraCapture::start(None, 320, 240, 30) {
            Ok((capture, source_rx)) => {
                let (preview_tx, preview_rx) = std::sync::mpsc::sync_channel(2);
                std::thread::spawn(move || {
                    while let Ok(frame) = source_rx.recv() {
                        crate::calling::external_display::push_local_frame(
                            frame.width,
                            frame.height,
                            frame.data.clone(),
                        );
                        match preview_tx.try_send(frame) {
                            Ok(()) | Err(std::sync::mpsc::TrySendError::Full(_)) => {}
                            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => break,
                        }
                    }
                });
                tracing::info!("Camera capture started for outgoing preview");
                (Some(capture), Some(preview_rx))
            }
            Err(error) => {
                tracing::warn!("Failed to start outgoing camera preview: {error:#}");
                (None, None)
            }
        }
    } else {
        (None, None)
    };

    let epconv_url = if config.personal {
        ost_microsoft::calling::PERSONAL_CALL_CONVERSATION_URL.to_string()
    } else {
        derive_epconv_url(&region_gtms).context("Cannot derive epconv URL from region_gtms")?
    };

    let endpoint_id = epid.clone();
    let participant_id = uuid::Uuid::new_v4().to_string();
    let chain_id = uuid::Uuid::new_v4().to_string();
    let message_id = uuid::Uuid::new_v4().to_string();

    let conv_params = signaling::ConversationCallParams {
        call_token: call_token_str,
        personal: config.personal,
        trouter_surl: &trouter_session.surl,
        caller_mri: &caller_mri,
        caller_display_name: &display_name,
        endpoint_id: &endpoint_id,
        participant_id: &participant_id,
        thread_id: &thread_id,
        chain_id: &chain_id,
        message_id: &message_id,
        caller_oid,
        tenant_id,
    };

    let echo_needs_invite = echo && !use_camera;
    let (phase1, _phase2) = if echo && use_camera {
        tracing::info!("Creating Microsoft video Test Call...");
        signaling::create_echo_call(&http, &epconv_url, &conv_params, &call_offer_sdp).await?
    } else if is_1to1_call {
        tracing::info!("Creating 1:1 call (single-shot epconv with SDP)...");
        signaling::create_1to1_call(
            &http,
            &epconv_url,
            &conv_params,
            &call_offer_sdp,
            config.personal.then_some(callee_mri.as_deref()).flatten(),
            use_camera,
        )
        .await?
    } else {
        tracing::info!("Phase 1: Creating conversation...");
        let created = signaling::create_conversation(&http, &epconv_url, &conv_params).await?;
        tracing::info!(
            "Phase 1 complete: conversationController = {}",
            created.conversation_controller
        );

        tracing::info!("Phase 2: Joining conversation with SDP...");
        let joined = signaling::join_conversation_with_sdp(
            &http,
            &created.conversation_controller,
            &conv_params,
            &call_offer_sdp,
        )
        .await?;
        tracing::info!(
            "Phase 2 complete. CC active URL: {:?}",
            joined.cc_active_url
        );
        (created, joined)
    };
    println!("call_placed=true");

    if echo_needs_invite {
        tracing::info!("Inviting Echo bot...");
        signaling::invite_echo_bot(
            &http,
            &phase1.conversation_controller,
            phase1.add_participant_url.as_deref(),
            &conv_params,
        )
        .await?;
    } else if !config.personal {
        if let Some(ref mri) = callee_mri {
            signaling::invite_user(
                &http,
                &phase1.conversation_controller,
                phase1.add_participant_url.as_deref(),
                &conv_params,
                mri,
                use_camera,
            )
            .await?;
        }
    }

    if let Some(progress) = &progress {
        let _ = progress.send(CallProgress::Ringing);
    }

    let acceptance_timeout = Duration::from_secs(if echo { 30 } else { 120 });
    tracing::info!(
        "Waiting up to {}s for media answer on Trouter...",
        acceptance_timeout.as_secs()
    );
    let acceptance_result = if let Some(stop_receiver) = stop.as_mut() {
        tokio::select! {
            result = wait_for_call_acceptance(&mut ws, acceptance_timeout) => Some(result),
            changed = stop_receiver.changed() => {
                if changed.is_ok() && *stop_receiver.borrow() {
                    None
                } else {
                    Some(Err(anyhow::anyhow!("Call stop signal closed while waiting for acceptance")))
                }
            }
        }
    } else {
        Some(wait_for_call_acceptance(&mut ws, acceptance_timeout).await)
    };
    let Some(acceptance_result) = acceptance_result else {
        tracing::info!("Call cancelled while waiting for acceptance");
        return Ok(CallTestResult::failed(Some("Call cancelled".to_owned())));
    };
    let acceptance = match acceptance_result {
        Ok(acc) => {
            if let Some(ref ack_url) = acc.acknowledgement_url {
                signaling::acknowledge_call_acceptance(&http, ack_url, &conv_params)
                    .await
                    .context("Failed to acknowledge call acceptance")?;
            }
            if let Some(ref reason) = acc.rejection_reason {
                tracing::warn!("Call rejected: {}", reason);
                println!("call_accepted=false");
                println!("rejection_reason={}", reason);
                return Ok(CallTestResult::failed(Some(reason.clone())));
            }
            acc
        }
        Err(e) => {
            tracing::warn!("No call acceptance received: {:#}", e);
            println!("call_accepted=false");
            return Ok(CallTestResult::failed(Some(format!("{e:#}"))));
        }
    };
    println!("call_accepted=true");

    #[cfg(feature = "consumer-webrtc")]
    if let Some(mut session) = personal_webrtc.take() {
        let remote_sdp = acceptance
            .sdp_blob
            .clone()
            .context("Teams Free accepted the call without SDP")?;
        session
            .set_answer(remote_sdp)
            .await
            .context("Failed to apply Teams Free WebRTC answer")?;
        let mut result = CallTestResult::failed(None);
        result.call_accepted = true;
        match session.wait_connected(Duration::from_secs(15)).await {
            Ok(()) if use_camera => {
                let width = 320usize;
                let height = 240usize;
                let frame_duration = Duration::from_millis(33);
                let mut encoder = codec::H264Encoder::new(width as u32, height as u32, 30.0, 700)
                    .context("Failed to create Teams Free H264 encoder")?;
                let deadline = tokio::time::Instant::now() + Duration::from_secs(duration_secs);
                let mut ticker = tokio::time::interval(frame_duration);
                let mut frame_index = 0usize;
                while tokio::time::Instant::now() < deadline {
                    ticker.tick().await;
                    if frame_index.is_multiple_of(60) {
                        encoder.force_intra_frame();
                    }
                    let yuv = synthetic_video_frame(width, height, frame_index);
                    let nals = encoder
                        .encode(&yuv)
                        .context("Failed to encode Teams Free synthetic video frame")?;
                    session
                        .write_h264_nals(&nals, frame_duration)
                        .await
                        .context("Failed to send Teams Free synthetic video frame")?;
                    frame_index += 1;
                }
                result.camera_frames_sent = frame_index as u32;
                tracing::info!("Teams Free synthetic video frames sent: {frame_index}");
            }
            Ok(()) => {
                tokio::time::sleep(Duration::from_secs(duration_secs)).await;
            }
            Err(error) => {
                result.rejection_reason =
                    Some(format!("Teams Free media did not connect: {error:#}"));
            }
        }
        if let Some(ref end_url) = acceptance.end_url {
            if let Err(error) = end_call_by_url(&http, skype_token_str, end_url).await {
                tracing::warn!("Failed to end Teams Free test call: {error:#}");
            }
        }
        return Ok(result);
    }

    let mut active_ws = Some(ws);

    if acceptance.acknowledgement_url.is_none() {
        tracing::info!(
            "Media answer has no acknowledgement URL; waiting for later call acceptance"
        );
    }

    if let Some(ref leg_url) = acceptance.call_leg_url {
        if let Err(e) = signaling::register_cc_callbacks(&http, leg_url, &conv_params).await {
            tracing::warn!("Failed to register CC callbacks: {:#}", e);
        }
    } else {
        tracing::warn!("No callLeg URL in callAcceptance — skipping CC callback registration");
    }

    let remote_sdp = acceptance.sdp_blob.clone().unwrap_or_default();
    let media_setup = setup_media_leg(MediaLegSetup {
        local_audio_crypto_line: &offer_result.audio_crypto_line,
        local_video_crypto_line: &offer_result.video_crypto_line,
        local_audio_ufrag: &our_audio_ufrag,
        local_audio_pwd: &our_audio_pwd,
        local_video_ufrag: &our_video_ufrag,
        local_video_pwd: &our_video_pwd,
        remote_sdp: &remote_sdp,
        audio_socket,
        video_socket,
        label: "outgoing",
        controlling: true,
        audio_ssrc,
        video_ssrc,
    });
    let mut outgoing_leg = if let Some(ws) = active_ws.as_mut() {
        await_with_call_signaling(ws, media_setup, &http, &conv_params, &call_offer_sdp).await
    } else {
        media_setup.await
    }
    .context("Failed to set up outgoing media leg")?;

    let recorder = Arc::new(Mutex::new(test_tone::AudioRecorder::new(
        (duration_secs as usize) * 8000,
    )));

    #[cfg(feature = "audio")]
    let (_audio_playback, speaker_tx) = {
        match super::audio::AudioPlayback::start() {
            Some((playback, tx)) => {
                tracing::info!("Audio playback initialized successfully");
                (Some(playback), Some(tx))
            }
            None => {
                tracing::warn!("No audio output device — received audio will not be rendered");
                (None, None)
            }
        }
    };
    #[cfg(not(feature = "audio"))]
    let speaker_tx: Option<std::sync::mpsc::SyncSender<Vec<i16>>> = None;

    #[cfg(feature = "audio")]
    let (_audio_capture, mic_rx) = if !tone_mode {
        match super::audio::AudioCapture::start() {
            Some((cap, rx)) => {
                tracing::info!("Microphone capture started — sending real audio");
                (Some(cap), Some(rx))
            }
            None => {
                tracing::warn!("No audio input device — falling back to 1kHz tone");
                (None, None)
            }
        }
    } else {
        tracing::info!("Tone mode: sending 1kHz test tone");
        (None, None)
    };
    #[cfg(not(feature = "audio"))]
    let mic_rx: Option<std::sync::mpsc::Receiver<Vec<i16>>> = None;

    #[cfg(any(feature = "video-capture", feature = "video-capture-windows"))]
    if use_camera {
        if let Some(rx) = outgoing_camera_rx.take() {
            tracing::info!("Using outgoing preview camera capture for call media");
            outgoing_leg.camera_rx = Some(rx);
        } else {
            match camera::CameraCapture::start(None, 320, 240, 30) {
                Ok((_capture, rx)) => {
                    tracing::info!("Camera capture started (320x240 @ 30fps)");
                    outgoing_leg.camera_rx = Some(rx);
                }
                Err(e) => tracing::warn!("Failed to start camera: {:#}", e),
            }
        }
    }
    #[cfg(all(feature = "video-codec", target_os = "android"))]
    if use_camera {
        outgoing_leg.camera_rx = Some(external_camera::subscribe());
    }
    outgoing_leg.decode_remote_video = use_display || use_camera;
    #[cfg(feature = "video-capture")]
    if use_display {
        match display::VideoDisplay::start("Teams Video - Received") {
            Ok((_display, tx)) => {
                tracing::info!("Video display window opened");
                outgoing_leg.display_tx = Some(tx);
            }
            Err(e) => tracing::warn!("Failed to open video display: {:#}", e),
        }
    }
    #[cfg(not(feature = "video-codec"))]
    if use_camera {
        tracing::warn!("Camera requested without video codec support");
    }
    #[cfg(not(feature = "video-capture"))]
    if use_display {
        tracing::warn!("Video display requested without video capture support");
    }

    let outgoing_handles = spawn_media_leg(MediaLegSpawn {
        leg: outgoing_leg,
        cname: &caller_mri,
        recorder: Some(recorder.clone()),
        loopback: false,
        speaker_tx,
        mic_rx,
        tone_mode,
        progress: progress.clone(),
    });

    let mut recording_handle = None;
    if record {
        if let Some(rec_token) = recorder_token_str {
            let http = http.clone();
            let conversation_controller = phase1.conversation_controller.clone();
            let add_participant_url_override = phase1.add_participant_url.clone();
            let caller_mri = caller_mri.clone();
            let participant_id = participant_id.clone();
            let endpoint_id = endpoint_id.clone();
            let chain_id = chain_id.clone();
            let message_id = message_id.clone();
            let thread_id = thread_id.clone();
            let display_name = display_name.clone();
            let trouter_surl = trouter_session.surl.clone();
            let ic3_token = ic3_token
                .as_ref()
                .context("Recording requires an IC3 token")?
                .token
                .clone();
            let skype_token = skype_token_str.to_string();
            let mut recording_ws = active_ws
                .take()
                .context("Trouter unavailable for recording")?;
            recording_handle = Some(tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(3)).await;
                tracing::info!("Starting call recording (background, after 3s media warm-up)...");
                match recording::start_call_recording(
                    &http,
                    &mut recording_ws,
                    recording::RecordingRequest {
                        caller_mri: &caller_mri,
                        participant_id: &participant_id,
                        endpoint_id: &endpoint_id,
                        chain_id: &chain_id,
                        message_id: &message_id,
                        thread_id: &thread_id,
                        display_name: &display_name,
                        trouter_surl: &trouter_surl,
                        ic3_token: &ic3_token,
                        recorder_token: &rec_token,
                        skype_token: &skype_token,
                        conversation_controller: &conversation_controller,
                        add_participant_url_override: add_participant_url_override.as_deref(),
                    },
                )
                .await
                {
                    Ok(session) => {
                        println!("recording=true");
                        Some(session)
                    }
                    Err(e) => {
                        tracing::warn!("Recording flow failed (non-fatal): {:#}", e);
                        None
                    }
                }
            }));
        }
    }

    let interactive_test_call = echo && progress.is_some();
    let mut media_failure_reason = None;
    if interactive_test_call {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        loop {
            if outgoing_handles
                .audio_first_rtp_source
                .lock()
                .await
                .is_some()
            {
                break;
            }
            if stop.as_ref().is_some_and(|receiver| *receiver.borrow()) {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                media_failure_reason =
                    Some("Microsoft answered but no inbound audio media arrived".to_string());
                tracing::warn!("Test call media connection timed out after 15s");
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    if media_failure_reason.is_none() && !stop.as_ref().is_some_and(|receiver| *receiver.borrow()) {
        tracing::info!("Call active, running for up to {}s...", duration_secs);
        if let Some(ws) = active_ws.as_mut() {
            let duration = if interactive_test_call {
                duration_secs.min(35)
            } else {
                duration_secs
            };
            let call_end = wait_for_call_end(
                ws,
                Duration::from_secs(duration),
                &http,
                &conv_params,
                &offer_result.sdp,
                Some(&outgoing_handles),
            );
            let result = if let Some(stop) = stop.as_mut() {
                tokio::select! {
                    result = call_end => result,
                    _ = stop.changed() => Ok(None),
                }
            } else {
                call_end.await
            };
            match result {
                Ok(reason) => media_failure_reason = reason,
                Err(error) => {
                    media_failure_reason = Some(format!("Call signaling failed: {error:#}"))
                }
            }
        } else if let Some(stop) = stop.as_mut() {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(duration_secs)) => {},
                _ = stop.changed() => {
                    tracing::info!("Call stop requested by client");
                }
            }
        } else {
            tokio::time::sleep(Duration::from_secs(duration_secs)).await;
        }
    }

    outgoing_handles.abort_all();
    #[cfg(all(feature = "video-codec", target_os = "android"))]
    external_camera::clear();
    #[cfg(all(
        feature = "video-codec",
        any(target_os = "android", target_os = "windows")
    ))]
    super::external_display::clear();

    if let Some(handle) = recording_handle {
        match handle.await {
            Ok(Some(session)) => {
                if let Err(e) = recording::stop_call_recording(&http, &session).await {
                    tracing::warn!("Stop recording failed (non-fatal): {:#}", e);
                }
            }
            Ok(None) => {}
            Err(e) => tracing::warn!("Recording task panicked: {:#}", e),
        }
    }

    let end_url = acceptance.end_url.clone().or_else(|| {
        acceptance.call_leg_url.as_ref().map(|u| {
            if let Some(idx) = u.find('?') {
                format!("{}/end{}", &u[..idx], &u[idx..])
            } else {
                format!("{}/end", u)
            }
        })
    });
    if let Some(ref url) = end_url {
        end_call_by_url(&http, skype_token_str, url).await.ok();
    } else {
        tracing::warn!("No end URL or call_leg_url available — cannot hang up");
    }

    let rec = recorder.lock().await;
    let echo_result = if tone_mode {
        test_tone::detect_echo(rec.samples(), 1000.0, 8000.0)
    } else {
        test_tone::EchoResult {
            detected: false,
            delay_ms: 0.0,
            correlation_peak: 0.0,
        }
    };

    {
        let pcm_path = "/tmp/received_audio.pcm";
        let samples = rec.samples();
        let mut f = std::fs::File::create(pcm_path).ok();
        if let Some(ref mut f) = f {
            use std::io::Write;
            for &s in samples {
                let _ = f.write_all(&s.to_le_bytes());
            }
            tracing::info!(
                "Raw PCM i16 LE saved to {} ({} samples, {:.1}s)",
                pcm_path,
                samples.len(),
                samples.len() as f64 / 8000.0
            );
        }
    }

    let out_ss = outgoing_handles.send_stats.lock().await;
    let out_rs = outgoing_handles.recv_stats.lock().await;
    let out_vid_ss = outgoing_handles.video_send_stats.lock().await;
    let out_vid_rs = outgoing_handles.video_recv_stats.lock().await;
    #[cfg(feature = "audio")]
    let (
        microphone_frames_produced,
        microphone_frames_enqueued,
        microphone_frames_dropped,
        microphone_receiver_disconnects,
        microphone_stream_errors,
    ) = _audio_capture
        .as_ref()
        .map(super::audio::AudioCapture::stats)
        .map(|stats| {
            (
                stats.produced(),
                stats.enqueued(),
                stats.full(),
                stats.disconnected(),
                stats.stream_errors(),
            )
        })
        .unwrap_or_default();
    #[cfg(not(feature = "audio"))]
    let (
        microphone_frames_produced,
        microphone_frames_enqueued,
        microphone_frames_dropped,
        microphone_receiver_disconnects,
        microphone_stream_errors,
    ) = (0, 0, 0, 0, 0);
    #[cfg(feature = "audio")]
    let (speaker_frames_received, speaker_samples_rendered, speaker_stream_errors) =
        _audio_playback
            .as_ref()
            .map(super::audio::AudioPlayback::stats)
            .map(|stats| {
                (
                    stats.frames_received(),
                    stats.samples_rendered(),
                    stats.stream_errors(),
                )
            })
            .unwrap_or_default();
    #[cfg(not(feature = "audio"))]
    let (speaker_frames_received, speaker_samples_rendered, speaker_stream_errors) = (0, 0, 0);

    let result = CallTestResult {
        call_placed: true,
        call_accepted: true,
        rejection_reason: media_failure_reason,
        packets_sent: out_ss.packets_sent,
        packets_received: out_rs.packets_received,
        raw_audio_datagrams: outgoing_handles.raw_audio_datagrams.load(Ordering::Relaxed),
        audio_stun_datagrams: outgoing_handles
            .audio_stun_datagrams
            .load(Ordering::Relaxed),
        audio_srtp_decrypt_failures: outgoing_handles
            .audio_srtp_decrypt_failures
            .load(Ordering::Relaxed),
        audio_initial_remote_addr: Some(outgoing_handles.audio_initial_remote_addr),
        audio_final_remote_addr: Some(*outgoing_handles.audio_dynamic_remote_addr.lock().await),
        audio_first_rtp_source: *outgoing_handles.audio_first_rtp_source.lock().await,
        microphone_frames: outgoing_handles.microphone_frames.load(Ordering::Relaxed),
        microphone_non_silent_frames: outgoing_handles
            .microphone_non_silent_frames
            .load(Ordering::Relaxed),
        microphone_peak: outgoing_handles.microphone_peak.load(Ordering::Relaxed),
        microphone_frames_produced,
        microphone_frames_enqueued,
        microphone_frames_dropped,
        microphone_receiver_disconnects,
        microphone_stream_errors,
        speaker_frames_received,
        speaker_samples_rendered,
        speaker_stream_errors,
        video_packets_sent: out_vid_ss.packets_sent,
        video_packets_received: out_vid_rs.packets_received,
        raw_video_datagrams: outgoing_handles.raw_video_datagrams.load(Ordering::Relaxed),
        video_stun_datagrams: outgoing_handles
            .video_stun_datagrams
            .load(Ordering::Relaxed),
        video_srtcp_datagrams: outgoing_handles
            .video_srtcp_datagrams
            .load(Ordering::Relaxed),
        video_srtp_decrypt_failures: outgoing_handles
            .video_srtp_decrypt_failures
            .load(Ordering::Relaxed),
        video_vsr_requests_sent: outgoing_handles
            .video_vsr_requests_sent
            .load(Ordering::Relaxed),
        video_keyframe_requests_received: outgoing_handles
            .video_keyframe_requests_received
            .load(Ordering::Relaxed),
        video_receiver_reports_for_local_ssrc: outgoing_handles
            .video_receiver_reports_for_local_ssrc
            .load(Ordering::Relaxed),
        video_rr_fraction_lost: outgoing_handles
            .video_rr_fraction_lost
            .load(Ordering::Relaxed),
        video_rr_cumulative_lost: outgoing_handles
            .video_rr_cumulative_lost
            .load(Ordering::Relaxed),
        video_rr_extended_highest_seq: outgoing_handles
            .video_rr_extended_highest_seq
            .load(Ordering::Relaxed),
        video_rr_jitter: outgoing_handles.video_rr_jitter.load(Ordering::Relaxed),
        video_first_sequence_sent: outgoing_handles
            .video_first_sequence_sent
            .load(Ordering::Relaxed),
        video_last_sequence_sent: outgoing_handles
            .video_last_sequence_sent
            .load(Ordering::Relaxed),
        video_initial_remote_addr: outgoing_handles.video_initial_remote_addr,
        video_final_remote_addr: *outgoing_handles.video_dynamic_remote_addr.lock().await,
        video_first_rtp_source: *outgoing_handles.video_first_rtp_source.lock().await,
        camera_frames_sent: outgoing_handles.camera_frames_sent.load(Ordering::Relaxed),
        incoming_audio_pkts_sent: 0,
        incoming_audio_pkts_recv: 0,
        incoming_video_pkts_sent: 0,
        incoming_video_pkts_recv: 0,
        echo_detected: echo_result.detected,
        echo_delay_ms: echo_result.delay_ms,
        echo_correlation: echo_result.correlation_peak,
    };

    println!("audio_packets_sent={}", result.packets_sent);
    println!("audio_packets_received={}", result.packets_received);
    println!("raw_audio_datagrams={}", result.raw_audio_datagrams);
    println!("audio_stun_datagrams={}", result.audio_stun_datagrams);
    println!(
        "audio_srtp_decrypt_failures={}",
        result.audio_srtp_decrypt_failures
    );
    println!(
        "audio_initial_remote_addr={:?}",
        result.audio_initial_remote_addr
    );
    println!(
        "audio_final_remote_addr={:?}",
        result.audio_final_remote_addr
    );
    println!("audio_first_rtp_source={:?}", result.audio_first_rtp_source);
    println!("microphone_frames={}", result.microphone_frames);
    println!(
        "microphone_non_silent_frames={}",
        result.microphone_non_silent_frames
    );
    println!("microphone_peak={}", result.microphone_peak);
    println!(
        "microphone_frames_produced={}",
        result.microphone_frames_produced
    );
    println!(
        "microphone_frames_enqueued={}",
        result.microphone_frames_enqueued
    );
    println!(
        "microphone_frames_dropped={}",
        result.microphone_frames_dropped
    );
    println!(
        "microphone_receiver_disconnects={}",
        result.microphone_receiver_disconnects
    );
    println!(
        "microphone_stream_errors={}",
        result.microphone_stream_errors
    );
    println!("speaker_frames_received={}", result.speaker_frames_received);
    println!(
        "speaker_samples_rendered={}",
        result.speaker_samples_rendered
    );
    println!("speaker_stream_errors={}", result.speaker_stream_errors);
    println!("camera_frames_sent={}", result.camera_frames_sent);
    println!("video_packets_sent={}", result.video_packets_sent);
    println!("video_packets_received={}", result.video_packets_received);
    println!("raw_video_datagrams={}", result.raw_video_datagrams);
    println!("video_stun_datagrams={}", result.video_stun_datagrams);
    println!("video_srtcp_datagrams={}", result.video_srtcp_datagrams);
    println!(
        "video_srtp_decrypt_failures={}",
        result.video_srtp_decrypt_failures
    );
    println!("video_vsr_requests_sent={}", result.video_vsr_requests_sent);
    println!(
        "video_keyframe_requests_received={}",
        result.video_keyframe_requests_received
    );
    println!(
        "video_receiver_reports_for_local_ssrc={}",
        result.video_receiver_reports_for_local_ssrc
    );
    println!("video_rr_fraction_lost={}", result.video_rr_fraction_lost);
    println!(
        "video_rr_cumulative_lost={}",
        result.video_rr_cumulative_lost
    );
    println!(
        "video_rr_extended_highest_seq={}",
        result.video_rr_extended_highest_seq
    );
    println!("video_rr_jitter={}", result.video_rr_jitter);
    println!(
        "video_first_sequence_sent={}",
        result.video_first_sequence_sent
    );
    println!(
        "video_last_sequence_sent={}",
        result.video_last_sequence_sent
    );
    println!(
        "video_initial_remote_addr={:?}",
        result.video_initial_remote_addr
    );
    println!(
        "video_final_remote_addr={:?}",
        result.video_final_remote_addr
    );
    println!("video_first_rtp_source={:?}", result.video_first_rtp_source);
    println!("echo_detected={}", result.echo_detected);
    println!("echo_delay_ms={:.1}", result.echo_delay_ms);
    println!("echo_correlation={:.3}", result.echo_correlation);

    Ok(result)
}
