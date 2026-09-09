use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Mutex as StdMutex,
};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use base64::Engine;
use once_cell::sync::Lazy;
use ost_microsoft::calling as microsoft_calling;
use tokio::sync::{broadcast, watch, Mutex, Semaphore};

use crate::frb_generated::StreamSink;
use teams_cli::calling::{self, CallNotification};

use super::auth;

mod incoming;

pub(crate) use incoming::subscribe_events_for_web;
use incoming::{
    build_call_config, clear_pending_call, emit, ensure_incoming_listener, wait_for_retry_or_stop,
};

#[derive(Clone)]
pub struct CallEvent {
    pub kind: String,
    pub call_id: String,
    pub conversation_id: Option<String>,
    pub display_name: Option<String>,
    pub detail: Option<String>,
}

pub struct RemoteVideoFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct MediaDeviceInfo {
    pub id: String,
    pub label: String,
}

static MICROPHONE_PREVIEW_ACTIVE: AtomicBool = AtomicBool::new(false);
static MICROPHONE_PREVIEW_HAS_SAMPLE: AtomicBool = AtomicBool::new(false);
static MICROPHONE_PREVIEW_PEAK: AtomicU32 = AtomicU32::new(0);
static MICROPHONE_PREVIEW_LAST_REQUEST: Lazy<StdMutex<Option<Instant>>> =
    Lazy::new(|| StdMutex::new(None));

#[cfg(any(target_os = "linux", target_os = "windows"))]
static CAMERA_PREVIEW_ACTIVE: AtomicBool = AtomicBool::new(false);
#[cfg(any(target_os = "linux", target_os = "windows"))]
static CAMERA_PREVIEW_STOP: AtomicBool = AtomicBool::new(false);
#[cfg(any(target_os = "linux", target_os = "windows"))]
static CAMERA_PREVIEW_LAST_REQUEST: Lazy<StdMutex<Option<Instant>>> =
    Lazy::new(|| StdMutex::new(None));

pub fn media_devices(kind: String) -> Vec<MediaDeviceInfo> {
    let names = match kind.as_str() {
        "microphone" => calling::audio::device_names(true),
        "speaker" => calling::audio::device_names(false),
        "camera" => {
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            {
                calling::camera::device_names()
            }
            #[cfg(not(any(target_os = "linux", target_os = "windows")))]
            {
                Vec::new()
            }
        }
        _ => Vec::new(),
    };
    let mut devices = vec![MediaDeviceInfo {
        id: String::new(),
        label: "System default".to_owned(),
    }];
    devices.extend(names.into_iter().map(|name| MediaDeviceInfo {
        id: name.clone(),
        label: name,
    }));
    devices
}

fn ensure_microphone_preview() {
    *MICROPHONE_PREVIEW_LAST_REQUEST.lock().unwrap() = Some(Instant::now());
    if MICROPHONE_PREVIEW_ACTIVE
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    MICROPHONE_PREVIEW_HAS_SAMPLE.store(false, Ordering::Release);
    MICROPHONE_PREVIEW_PEAK.store(0, Ordering::Release);
    std::thread::spawn(|| {
        if let Some((_capture, receiver)) = calling::audio::AudioCapture::start() {
            loop {
                let expired = MICROPHONE_PREVIEW_LAST_REQUEST
                    .lock()
                    .unwrap()
                    .as_ref()
                    .is_none_or(|requested| requested.elapsed() > Duration::from_millis(750));
                if expired {
                    break;
                }
                match receiver.recv_timeout(Duration::from_millis(50)) {
                    Ok(frame) => {
                        let peak = frame
                            .iter()
                            .map(|sample| (*sample as i32).unsigned_abs())
                            .max()
                            .unwrap_or(0);
                        MICROPHONE_PREVIEW_PEAK.fetch_max(peak, Ordering::AcqRel);
                        MICROPHONE_PREVIEW_HAS_SAMPLE.store(true, Ordering::Release);
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        MICROPHONE_PREVIEW_HAS_SAMPLE.store(true, Ordering::Release);
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        }
        MICROPHONE_PREVIEW_PEAK.store(0, Ordering::Release);
        MICROPHONE_PREVIEW_HAS_SAMPLE.store(false, Ordering::Release);
        MICROPHONE_PREVIEW_ACTIVE.store(false, Ordering::Release);
    });
}

fn stop_microphone_preview() {
    *MICROPHONE_PREVIEW_LAST_REQUEST.lock().unwrap() = None;
    let deadline = Instant::now() + Duration::from_secs(1);
    while MICROPHONE_PREVIEW_ACTIVE.load(Ordering::Acquire) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    MICROPHONE_PREVIEW_PEAK.store(0, Ordering::Release);
    MICROPHONE_PREVIEW_HAS_SAMPLE.store(false, Ordering::Release);
}

pub fn preview_microphone_peak() -> u32 {
    ensure_microphone_preview();
    let deadline = Instant::now() + Duration::from_millis(250);
    while MICROPHONE_PREVIEW_ACTIVE.load(Ordering::Acquire)
        && !MICROPHONE_PREVIEW_HAS_SAMPLE.load(Ordering::Acquire)
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    MICROPHONE_PREVIEW_PEAK.swap(0, Ordering::AcqRel)
}

pub fn preview_speaker() -> bool {
    let Some((_playback, sender)) = calling::audio::AudioPlayback::start() else {
        return false;
    };
    let mut tone = calling::test_tone::ToneGenerator::new();
    for _ in 0..20 {
        if sender.send(tone.next_frame()).is_err() {
            return false;
        }
    }
    std::thread::sleep(Duration::from_millis(450));
    true
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn ensure_camera_preview() {
    *CAMERA_PREVIEW_LAST_REQUEST.lock().unwrap() = Some(Instant::now());
    if CAMERA_PREVIEW_ACTIVE
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    CAMERA_PREVIEW_STOP.store(false, Ordering::Release);
    calling::external_display::clear_local();
    std::thread::spawn(|| {
        if let Ok((_capture, receiver)) = calling::camera::CameraCapture::start(None, 320, 240, 30)
        {
            loop {
                let expired = CAMERA_PREVIEW_LAST_REQUEST
                    .lock()
                    .unwrap()
                    .as_ref()
                    .is_none_or(|requested| requested.elapsed() > Duration::from_millis(750));
                if CAMERA_PREVIEW_STOP.load(Ordering::Acquire) || expired {
                    break;
                }
                match receiver.recv_timeout(Duration::from_millis(100)) {
                    Ok(frame) => calling::external_display::push_local_frame(
                        frame.width,
                        frame.height,
                        frame.data,
                    ),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        }
        CAMERA_PREVIEW_ACTIVE.store(false, Ordering::Release);
    });
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn stop_camera_preview() {
    CAMERA_PREVIEW_STOP.store(true, Ordering::Release);
    let deadline = Instant::now() + Duration::from_secs(2);
    while CAMERA_PREVIEW_ACTIVE.load(Ordering::Acquire) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    CAMERA_PREVIEW_STOP.store(false, Ordering::Release);
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn stop_camera_preview() {}

pub fn preview_camera_frame() -> Option<RemoteVideoFrame> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        ensure_camera_preview();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(frame) = local_video_frame() {
                return Some(frame);
            }
            if !CAMERA_PREVIEW_ACTIVE.load(Ordering::Acquire) || Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

pub fn remote_video_frame() -> Option<RemoteVideoFrame> {
    convert_video_frame(calling::external_display::latest_frame()?)
}

pub fn local_video_frame() -> Option<RemoteVideoFrame> {
    convert_video_frame(calling::external_display::latest_local_frame()?)
}

fn convert_video_frame(
    frame: calling::external_display::RemoteVideoFrame,
) -> Option<RemoteVideoFrame> {
    let width = frame.width as usize;
    let height = frame.height as usize;
    let y_size = width.checked_mul(height)?;
    let uv_width = width / 2;
    let uv_size = uv_width.checked_mul(height / 2)?;
    if frame.data.len() < y_size + uv_size * 2 {
        return None;
    }
    let u_offset = y_size;
    let v_offset = y_size + uv_size;
    let mut rgba = vec![0u8; y_size * 4];
    for row in 0..height {
        for col in 0..width {
            let y = frame.data[row * width + col] as i32 - 16;
            let uv_index = (row / 2) * uv_width + col / 2;
            let u = frame.data[u_offset + uv_index] as i32 - 128;
            let v = frame.data[v_offset + uv_index] as i32 - 128;
            let c = y.max(0);
            let index = (row * width + col) * 4;
            rgba[index] = ((298 * c + 409 * v + 128) >> 8).clamp(0, 255) as u8;
            rgba[index + 1] = ((298 * c - 100 * u - 208 * v + 128) >> 8).clamp(0, 255) as u8;
            rgba[index + 2] = ((298 * c + 516 * u + 128) >> 8).clamp(0, 255) as u8;
            rgba[index + 3] = 255;
        }
    }
    Some(RemoteVideoFrame {
        width: frame.width,
        height: frame.height,
        rgba,
    })
}

struct ActiveIncomingCall {
    call_id: String,
    notification: CallNotification,
    media: calling::media::MediaSession,
    video: Option<calling::media::VideoMediaSession>,
}

#[derive(Clone)]
struct PendingIncomingCall {
    call_id: String,
    notification: CallNotification,
    received_at: Instant,
}

const PENDING_CALL_TTL: Duration = Duration::from_secs(90);

static CALL_EVENTS: Lazy<broadcast::Sender<CallEvent>> = Lazy::new(|| broadcast::channel(64).0);
static CALL_EVENTS_STOP: Lazy<broadcast::Sender<()>> = Lazy::new(|| broadcast::channel(8).0);
static ACCEPT_INCOMING_GATE: Lazy<Semaphore> = Lazy::new(|| Semaphore::new(1));
static INCOMING_STOP: Lazy<Mutex<Option<watch::Sender<bool>>>> = Lazy::new(|| Mutex::new(None));
static OUTGOING_STOP: Lazy<Mutex<Option<watch::Sender<bool>>>> = Lazy::new(|| Mutex::new(None));
static PENDING_INCOMING: Lazy<Mutex<Option<PendingIncomingCall>>> = Lazy::new(|| Mutex::new(None));
static ACTIVE_INCOMING: Lazy<Mutex<Option<ActiveIncomingCall>>> = Lazy::new(|| Mutex::new(None));

pub async fn listen_call_events(sink: StreamSink<CallEvent>) -> Result<()> {
    ensure_incoming_listener().await?;
    let mut receiver = CALL_EVENTS.subscribe();
    let mut stop_receiver = CALL_EVENTS_STOP.subscribe();
    loop {
        tokio::select! {
            _ = stop_receiver.recv() => return Ok(()),
            event = receiver.recv() => match event {
                Ok(event) => {
                    if sink.add(event).is_err() {
                        return Ok(());
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return Ok(()),
            },
        }
    }
}

pub async fn stop_call_events() {
    let _ = CALL_EVENTS_STOP.send(());
    if let Some(stop) = INCOMING_STOP.lock().await.take() {
        let _ = stop.send(true);
    }
    *PENDING_INCOMING.lock().await = None;
}

pub async fn start_call(conversation_id: String) -> Result<()> {
    let (conversation_id, use_camera) =
        match conversation_id.strip_prefix("__microslop_video_call__") {
            Some(conversation_id) => (conversation_id.to_owned(), true),
            None => (conversation_id, false),
        };
    let (conversation_id, callee_user_id) = if let Some(target) =
        conversation_id.strip_prefix("__microslop_callee_")
    {
        let (encoded, conversation_id) = target.split_once(':').context("Invalid call target")?;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(encoded)?;
        (
            conversation_id.to_owned(),
            Some(String::from_utf8(bytes).context("Invalid call target user ID")?),
        )
    } else {
        (conversation_id, None)
    };
    if let Some(selection) = conversation_id.strip_prefix("__microslop_device_") {
        let (kind, encoded) = selection
            .split_once('_')
            .context("Invalid media device selection")?;
        let bytes = base64::engine::general_purpose::URL_SAFE.decode(encoded)?;
        let value = String::from_utf8(bytes)?;
        let selected = (!value.is_empty()).then_some(value);
        match kind {
            "microphone" => {
                stop_microphone_preview();
                calling::audio::set_input_device(selected);
            }
            "speaker" => calling::audio::set_output_device(selected),
            "camera" => {
                stop_camera_preview();
                set_camera_device(selected);
            }
            _ => anyhow::bail!("Invalid media device kind"),
        }
        return Ok(());
    }
    match conversation_id.as_str() {
        "__microslop_call_mic_on__" => {
            calling::call_test::set_microphone_enabled(true);
            return Ok(());
        }
        "__microslop_call_mic_off__" => {
            calling::call_test::set_microphone_enabled(false);
            return Ok(());
        }
        "__microslop_call_speaker_on__" => {
            calling::call_test::set_speaker_enabled(true);
            return Ok(());
        }
        "__microslop_call_speaker_off__" => {
            calling::call_test::set_speaker_enabled(false);
            return Ok(());
        }
        "__microslop_call_ringback_on__" => {
            calling::audio::start_ringback();
            return Ok(());
        }
        "__microslop_call_ringback_off__" => {
            calling::audio::stop_ringback();
            return Ok(());
        }
        _ => {}
    }

    stop_microphone_preview();
    if conversation_id != "__microslop_test_call__" {
        stop_camera_preview();
    }
    if ACCEPT_INCOMING_GATE.available_permits() == 0
        || OUTGOING_STOP.lock().await.is_some()
        || ACTIVE_INCOMING.lock().await.is_some()
        || PENDING_INCOMING.lock().await.is_some()
    {
        anyhow::bail!("A call is already active");
    }
    calling::call_test::reset_media_controls();
    let call_config = build_call_config().await?;

    let call_id = uuid::Uuid::new_v4().to_string();
    let (stop_sender, stop_receiver) = watch::channel(false);
    *OUTGOING_STOP.lock().await = Some(stop_sender);
    let (progress_sender, mut progress_receiver) = tokio::sync::mpsc::unbounded_channel();

    let progress_call_id = call_id.clone();
    let progress_conversation_id = conversation_id.clone();
    tokio::spawn(async move {
        while let Some(progress) = progress_receiver.recv().await {
            let kind = match progress {
                calling::call_test::CallProgress::Dialing => "dialing",
                calling::call_test::CallProgress::Ringing => "ringing",
                calling::call_test::CallProgress::Connected => "connected",
            };
            emit(CallEvent {
                kind: kind.to_owned(),
                call_id: progress_call_id.clone(),
                conversation_id: Some(progress_conversation_id.clone()),
                display_name: None,
                detail: None,
            });
        }
    });

    tokio::spawn(async move {
        let mut attempt = 0u8;
        let result = loop {
            attempt += 1;
            let result = calling::call_test::run_call_until_stop_with_config(
                call_config.clone(),
                calling::call_test::CallTestOptions {
                    duration_secs: 0,
                    record: false,
                    echo: false,
                    thread_override: Some(conversation_id.clone()),
                    callee_user_id: callee_user_id.clone(),
                    use_camera,
                    // Flutter renders received frames from external_display. Starting the
                    // standalone SDL window here duplicates that path and can abort the
                    // whole call process when SDL receives an event value it cannot decode.
                    use_display: false,
                    tone_mode: false,
                },
                stop_receiver.clone(),
                progress_sender.clone(),
            )
            .await;

            let should_retry = conversation_id == "__microslop_test_call__"
                && attempt == 1
                && !*stop_receiver.borrow()
                && matches!(
                    &result,
                    Ok(call_result)
                        if call_result.call_accepted
                            && call_result.packets_received == 0
                            && call_result.audio_first_rtp_source.is_none()
                );
            if !should_retry {
                break result;
            }

            tracing::warn!(
                "Microsoft accepted the test call without sending RTP; retrying once with a fresh call leg"
            );
            if !wait_for_retry_or_stop(&stop_receiver, Duration::from_millis(500)).await {
                break result;
            }
        };
        match result {
            Ok(result) => {
                let (kind, detail) = if conversation_id == "__microslop_test_call__" {
                    (
                        "ended",
                        Some(format!(
                            "test-call;accepted={};sent={};received={};raw_udp={};stun={};decrypt_failures={};remote_initial={};remote_final={};first_rtp={};mic_frames={};mic_non_silent={};mic_peak={};speaker_frames={};speaker_samples={};speaker_errors={};video_sent={};video_received={};camera_frames={};echo={};delay_ms={:.1};correlation={:.3};reason={}",
                            result.call_accepted,
                            result.packets_sent,
                            result.packets_received,
                            result.raw_audio_datagrams,
                            result.audio_stun_datagrams,
                            result.audio_srtp_decrypt_failures,
                            result
                                .audio_initial_remote_addr
                                .map(|addr| addr.to_string())
                                .unwrap_or_default(),
                            result
                                .audio_final_remote_addr
                                .map(|addr| addr.to_string())
                                .unwrap_or_default(),
                            result
                                .audio_first_rtp_source
                                .map(|addr| addr.to_string())
                                .unwrap_or_default(),
                            result.microphone_frames,
                            result.microphone_non_silent_frames,
                            result.microphone_peak,
                            result.speaker_frames_received,
                            result.speaker_samples_rendered,
                            result.speaker_stream_errors,
                            result.video_packets_sent,
                            result.video_packets_received,
                            result.camera_frames_sent,
                            result.echo_detected,
                            result.echo_delay_ms,
                            result.echo_correlation,
                            result.rejection_reason.as_deref().unwrap_or("")
                        )),
                    )
                } else if result.call_accepted {
                    let detail = if use_camera
                        && std::env::var_os("MICROSLOP_VIDEO_E2E_PROOF").is_some()
                    {
                        Some(format!(
                            "video-call;accepted=true;video_sent={};video_received={};video_srtcp={};receiver_reports={};rr_fraction_lost={};rr_cumulative_lost={};rr_highest_seq={};rr_jitter={};first_seq_sent={};last_seq_sent={};keyframe_requests={};camera_frames={};raw_video={};video_stun={};decrypt_failures={};remote_initial={};remote_final={};first_rtp={};reason={}",
                            result.video_packets_sent,
                            result.video_packets_received,
                            result.video_srtcp_datagrams,
                            result.video_receiver_reports_for_local_ssrc,
                            result.video_rr_fraction_lost,
                            result.video_rr_cumulative_lost,
                            result.video_rr_extended_highest_seq,
                            result.video_rr_jitter,
                            result.video_first_sequence_sent,
                            result.video_last_sequence_sent,
                            result.video_keyframe_requests_received,
                            result.camera_frames_sent,
                            result.raw_video_datagrams,
                            result.video_stun_datagrams,
                            result.video_srtp_decrypt_failures,
                            result
                                .video_initial_remote_addr
                                .map(|addr| addr.to_string())
                                .unwrap_or_default(),
                            result
                                .video_final_remote_addr
                                .map(|addr| addr.to_string())
                                .unwrap_or_default(),
                            result
                                .video_first_rtp_source
                                .map(|addr| addr.to_string())
                                .unwrap_or_default(),
                            result.rejection_reason.as_deref().unwrap_or("")
                        ))
                    } else {
                        result.rejection_reason
                    };
                    ("ended", detail)
                } else {
                    (
                        "error",
                        Some(result.rejection_reason.unwrap_or_else(|| {
                            "The Teams call ended before it connected".to_owned()
                        })),
                    )
                };
                emit(CallEvent {
                    kind: kind.to_owned(),
                    call_id,
                    conversation_id: Some(conversation_id.clone()),
                    display_name: None,
                    detail,
                });
            }
            Err(error) => emit(CallEvent {
                kind: "error".to_owned(),
                call_id,
                conversation_id: Some(conversation_id),
                display_name: None,
                detail: Some(format!("{error:#}")),
            }),
        }
        *OUTGOING_STOP.lock().await = None;
    });
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn set_camera_device(selected: Option<String>) {
    calling::camera::set_camera_device(selected);
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn set_camera_device(_: Option<String>) {}

pub async fn hang_up() -> Result<()> {
    if let Some(stop) = OUTGOING_STOP.lock().await.as_ref() {
        let _ = stop.send(true);
        return Ok(());
    }

    let mut active = ACTIVE_INCOMING.lock().await;
    let Some(mut call) = active.take() else {
        return Ok(());
    };
    let call_auth = match auth::service().call_auth().await {
        Ok(call_auth) => call_auth,
        Err(error) => {
            *active = Some(call);
            return Err(error);
        }
    };
    if let Err(error) = calling::signaling::end_call(
        &reqwest::Client::new(),
        &call_auth.skype_token,
        &call.notification,
    )
    .await
    {
        *active = Some(call);
        return Err(error);
    }
    call.media.stop().await;
    if let Some(video) = call.video.as_mut() {
        video.stop().await;
        calling::external_display::clear();
    }
    emit(CallEvent {
        kind: "ended".to_owned(),
        call_id: call.call_id,
        conversation_id: None,
        display_name: None,
        detail: None,
    });
    Ok(())
}

pub async fn accept_call(call_id: String) -> Result<()> {
    let _accept_permit = ACCEPT_INCOMING_GATE
        .try_acquire()
        .context("A call is already active")?;
    if OUTGOING_STOP.lock().await.is_some() || ACTIVE_INCOMING.lock().await.is_some() {
        anyhow::bail!("A call is already active");
    }
    calling::call_test::reset_media_controls();
    let notification = PENDING_INCOMING
        .lock()
        .await
        .as_ref()
        .filter(|pending| pending.call_id == call_id)
        .map(|pending| pending.notification.clone())
        .context("The incoming call is no longer available")?;
    let call_auth = auth::service().call_auth().await?;
    let http = reqwest::Client::new();
    let invitation = notification
        .call_invitation
        .as_ref()
        .context("Incoming call did not include a call invitation")?;
    let blob = invitation
        .media_content
        .as_ref()
        .and_then(|content| content.blob.as_deref())
        .context("Incoming call did not include an SDP media offer")?;
    let offer = calling::sdp::parse_sdp_offer(blob).context("Could not parse incoming call SDP")?;
    let video_requested = invitation
        .call_modalities
        .as_ref()
        .is_some_and(|modalities| modalities.iter().any(|m| m.eq_ignore_ascii_case("video")))
        && offer.video.is_some();
    let remote_crypto = offer
        .crypto_lines
        .iter()
        .find_map(|line| calling::srtp::parse_crypto_line(line).ok())
        .context("Incoming call did not contain audio SRTP keys")?;
    let remote_video_crypto = if video_requested {
        Some(
            offer
                .video
                .as_ref()
                .and_then(|video| {
                    video
                        .crypto_lines
                        .iter()
                        .find_map(|line| calling::srtp::parse_crypto_line(line).ok())
                })
                .context("Incoming video call did not contain video SRTP keys")?,
        )
    } else {
        None
    };

    let audio_socket = tokio::net::UdpSocket::bind("0.0.0.0:0")
        .await
        .context("Could not bind incoming call audio socket")?;
    let audio_port = audio_socket.local_addr()?.port();
    let local_ip = calling::sdp::get_local_ip();
    let mut local_candidates = vec![calling::ice::IceCandidate {
        foundation: "1".to_owned(),
        component: 1,
        transport: calling::ice::Transport::Udp,
        priority: 2130706431,
        address: local_ip.clone(),
        port: audio_port,
        candidate_type: calling::ice::CandidateType::Host,
        raddr: None,
        rport: None,
    }];
    if let Some(srflx) =
        calling::ice::gather_srflx_candidate(&audio_socket, calling::ice::DEFAULT_STUN_SERVER).await
    {
        local_candidates.push(srflx);
    }

    let video_socket = if video_requested {
        Some(
            tokio::net::UdpSocket::bind("0.0.0.0:0")
                .await
                .context("Could not bind incoming call video socket")?,
        )
    } else {
        None
    };
    let video_port = video_socket
        .as_ref()
        .map(|socket| socket.local_addr().map(|addr| addr.port()))
        .transpose()?
        .unwrap_or(0);
    let mut video_candidates = if video_port == 0 {
        Vec::new()
    } else {
        vec![calling::ice::IceCandidate {
            foundation: "1".to_owned(),
            component: 1,
            transport: calling::ice::Transport::Udp,
            priority: 2130706431,
            address: local_ip.clone(),
            port: video_port,
            candidate_type: calling::ice::CandidateType::Host,
            raddr: None,
            rport: None,
        }]
    };
    if let Some(video_socket) = video_socket.as_ref() {
        if let Some(srflx) =
            calling::ice::gather_srflx_candidate(video_socket, calling::ice::DEFAULT_STUN_SERVER)
                .await
        {
            video_candidates.push(srflx);
        }
    }

    let answer = calling::sdp::generate_sdp_answer_full(
        &local_ip,
        audio_port,
        video_port,
        &offer,
        &local_candidates,
        &video_candidates,
    );
    let local_crypto = calling::srtp::parse_crypto_line(&answer.audio_crypto_line)
        .context("Could not prepare local audio SRTP keys")?;
    let local_video_crypto = answer
        .video_crypto_line
        .as_ref()
        .map(|line| calling::srtp::parse_crypto_line(line))
        .transpose()
        .context("Could not prepare local video SRTP keys")?;

    calling::signaling::send_media_answer(
        &http,
        &call_auth.skype_token,
        &notification,
        &answer.sdp,
    )
    .await?;
    calling::signaling::accept_call(
        &http,
        &call_auth.skype_token,
        &notification,
        video_requested,
    )
    .await?;
    clear_pending_call(&call_id).await;

    let candidates = calling::ice::parse_candidates_from_sdp(blob);
    let local_creds = calling::ice::IceCredentials {
        ufrag: answer.audio_ice_ufrag,
        pwd: answer.audio_ice_pwd,
    };
    let remote_creds = calling::ice::IceCredentials {
        ufrag: offer.ice_ufrag.clone(),
        pwd: offer.ice_pwd.clone(),
    };
    let mut media = match calling::media::MediaSession::start_with_bound_ice(
        audio_socket,
        &candidates,
        &local_creds,
        &remote_creds,
        &local_crypto,
        &remote_crypto,
    )
    .await
    {
        Ok(media) => media,
        Err(media_error) => {
            let detail = match calling::signaling::end_call(
                &http,
                &call_auth.skype_token,
                &notification,
            )
            .await
            {
                Ok(()) => format!("Could not start incoming call audio: {media_error:#}"),
                Err(end_error) => format!(
                    "Could not start incoming call audio: {media_error:#}; remote hang-up also failed: {end_error:#}"
                ),
            };
            emit(CallEvent {
                kind: "error".to_owned(),
                call_id,
                conversation_id: None,
                display_name: None,
                detail: Some(detail),
            });
            return Ok(());
        }
    };

    let video = if let (
        Some(video_socket),
        Some(local_video_crypto),
        Some(remote_video_crypto),
        Some(video_offer),
        Some(video_ufrag),
        Some(video_pwd),
    ) = (
        video_socket,
        local_video_crypto,
        remote_video_crypto,
        offer.video.as_ref(),
        answer.video_ice_ufrag,
        answer.video_ice_pwd,
    ) {
        calling::external_display::clear();
        let video_remote_candidates = calling::ice::parse_main_video_candidates_from_sdp(blob);
        let video_local_creds = calling::ice::IceCredentials {
            ufrag: video_ufrag,
            pwd: video_pwd,
        };
        let video_remote_creds = calling::ice::IceCredentials {
            ufrag: video_offer.ice_ufrag.clone(),
            pwd: video_offer.ice_pwd.clone(),
        };
        match calling::media::VideoMediaSession::start_receive_with_bound_ice(
            video_socket,
            &video_remote_candidates,
            &video_local_creds,
            &video_remote_creds,
            &local_video_crypto,
            &remote_video_crypto,
        )
        .await
        {
            Ok(video) => Some(video),
            Err(video_error) => {
                let end_result =
                    calling::signaling::end_call(&http, &call_auth.skype_token, &notification)
                        .await;
                media.stop().await;
                calling::external_display::clear();
                let detail = match end_result {
                    Ok(()) => format!("Could not start incoming call video: {video_error:#}"),
                    Err(end_error) => format!(
                        "Could not start incoming call video: {video_error:#}; remote hang-up also failed: {end_error:#}"
                    ),
                };
                emit(CallEvent {
                    kind: "error".to_owned(),
                    call_id,
                    conversation_id: None,
                    display_name: None,
                    detail: Some(detail),
                });
                return Ok(());
            }
        }
    } else {
        None
    };

    *ACTIVE_INCOMING.lock().await = Some(ActiveIncomingCall {
        call_id: call_id.clone(),
        notification,
        media,
        video,
    });
    emit(CallEvent {
        kind: "connected".to_owned(),
        call_id,
        conversation_id: None,
        display_name: None,
        detail: video_requested.then_some("video=true".to_owned()),
    });
    Ok(())
}

pub async fn decline_call(call_id: String) -> Result<()> {
    let notification = PENDING_INCOMING
        .lock()
        .await
        .as_ref()
        .filter(|pending| pending.call_id == call_id)
        .map(|pending| pending.notification.clone())
        .context("The incoming call is no longer available")?;
    let call_auth = auth::service().call_auth().await?;
    calling::signaling::end_call(
        &reqwest::Client::new(),
        &call_auth.skype_token,
        &notification,
    )
    .await?;
    clear_pending_call(&call_id).await;
    emit(CallEvent {
        kind: "ended".to_owned(),
        call_id,
        conversation_id: None,
        display_name: None,
        detail: Some("declined".to_owned()),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::incoming::{notification_call_id, register_incoming_call};
    use super::{
        remote_video_frame, start_call, wait_for_retry_or_stop, ACCEPT_INCOMING_GATE,
        PENDING_INCOMING,
    };
    use ost_microsoft::calling as microsoft_calling;
    use std::time::Duration;
    use teams_cli::calling::parse_call_notification;

    #[test]
    fn remote_video_frame_converts_i420_to_rgba() {
        teams_cli::calling::external_display::push_frame(2, 2, vec![82, 82, 82, 82, 90, 240]);
        let frame = remote_video_frame().unwrap();
        assert_eq!((frame.width, frame.height), (2, 2));
        assert_eq!(frame.rgba.len(), 16);
        assert_eq!(&frame.rgba[..4], &[255, 1, 0, 255]);
        teams_cli::calling::external_display::clear();
    }

    #[test]
    fn test_empty_incoming_call_id_uses_operation_id() {
        let notification = parse_call_notification(
            r#"{"callInvitation":{},"debugContent":{"callId":"","operationId":"op-123"}}"#,
        )
        .unwrap();

        assert_eq!(notification_call_id(&notification), "operation:op-123");
    }

    #[test]
    fn test_missing_incoming_call_identity_generates_nonempty_id() {
        let notification = parse_call_notification(r#"{"callInvitation":{}}"#).unwrap();

        assert!(!notification_call_id(&notification).is_empty());
    }

    #[tokio::test]
    async fn pending_incoming_call_blocks_other_call_creation() {
        *PENDING_INCOMING.lock().await = None;
        let notification = parse_call_notification(
            r#"{"callInvitation":{},"debugContent":{"callId":"call-one"}}"#,
        )
        .unwrap();

        register_incoming_call(notification.clone()).await;
        register_incoming_call(notification).await;
        assert_eq!(
            PENDING_INCOMING
                .lock()
                .await
                .as_ref()
                .map(|pending| pending.call_id.as_str()),
            Some("call-one")
        );
        assert!(start_call("chat-two".to_owned()).await.is_err());
        *PENDING_INCOMING.lock().await = None;

        let mut events = super::CALL_EVENTS.subscribe();
        register_incoming_call(parse_call_notification(
            r#"{"callInvitation":{"callModalities":["Audio","Video"]},"groupChat":{"threadId":"stand-up"},"participants":{"from":{"id":"caller","displayName":"Ada"}},"debugContent":{"callId":"group-video"}}"#,
        ).unwrap()).await;
        let event = events.try_recv().unwrap();
        assert_eq!(event.conversation_id.as_deref(), Some("stand-up"));
        assert_eq!(event.detail.as_deref(), Some("video=true"));
        register_incoming_call(
            microsoft_calling::parse_call_event(
                r#"{"callEnd":{"phrase":"Remote hangup"},"debugContent":{"callId":"other-call"}}"#,
            )
            .unwrap(),
        )
        .await;
        assert!(PENDING_INCOMING.lock().await.is_some());
        assert!(events.try_recv().is_err());
        let accepting = ACCEPT_INCOMING_GATE.try_acquire().unwrap();
        let remote_end = tokio::spawn(register_incoming_call(
            microsoft_calling::parse_call_event(
                r#"{"callEnd":{"phrase":"Remote hangup"},"debugContent":{"callId":"group-video"}}"#,
            )
            .unwrap(),
        ));
        tokio::task::yield_now().await;
        assert!(!remote_end.is_finished());
        assert!(events.try_recv().is_err());
        drop(accepting);
        remote_end.await.unwrap();
        assert!(PENDING_INCOMING.lock().await.is_none());
        let ended = events.try_recv().unwrap();
        assert_eq!(ended.kind, "ended");
        assert_eq!(ended.call_id, "group-video");
        assert_eq!(ended.detail.as_deref(), Some("Remote hangup"));

        let permit = ACCEPT_INCOMING_GATE.try_acquire().unwrap();
        assert!(start_call("chat-three".to_owned()).await.is_err());
        drop(permit);
    }

    #[tokio::test]
    async fn test_retry_delay_stops_immediately_on_hangup() {
        let (stop_sender, stop_receiver) = tokio::sync::watch::channel(false);
        let stop_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            stop_sender.send(true).unwrap();
        });

        let should_retry = wait_for_retry_or_stop(&stop_receiver, Duration::from_secs(1)).await;
        stop_task.await.unwrap();

        assert!(!should_retry);
    }

    #[tokio::test]
    async fn test_retry_delay_skips_when_already_stopped() {
        let (stop_sender, stop_receiver) = tokio::sync::watch::channel(false);
        stop_sender.send(true).unwrap();

        assert!(!wait_for_retry_or_stop(&stop_receiver, Duration::from_secs(1)).await);
    }
}
