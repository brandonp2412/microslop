//! Call signaling module — parse incoming call invitations and manage call lifecycle.
//!
//! This handles signaling only (no media streaming).

#[cfg(feature = "audio")]
pub mod audio;
pub mod call_test;
#[cfg(any(feature = "video-capture", feature = "video-capture-windows"))]
pub mod camera;
#[cfg(feature = "video-codec")]
pub mod codec;
#[cfg(feature = "video-capture")]
pub mod display;
#[cfg(feature = "video-codec")]
pub mod external_camera;
#[cfg(feature = "video-codec")]
pub mod external_display;
pub mod ice;
pub mod media;
#[cfg(feature = "consumer-webrtc")]
pub mod personal_webrtc;
pub mod recording;
pub mod rtcp;
pub mod rtp;
pub mod sdp;
pub mod sdp_compress;
pub mod signaling;
pub mod srtp;
pub mod test_tone;
pub mod turn;
pub mod video;

pub use ost_microsoft::calling::{parse_call_notification, CallNotification};

/// Simple call lifecycle state.
#[derive(Debug, Clone, PartialEq)]
pub enum CallState {
    Idle,
    Ringing,
    Accepting,
    Connected,
    Ended,
}
