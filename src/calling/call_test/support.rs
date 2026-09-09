use std::sync::{
    atomic::{AtomicI32, AtomicU32},
    Arc,
};

use anyhow::{Context, Result};
use serde::Deserialize;
use tokio::sync::Mutex;

#[cfg(feature = "video-capture")]
use crate::calling::display;
#[cfg(feature = "video-codec")]
use crate::calling::video;
use crate::calling::{rtcp, srtp};

pub(super) fn derive_epconv_url(region_gtms: &serde_json::Value) -> Option<String> {
    if let Ok(url) = std::env::var("TEAMS_EPCONV_URL") {
        return Some(url);
    }
    if let Some(url) = region_gtms
        .get("calling_conversationServiceUrl")
        .and_then(|value| value.as_str())
    {
        return Some(url.to_string());
    }
    let potential_url = region_gtms
        .get("calling_potentialCallRequestUrl")
        .and_then(|value| value.as_str())?;
    if let Some(index) = potential_url.find("/api/v2/") {
        Some(format!("{}/api/v2/epconv", &potential_url[..index]))
    } else {
        Some(potential_url.replace("/cc/v1/potentialcall", "/epconv"))
    }
}

pub(super) struct MediaLeg {
    pub(super) label: String,
    pub(super) audio_socket: Arc<tokio::net::UdpSocket>,
    pub(super) audio_srtp_ctx: Arc<Mutex<srtp::SrtpContext>>,
    pub(super) audio_remote_addr: std::net::SocketAddr,
    pub(super) audio_local_pwd: String,
    pub(super) audio_ssrc: u32,
    pub(super) video_socket: Arc<tokio::net::UdpSocket>,
    pub(super) video_srtp_ctx: Option<Arc<Mutex<srtp::SrtpContext>>>,
    pub(super) video_remote_addr: Option<std::net::SocketAddr>,
    pub(super) video_local_crypto_line: String,
    pub(super) video_local_ufrag: String,
    pub(super) video_local_pwd: String,
    pub(super) video_controlling: bool,
    pub(super) video_ssrc: u32,
    #[cfg(feature = "video-codec")]
    pub(super) camera_rx: Option<std::sync::mpsc::Receiver<video::YuvFrame>>,
    #[cfg(feature = "video-capture")]
    pub(super) display_tx: Option<std::sync::mpsc::SyncSender<display::DisplayFrame>>,
    pub(super) decode_remote_video: bool,
}

pub(super) struct MediaLegHandles {
    pub(super) send_stats: Arc<Mutex<rtcp::RtpSendStats>>,
    pub(super) recv_stats: Arc<Mutex<rtcp::RtpRecvStats>>,
    pub(super) video_send_stats: Arc<Mutex<rtcp::RtpSendStats>>,
    pub(super) video_recv_stats: Arc<Mutex<rtcp::RtpRecvStats>>,
    pub(super) video_socket: Arc<tokio::net::UdpSocket>,
    pub(super) video_srtp_ctx: Option<Arc<Mutex<srtp::SrtpContext>>>,
    pub(super) video_dynamic_remote_addr: Arc<Mutex<Option<std::net::SocketAddr>>>,
    pub(super) video_local_crypto_line: String,
    pub(super) video_local_ufrag: String,
    pub(super) video_local_pwd: String,
    pub(super) video_controlling: bool,
    pub(super) camera_frames_sent: Arc<AtomicU32>,
    pub(super) raw_audio_datagrams: Arc<AtomicU32>,
    pub(super) audio_stun_datagrams: Arc<AtomicU32>,
    pub(super) audio_srtp_decrypt_failures: Arc<AtomicU32>,
    pub(super) audio_initial_remote_addr: std::net::SocketAddr,
    pub(super) audio_dynamic_remote_addr: Arc<Mutex<std::net::SocketAddr>>,
    pub(super) audio_first_rtp_source: Arc<Mutex<Option<std::net::SocketAddr>>>,
    pub(super) raw_video_datagrams: Arc<AtomicU32>,
    pub(super) video_stun_datagrams: Arc<AtomicU32>,
    pub(super) video_srtcp_datagrams: Arc<AtomicU32>,
    pub(super) video_srtp_decrypt_failures: Arc<AtomicU32>,
    pub(super) video_vsr_requests_sent: Arc<AtomicU32>,
    pub(super) video_keyframe_requests_received: Arc<AtomicU32>,
    pub(super) video_receiver_reports_for_local_ssrc: Arc<AtomicU32>,
    pub(super) video_rr_fraction_lost: Arc<AtomicU32>,
    pub(super) video_rr_cumulative_lost: Arc<AtomicI32>,
    pub(super) video_rr_extended_highest_seq: Arc<AtomicU32>,
    pub(super) video_rr_jitter: Arc<AtomicU32>,
    pub(super) video_first_sequence_sent: Arc<AtomicU32>,
    pub(super) video_last_sequence_sent: Arc<AtomicU32>,
    pub(super) video_initial_remote_addr: Option<std::net::SocketAddr>,
    pub(super) video_first_rtp_source: Arc<Mutex<Option<std::net::SocketAddr>>>,
    pub(super) microphone_frames: Arc<AtomicU32>,
    pub(super) microphone_non_silent_frames: Arc<AtomicU32>,
    pub(super) microphone_peak: Arc<AtomicU32>,
    pub(super) handles: Vec<tokio::task::JoinHandle<()>>,
}

impl MediaLegHandles {
    pub(super) fn abort_all(&self) {
        for handle in &self.handles {
            handle.abort();
        }
    }
}

#[derive(Debug)]
pub(super) struct CallAcceptanceResponse {
    pub(super) sdp_blob: Option<String>,
    pub(super) end_url: Option<String>,
    pub(super) rejection_reason: Option<String>,
    pub(super) acknowledgement_url: Option<String>,
    pub(super) call_leg_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct MeResponse {
    #[serde(rename = "displayName")]
    pub(super) display_name: Option<String>,
    pub(super) mail: Option<String>,
}

pub(super) async fn fetch_me(http: &reqwest::Client, graph_token: &str) -> Result<MeResponse> {
    let response = http
        .get(format!("{}/me", ost_microsoft::teams::GRAPH_BASE))
        .bearer_auth(graph_token)
        .send()
        .await
        .context("Failed to GET /me")?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Graph /me returned {}: {}", status, body);
    }
    response
        .json()
        .await
        .context("Failed to parse /me response")
}
