use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use rtc::interceptor::Registry;
use rtc::media::Sample;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{
    MediaEngine, MIME_TYPE_H264, MIME_TYPE_OPUS,
};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
};
use webrtc::media_stream::track_local::static_sample::TrackLocalStaticSample;
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::rtp_transceiver::RtpSender;
use webrtc::runtime::{channel, Receiver, Sender};

struct Handler {
    gathered: Sender<()>,
    connected: Sender<()>,
}

#[async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gathered.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        tracing::info!("Personal WebRTC connection state: {state}");
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected.try_send(());
        }
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        tokio::spawn(async move {
            while let Some(event) = track.poll().await {
                if matches!(event, TrackRemoteEvent::OnEnded) {
                    break;
                }
            }
        });
    }
}

pub struct PersonalWebRtcSession {
    peer: Arc<dyn PeerConnection>,
    connected: Receiver<()>,
    video_track: Option<Arc<TrackLocalStaticSample>>,
    video_sender: Option<Arc<dyn RtpSender>>,
    video_ssrc: Option<u32>,
}

impl PersonalWebRtcSession {
    pub async fn create(include_video: bool) -> Result<(Self, String)> {
        let mut media_engine = MediaEngine::default();
        media_engine.register_default_codecs()?;
        let interceptors = register_default_interceptors(Registry::new(), &mut media_engine)?;
        let (gathered_tx, mut gathered_rx) = channel(1);
        let (connected_tx, connected_rx) = channel(1);
        let peer: Arc<dyn PeerConnection> = Arc::new(
            PeerConnectionBuilder::new()
                .with_media_engine(media_engine)
                .with_interceptor_registry(interceptors)
                .with_handler(Arc::new(Handler {
                    gathered: gathered_tx,
                    connected: connected_tx,
                }))
                .with_udp_addrs(vec!["0.0.0.0:0".to_owned()])
                .build()
                .await
                .context("Failed to create personal WebRTC peer connection")?,
        );

        let audio_track = Arc::new(TrackLocalStaticSample::new(MediaStreamTrack::new(
            "main-audio".to_owned(),
            "main-audio".to_owned(),
            "main-audio".to_owned(),
            RtpCodecKind::Audio,
            vec![RTCRtpEncodingParameters {
                rtp_coding_parameters: RTCRtpCodingParameters {
                    ssrc: Some(rand_ssrc()),
                    ..Default::default()
                },
                codec: RTCRtpCodec {
                    mime_type: MIME_TYPE_OPUS.to_owned(),
                    clock_rate: 48_000,
                    channels: 2,
                    sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                    rtcp_feedback: vec![],
                },
                ..Default::default()
            }],
        ))?);
        peer.add_track(audio_track as Arc<dyn TrackLocal>)
            .await
            .context("Failed to add personal WebRTC audio track")?;

        let (video_track, video_sender, video_ssrc) = if include_video {
            let ssrc = rand_ssrc();
            let track = Arc::new(TrackLocalStaticSample::new(MediaStreamTrack::new(
                "main-video".to_owned(),
                "main-video".to_owned(),
                "main-video".to_owned(),
                RtpCodecKind::Video,
                vec![RTCRtpEncodingParameters {
                    rtp_coding_parameters: RTCRtpCodingParameters {
                        ssrc: Some(ssrc),
                        ..Default::default()
                    },
                    codec: RTCRtpCodec {
                        mime_type: MIME_TYPE_H264.to_owned(),
                        clock_rate: 90_000,
                        channels: 0,
                        sdp_fmtp_line:
                            "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f"
                                .to_owned(),
                        rtcp_feedback: vec![],
                    },
                    ..Default::default()
                }],
            ))?);
            let sender = peer
                .add_track(Arc::clone(&track) as Arc<dyn TrackLocal>)
                .await
                .context("Failed to add personal WebRTC video track")?;
            (Some(track), Some(sender), Some(ssrc))
        } else {
            (None, None, None)
        };

        let offer = peer
            .create_offer(None)
            .await
            .context("Failed to create personal WebRTC offer")?;
        peer.set_local_description(offer)
            .await
            .context("Failed to set personal WebRTC local description")?;
        tokio::time::timeout(Duration::from_secs(8), gathered_rx.recv())
            .await
            .context("Timed out gathering personal WebRTC ICE candidates")?;
        let offer = peer
            .local_description()
            .await
            .context("Personal WebRTC local description disappeared")?;

        Ok((
            Self {
                peer,
                connected: connected_rx,
                video_track,
                video_sender,
                video_ssrc,
            },
            offer.sdp,
        ))
    }

    pub async fn set_answer(&self, sdp: String) -> Result<()> {
        let answer = RTCSessionDescription::answer(sdp)
            .context("Failed to parse personal WebRTC answer SDP")?;
        self.peer
            .set_remote_description(answer)
            .await
            .context("Failed to apply personal WebRTC answer")
    }

    pub async fn wait_connected(&mut self, timeout: Duration) -> Result<()> {
        tokio::time::timeout(timeout, self.connected.recv())
            .await
            .context("Timed out establishing personal WebRTC media")?;
        Ok(())
    }

    pub fn has_video_track(&self) -> bool {
        self.video_track.is_some()
    }

    pub async fn write_h264_nals(&self, nals: &[Vec<u8>], duration: Duration) -> Result<()> {
        let track = self
            .video_track
            .as_ref()
            .context("Personal WebRTC session has no video track")?;
        let sender = self
            .video_sender
            .as_ref()
            .context("Personal WebRTC session has no video sender")?;
        let ssrc = self
            .video_ssrc
            .context("Personal WebRTC session has no video SSRC")?;
        let params = sender
            .get_parameters()
            .await
            .context("Failed to read personal WebRTC video parameters")?;
        let payload_type = params
            .rtp_parameters
            .codecs
            .iter()
            .find(|codec| {
                codec
                    .rtp_codec
                    .mime_type
                    .eq_ignore_ascii_case(MIME_TYPE_H264)
            })
            .or_else(|| params.rtp_parameters.codecs.first())
            .context("No negotiated personal WebRTC video codec")?
            .payload_type;
        let mut annexb = Vec::with_capacity(nals.iter().map(|nal| nal.len() + 4).sum());
        for nal in nals {
            annexb.extend_from_slice(&[0, 0, 0, 1]);
            annexb.extend_from_slice(nal);
        }
        track
            .sample_writer(ssrc, payload_type)
            .write_sample(&Sample {
                data: annexb.into(),
                duration,
                ..Default::default()
            })
            .await
            .context("Failed to send personal WebRTC H264 sample")
    }
}

fn rand_ssrc() -> u32 {
    let mut bytes = [0u8; 4];
    getrandom::getrandom(&mut bytes).expect("OS CSPRNG failed");
    u32::from_ne_bytes(bytes)
}
