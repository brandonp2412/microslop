//! H.264 RTP packetization (RFC 6184 + RFC 6190 SVC extensions) and black frame generation.
//!
//! Implements X-H264UC packetization per MS-H264PF:
//! - PACSI NAL unit (type 30) with Stream Layout SEI and Bitstream Info SEI
//! - Prefix NAL units (type 14) before coded slices
//! - Single NAL Unit mode and FU-A fragmentation

use std::sync::OnceLock;

use anyhow::{bail, Result};

pub struct YuvFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// H.264 payload type (dynamic, matching our SDP).
pub const PT_H264: u8 = 122;

/// Video clock rate (90 kHz per RTP spec for video).
pub const CLOCK_RATE: u32 = 90000;

/// Maximum RTP payload size before fragmentation.
pub const MTU: usize = 1200;

pub const FRAME_INTERVAL_TICKS: u32 = CLOCK_RATE / 30;

pub const FRAME_INTERVAL_MS: u64 = 33;

/// X-H264UC reserves 100 SSRCs for temporal/spatial SVC layers (T0-T3, L0-L2).
pub const VIDEO_SSRC_RANGE_SIZE: u32 = 100;

/// Generate a random SSRC via OS CSPRNG.
pub fn generate_ssrc() -> u32 {
    let mut buf = [0u8; 4];
    getrandom::getrandom(&mut buf).expect("OS CSPRNG failed");
    u32::from_be_bytes(buf)
}

const NAL_TYPE_SLICE: u8 = 1;
const NAL_TYPE_IDR: u8 = 5;
#[cfg(test)]
const NAL_TYPE_SPS: u8 = 7;
#[cfg(test)]
const NAL_TYPE_PPS: u8 = 8;
const NAL_TYPE_PREFIX: u8 = 14;
const NAL_TYPE_FU_A: u8 = 28;
const NAL_TYPE_PACSI: u8 = 30;

// FU-A header bits
const FU_START_BIT: u8 = 0x80;
const FU_END_BIT: u8 = 0x40;

/// Stream Layout SEI UUID: {139FB1A9-446A-4DEC-8CBF-65B1E12D2CFD}
const STREAM_LAYOUT_UUID: [u8; 16] = [
    0x13, 0x9F, 0xB1, 0xA9, 0x44, 0x6A, 0x4D, 0xEC, 0x8C, 0xBF, 0x65, 0xB1, 0xE1, 0x2D, 0x2C, 0xFD,
];

/// Bitstream Info SEI UUID: {05FBC6B9-5A80-40E5-A22A-AB4020267E26}
const BITSTREAM_INFO_UUID: [u8; 16] = [
    0x05, 0xFB, 0xC6, 0xB9, 0x5A, 0x80, 0x40, 0xE5, 0xA2, 0x2A, 0xAB, 0x40, 0x20, 0x26, 0x7E, 0x26,
];

/// Video stream configuration for PACSI generation.
#[derive(Clone, Debug)]
pub struct SvcConfig {
    pub coded_width: u16,
    pub coded_height: u16,
    pub display_width: u16,
    pub display_height: u16,
    pub bitrate: u32,
    pub fps_idx: u8,
    pub constrained_baseline: bool,
}

impl Default for SvcConfig {
    fn default() -> Self {
        Self {
            coded_width: 320,
            coded_height: 240,
            display_width: 320,
            display_height: 240,
            bitrate: 256000,
            fps_idx: ost_microsoft::calling::VIDEO_FPS_INDEX,
            constrained_baseline: true,
        }
    }
}

/// Build the Stream Layout SEI message payload (everything after the NAL header byte).
///
/// This is a User Data Unregistered SEI (payloadType=5) containing:
/// - 16-byte UUID
/// - 8-byte Layer Presence Bitmask (bit 0 set for PRID=0)
fn build_stream_layout_sei(cfg: &SvcConfig) -> Vec<u8> {
    let mut layer_desc = Vec::with_capacity(16);
    layer_desc.extend_from_slice(&cfg.coded_width.to_be_bytes());
    layer_desc.extend_from_slice(&cfg.coded_height.to_be_bytes());
    layer_desc.extend_from_slice(&cfg.display_width.to_be_bytes());
    layer_desc.extend_from_slice(&cfg.display_height.to_be_bytes());
    layer_desc.extend_from_slice(&cfg.bitrate.to_be_bytes());
    let byte12 = cfg.fps_idx << 3;
    let byte13 = if cfg.constrained_baseline { 0x02 } else { 0x00 };
    layer_desc.push(byte12);
    layer_desc.push(byte13);
    layer_desc.push(0x00);
    layer_desc.push(0x00);

    let payload_size: u8 = 16 + 8 + 1 + 1 + 16;

    let mut sei = Vec::with_capacity(2 + payload_size as usize);
    sei.push(5u8);
    sei.push(payload_size);
    sei.extend_from_slice(&STREAM_LAYOUT_UUID);
    sei.push(0x01);
    sei.push(0x00);
    sei.push(0x00);
    sei.push(0x00);
    sei.push(0x00);
    sei.push(0x00);
    sei.push(0x00);
    sei.push(0x00);
    sei.push(0x01);
    sei.push(16u8);
    sei.extend_from_slice(&layer_desc);

    sei
}

/// Build the Bitstream Info SEI message payload (everything after the NAL header byte).
///
/// payloadType=5, UUID, ref_frm_cnt, num_of_nal_unit.
fn build_bitstream_info_sei(ref_frm_cnt: u8, num_nal_units: u8) -> Vec<u8> {
    let payload_size: u8 = 18;

    let mut sei = Vec::with_capacity(2 + payload_size as usize);
    sei.push(5u8);
    sei.push(payload_size);
    sei.extend_from_slice(&BITSTREAM_INFO_UUID);
    sei.push(ref_frm_cnt);
    sei.push(num_nal_units);

    sei
}

/// Build the 3-byte SVC extension header per RFC 6190.
///
///   Byte 0: R(1)=1 | I(1)=idr_flag | PRID(6)=0
///   Byte 1: N(1)=1 | DID(3)=0 | QID(4)=0
///   Byte 2: TID(3)=0 | U(1)=0 | D(1)=0 | O(1)=1 | RR(2)=3
fn svc_extension_bytes(is_idr: bool) -> [u8; 3] {
    [0x80 | if is_idr { 0x40 } else { 0x00 }, 0x80, 0x07]
}

/// Build a complete PACSI NAL unit (type 30) containing Stream Layout and Bitstream Info SEIs.
///
///   Byte 0:    NAL header (F=0, NRI=3, Type=30)
///   Bytes 1-3: SVC extension header (for base layer)
///   Remaining: one or more length-prefixed SEI NAL units (type 6)
///
/// The Microsoft SEI messages are embedded as separate NAL units inside the PACSI.
/// Per MS-H264PF, no emulation prevention bytes are inserted.
fn append_pacsi_sei(pacsi: &mut Vec<u8>, sei_payload: &[u8]) {
    let nal_size = u16::try_from(sei_payload.len() + 1).expect("PACSI SEI NAL exceeds u16 size");
    pacsi.extend_from_slice(&nal_size.to_be_bytes());
    pacsi.push(0x06);
    pacsi.extend_from_slice(sei_payload);
}

fn build_pacsi_nal(
    cfg: &SvcConfig,
    ref_frm_cnt: u8,
    num_nal_units: u8,
    donc: u16,
    has_idr: bool,
    associated_nal_header: u8,
    include_stream_layout: bool,
) -> Vec<u8> {
    let mut pacsi = Vec::with_capacity(128);

    pacsi.push((associated_nal_header & 0xE0) | NAL_TYPE_PACSI);
    pacsi.extend_from_slice(&svc_extension_bytes(has_idr));
    pacsi.push(0x20);
    pacsi.extend_from_slice(&donc.to_be_bytes());

    if include_stream_layout {
        let stream_layout = build_stream_layout_sei(cfg);
        append_pacsi_sei(&mut pacsi, &stream_layout);
    }

    let bitstream_info = build_bitstream_info_sei(ref_frm_cnt, num_nal_units);
    append_pacsi_sei(&mut pacsi, &bitstream_info);

    pacsi
}

/// Build a prefix NAL unit (type 14) to prepend before a coded slice NAL.
///
///   Byte 0:    NAL header: F/NRI copied from the associated slice, Type=14
///   Bytes 1-3: SVC extension header
///   Byte 4:    prefix_nal_unit_svc RBSP for reference slices:
///              store_ref_base_pic_flag=0,
///              additional_prefix_nal_unit_extension_flag=0,
///              followed by rbsp_trailing_bits.
fn build_prefix_nal(slice_nal_header: u8, is_idr: bool) -> Vec<u8> {
    let forbidden_and_nri = slice_nal_header & 0xE0;
    let header = forbidden_and_nri | NAL_TYPE_PREFIX;
    let svc = svc_extension_bytes(is_idr);
    let mut prefix = vec![header, svc[0], svc[1], svc[2]];
    if slice_nal_header & 0x60 != 0 {
        prefix.push(0x20);
    }
    prefix
}

/// Packetizes H.264 NAL units into RTP payloads with X-H264UC SVC wrapping.
///
/// 1. Sends a PACSI NAL unit (type 30) as the first RTP packet
/// 2. Sends SPS/PPS NAL units as-is (single NAL mode)
/// 3. Prepends a prefix NAL unit (type 14) before each coded slice, then
///    sends the slice via single NAL or FU-A fragmentation
pub struct VideoPacketizer {
    ssrc: u32,
    seq: u16,
    timestamp: u32,
    ref_frm_cnt: u8,
    cs_don: u16,
    svc_config: SvcConfig,
    stream_layout_sent: bool,
}

impl VideoPacketizer {
    fn random_initial_state() -> (u16, u32, u8, u16) {
        let mut buf = [0u8; 9];
        getrandom::getrandom(&mut buf).expect("OS CSPRNG failed");
        let mut seq = u16::from_be_bytes([buf[0], buf[1]]);
        if seq == 0 {
            seq = 1;
        }
        (
            seq,
            u32::from_be_bytes([buf[2], buf[3], buf[4], buf[5]]),
            buf[6],
            u16::from_be_bytes([buf[7], buf[8]]),
        )
    }

    pub fn new(ssrc: u32) -> Self {
        let (seq, timestamp, ref_frm_cnt, cs_don) = Self::random_initial_state();
        Self {
            ssrc,
            seq,
            timestamp,
            ref_frm_cnt,
            cs_don,
            svc_config: SvcConfig::default(),
            stream_layout_sent: false,
        }
    }

    pub fn with_config(ssrc: u32, config: SvcConfig) -> Self {
        let (seq, timestamp, ref_frm_cnt, cs_don) = Self::random_initial_state();
        Self {
            ssrc,
            seq,
            timestamp,
            ref_frm_cnt,
            cs_don,
            svc_config: config,
            stream_layout_sent: false,
        }
    }

    pub fn set_frame_dimensions(&mut self, width: u16, height: u16) {
        self.svc_config.coded_width = width.next_multiple_of(16);
        self.svc_config.coded_height = height.next_multiple_of(16);
        self.svc_config.display_width = width;
        self.svc_config.display_height = height;
    }

    /// Packetize a single H.264 access unit (frame) consisting of multiple NAL units.
    ///
    /// Wraps the frame in X-H264UC SVC containers:
    /// - PACSI NAL (type 30) sent first, with Bitstream Info on every frame
    /// - Full Stream Layout on the first PACSI and every IDR PACSI
    /// - Prefix NAL (type 14) before each coded slice (types 1, 5)
    ///
    /// Returns a list of complete RTP packets (header + payload) ready for SRTP.
    /// The marker bit is set on the last packet of the access unit.
    pub fn packetize_frame(&mut self, nal_units: &[Vec<u8>]) -> Vec<Vec<u8>> {
        if nal_units.iter().all(Vec::is_empty) {
            self.timestamp = self.timestamp.wrapping_add(FRAME_INTERVAL_TICKS);
            return Vec::new();
        }
        let mut packets = Vec::new();

        let has_idr = nal_units
            .iter()
            .any(|n| !n.is_empty() && (n[0] & 0x1F) == NAL_TYPE_IDR);

        let encoder_nals = nal_units.iter().filter(|n| !n.is_empty()).count();
        let prefix_nals = nal_units
            .iter()
            .filter(|nal| !nal.is_empty() && matches!(nal[0] & 0x1f, NAL_TYPE_SLICE | NAL_TYPE_IDR))
            .count();
        let access_unit_nals = encoder_nals + prefix_nals;
        let num_nal_units = access_unit_nals.min(255) as u8;

        let associated_nal_header = nal_units
            .iter()
            .find_map(|nal| nal.first().copied())
            .unwrap_or(0);
        let include_stream_layout = !self.stream_layout_sent || has_idr;
        let pacsi = build_pacsi_nal(
            &self.svc_config,
            self.ref_frm_cnt,
            num_nal_units,
            self.cs_don,
            has_idr,
            associated_nal_header,
            include_stream_layout,
        );
        if include_stream_layout {
            self.stream_layout_sent = true;
        }
        packets.push(self.build_rtp_packet(&[], &pacsi, false));

        let last_nonempty_index = nal_units.iter().rposition(|nal| !nal.is_empty());
        for (i, nal) in nal_units.iter().enumerate() {
            if nal.is_empty() {
                continue;
            }
            let is_last_nal = Some(i) == last_nonempty_index;
            let nal_type = nal[0] & 0x1F;

            match nal_type {
                NAL_TYPE_SLICE | NAL_TYPE_IDR => {
                    // Send prefix NAL (type 14) before the slice — never marker
                    let prefix = build_prefix_nal(nal[0], nal_type == NAL_TYPE_IDR);
                    packets.push(self.build_rtp_packet(&[], &prefix, false));

                    // Send the slice itself
                    let mut slice_packets = self.packetize_nal(nal, is_last_nal);
                    packets.append(&mut slice_packets);
                }
                _ => {
                    let mut nal_packets = self.packetize_nal(nal, is_last_nal);
                    packets.append(&mut nal_packets);
                }
            }
        }

        let is_reference = nal_units.iter().any(|nal| {
            !nal.is_empty()
                && matches!(nal[0] & 0x1f, NAL_TYPE_SLICE | NAL_TYPE_IDR)
                && (nal[0] & 0x60) != 0
        });
        if is_reference {
            self.ref_frm_cnt = self.ref_frm_cnt.wrapping_add(1);
        }
        self.cs_don = self.cs_don.wrapping_add(access_unit_nals as u16);

        self.timestamp = self.timestamp.wrapping_add(FRAME_INTERVAL_TICKS);
        packets
    }

    /// Packetize a single NAL unit. If `is_last` is true, the marker bit is set
    /// on the final RTP packet.
    fn packetize_nal(&mut self, nal: &[u8], is_last: bool) -> Vec<Vec<u8>> {
        if nal.is_empty() {
            return Vec::new();
        }

        if nal.len() <= MTU {
            let marker = is_last;
            let pkt = self.build_rtp_packet(&[], nal, marker);
            vec![pkt]
        } else {
            self.fragment_nal(nal, is_last)
        }
    }

    fn fragment_nal(&mut self, nal: &[u8], is_last_nal: bool) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();
        let nal_header = nal[0];
        let nri = nal_header & 0x60;
        let nal_type = nal_header & 0x1F;

        let fu_indicator = (nal_header & 0x80) | nri | NAL_TYPE_FU_A;

        let payload_data = &nal[1..]; // Skip the NAL header byte
        let max_fragment = MTU - 2; // 2 bytes for FU indicator + FU header
        let mut offset = 0;

        while offset < payload_data.len() {
            let remaining = payload_data.len() - offset;
            let chunk_size = remaining.min(max_fragment);
            let is_first = offset == 0;
            let is_last_fragment = offset + chunk_size >= payload_data.len();

            let mut fu_header = nal_type;
            if is_first {
                fu_header |= FU_START_BIT;
            }
            if is_last_fragment {
                fu_header |= FU_END_BIT;
            }

            let fragment_header = [fu_indicator, fu_header];
            let marker = is_last_nal && is_last_fragment;
            packets.push(self.build_rtp_packet(
                &fragment_header,
                &payload_data[offset..offset + chunk_size],
                marker,
            ));

            offset += chunk_size;
        }

        packets
    }

    fn build_rtp_packet(&mut self, prefix: &[u8], payload: &[u8], marker: bool) -> Vec<u8> {
        let mut buf =
            Vec::with_capacity(super::rtp::RTP_HEADER_SIZE + prefix.len() + payload.len());

        buf.push(0x80);
        let byte1 = if marker { 0x80 | PT_H264 } else { PT_H264 };
        buf.push(byte1);
        buf.extend_from_slice(&self.seq.to_be_bytes());
        buf.extend_from_slice(&self.timestamp.to_be_bytes());
        buf.extend_from_slice(&self.ssrc.to_be_bytes());
        buf.extend_from_slice(prefix);
        buf.extend_from_slice(payload);

        self.seq = self.seq.wrapping_add(1);
        buf
    }
}

/// Reassembles H.264 NAL units from RTP packets.
pub struct VideoDepacketizer {
    fu_buffer: Vec<u8>,
    fu_in_progress: bool,
    pub frames_received: u64,
    pub nals_received: u64,
}

impl Default for VideoDepacketizer {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoDepacketizer {
    pub fn new() -> Self {
        Self {
            fu_buffer: Vec::new(),
            fu_in_progress: false,
            frames_received: 0,
            nals_received: 0,
        }
    }

    /// Process an RTP payload and return a complete NAL unit if one is ready.
    ///
    /// Returns `Ok(Some(nal))` when a complete NAL unit is assembled,
    /// `Ok(None)` when more fragments are needed, or `Err` on protocol error.
    pub fn depacketize(&mut self, rtp_payload: &[u8], marker: bool) -> Result<Option<Vec<u8>>> {
        if rtp_payload.is_empty() {
            bail!("empty RTP payload");
        }

        let nal_type = rtp_payload[0] & 0x1F;

        match nal_type {
            1..=23 | NAL_TYPE_PACSI => {
                self.nals_received += 1;
                if marker {
                    self.frames_received += 1;
                }
                Ok(Some(rtp_payload.to_vec()))
            }
            NAL_TYPE_FU_A => self.depacketize_fu_a(rtp_payload, marker),
            _ => {
                tracing::debug!("Unsupported NAL aggregation type: {}", nal_type);
                Ok(None)
            }
        }
    }

    fn depacketize_fu_a(&mut self, payload: &[u8], marker: bool) -> Result<Option<Vec<u8>>> {
        if payload.len() < 2 {
            bail!("FU-A packet too short");
        }

        let fu_indicator = payload[0];
        let fu_header = payload[1];
        let is_start = fu_header & FU_START_BIT != 0;
        let is_end = fu_header & FU_END_BIT != 0;
        let nal_type = fu_header & 0x1F;
        let nri = fu_indicator & 0x60;

        if is_start {
            // Reconstruct the NAL header byte
            let nal_header = (fu_indicator & 0x80) | nri | nal_type;
            self.fu_buffer.clear();
            self.fu_buffer.push(nal_header);
            self.fu_buffer.extend_from_slice(&payload[2..]);
            self.fu_in_progress = true;
        } else if self.fu_in_progress {
            self.fu_buffer.extend_from_slice(&payload[2..]);
        } else {
            // Middle/end fragment without a start — discard
            return Ok(None);
        }

        if is_end {
            self.fu_in_progress = false;
            self.nals_received += 1;
            if marker {
                self.frames_received += 1;
            }
            let nal = std::mem::take(&mut self.fu_buffer);
            Ok(Some(nal))
        } else {
            Ok(None)
        }
    }
}

/// Generate a minimal black H.264 I-frame at 176x144 resolution.
///
/// Returns a list of NAL units: [SPS, PPS, IDR slice].
/// The IDR slice contains all-zero macroblocks (black).
///
/// This is a hardcoded bitstream — not generated dynamically.
/// Resolution: 176x144 (11x9 macroblocks = 99 MBs), Baseline profile.
pub(crate) fn black_iframe_nals() -> &'static [Vec<u8>] {
    static NALS: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
    NALS.get_or_init(|| {
        let sps: Vec<u8> = vec![
            0x67, // NAL header: type 7 (SPS), NRI=3
            0x42, 0xC0, 0x0C, 0xDA, 0x0F, 0x0A, 0x68,
        ];

        let pps: Vec<u8> = vec![
            0x68, // NAL header: type 8 (PPS), NRI=3
            0xCE, 0x38, // num_slice_groups=0, num_ref_idx=0, weighted_pred=0
            0x80,
        ];

        // IDR slice (type 5): All-skip macroblocks = black
        let mut idr: Vec<u8> = vec![
            0x65, // NAL header: type 5 (IDR), NRI=3
            0x88, // first_mb_in_slice=0, slice_type=7(I), pps_id=0, frame_num=0
            0x80, 0x40, // slice_qp_delta=0, then mb data begins
        ];

        idr.extend(std::iter::repeat_n(0xFF, 25));
        idr.push(0x80);
        vec![sps, pps, idr]
    })
}

pub fn generate_black_iframe() -> Vec<Vec<u8>> {
    black_iframe_nals().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packetizer_never_starts_at_sequence_zero() {
        for _ in 0..128 {
            let mut packetizer = VideoPacketizer::new(0x1234_5678);
            let packets = packetizer.packetize_frame(&generate_black_iframe());
            assert!(!packets.is_empty());
            assert_ne!(u16::from_be_bytes([packets[0][2], packets[0][3]]), 0);
        }
    }

    #[test]
    fn test_packetize_small_nal() {
        let mut p = VideoPacketizer::new(0xAABBCCDD);
        let nal = vec![0x65, 0x00, 0x01, 0x02];
        let packets = p.packetize_frame(std::slice::from_ref(&nal));

        assert_eq!(packets.len(), 3);

        let pacsi_type = packets[0][12] & 0x1F;
        assert_eq!(pacsi_type, NAL_TYPE_PACSI);
        assert_eq!(packets[0][1] & 0x80, 0);

        let prefix_type = packets[1][12] & 0x1F;
        assert_eq!(prefix_type, NAL_TYPE_PREFIX);
        assert_eq!(packets[1][1] & 0x80, 0);

        assert_eq!(&packets[2][12..], &nal[..]);
        assert_eq!(packets[2][1], 0x80 | PT_H264);
    }

    #[test]
    fn test_packetize_large_nal_fragments() {
        let mut p = VideoPacketizer::new(0x12345678);
        let mut nal = vec![0x65]; // IDR NAL header
        nal.extend(vec![0xAB; MTU + 500]);
        let packets = p.packetize_frame(&[nal]);

        assert!(
            packets.len() > 3,
            "should have PACSI + prefix + multiple fragments"
        );

        assert_eq!(packets[0][12] & 0x1F, NAL_TYPE_PACSI);

        assert_eq!(packets[1][12] & 0x1F, NAL_TYPE_PREFIX);

        let fu_indicator = packets[2][12];
        assert_eq!(fu_indicator & 0x1F, NAL_TYPE_FU_A);
        let fu_header = packets[2][13];
        assert!(fu_header & FU_START_BIT != 0);

        let last = packets.last().unwrap();
        let fu_header_last = last[13];
        assert!(fu_header_last & FU_END_BIT != 0);
        assert_eq!(last[1], 0x80 | PT_H264);
    }

    #[test]
    fn test_packetize_sps_pps_no_prefix() {
        let mut p = VideoPacketizer::new(0xAABBCCDD);
        let sps = vec![0x67, 0x42, 0xC0, 0x0C];
        let pps = vec![0x68, 0xCE, 0x38, 0x80];
        let idr = vec![0x65, 0x88, 0x80, 0x40];
        let packets = p.packetize_frame(&[sps.clone(), pps.clone(), idr.clone()]);

        assert_eq!(packets.len(), 5);

        assert_eq!(packets[0][12] & 0x1F, NAL_TYPE_PACSI);
        assert_eq!(packets[1][12] & 0x1F, NAL_TYPE_SPS);
        assert_eq!(packets[2][12] & 0x1F, NAL_TYPE_PPS);
        assert_eq!(packets[3][12] & 0x1F, NAL_TYPE_PREFIX);
        assert_eq!(packets[4][12] & 0x1F, NAL_TYPE_IDR);
        assert_eq!(packets[4][1], 0x80 | PT_H264);
    }

    #[test]
    fn test_depacketize_single_nal() {
        let mut d = VideoDepacketizer::new();
        let nal = vec![0x67, 0x42, 0xC0];
        let result = d.depacketize(&nal, true).unwrap();
        assert_eq!(result.unwrap(), nal);
        assert_eq!(d.nals_received, 1);
        assert_eq!(d.frames_received, 1);
    }

    #[test]
    fn test_packetize_depacketize_roundtrip() {
        let mut p = VideoPacketizer::new(0x11223344);
        let mut d = VideoDepacketizer::new();

        let mut original_nal = vec![0x65];
        original_nal.extend(vec![0x42; MTU + 100]);

        let packets = p.packetize_frame(&[original_nal.clone()]);
        assert!(packets.len() > 3);

        let mut reassembled = None;
        for pkt in &packets[2..] {
            let marker = pkt[1] & 0x80 != 0;
            let payload = &pkt[12..]; // skip RTP header
            if let Some(nal) = d.depacketize(payload, marker).unwrap() {
                reassembled = Some(nal);
            }
        }

        assert_eq!(reassembled.unwrap(), original_nal);
    }

    #[test]
    fn test_generate_black_iframe() {
        let nals = generate_black_iframe();
        assert_eq!(nals.len(), 3);
        assert_eq!(nals[0][0] & 0x1F, NAL_TYPE_SPS);
        assert_eq!(nals[1][0] & 0x1F, NAL_TYPE_PPS);
        assert_eq!(nals[2][0] & 0x1F, NAL_TYPE_IDR);
    }

    #[test]
    fn test_sequence_numbers_increment() {
        let mut p = VideoPacketizer::new(0x00000001);
        let nal = vec![0x67, 0x42];
        let pkts1 = p.packetize_frame(std::slice::from_ref(&nal));
        let pkts2 = p.packetize_frame(std::slice::from_ref(&nal));

        let seq_first = u16::from_be_bytes([pkts1[0][2], pkts1[0][3]]);
        let seq_last_frame1 =
            u16::from_be_bytes([pkts1.last().unwrap()[2], pkts1.last().unwrap()[3]]);
        let seq_first_frame2 = u16::from_be_bytes([pkts2[0][2], pkts2[0][3]]);
        assert_eq!(seq_first_frame2, seq_last_frame1.wrapping_add(1));
        assert_ne!(seq_first, 0);
    }

    #[test]
    fn test_timestamp_increments_per_frame() {
        let mut p = VideoPacketizer::new(0x00000001);
        let nal = vec![0x67, 0x42];
        let pkts1 = p.packetize_frame(std::slice::from_ref(&nal));
        let pkts2 = p.packetize_frame(std::slice::from_ref(&nal));

        let ts1 = u32::from_be_bytes([pkts1[0][4], pkts1[0][5], pkts1[0][6], pkts1[0][7]]);
        let ts2 = u32::from_be_bytes([pkts2[0][4], pkts2[0][5], pkts2[0][6], pkts2[0][7]]);
        assert_eq!(ts2.wrapping_sub(ts1), FRAME_INTERVAL_TICKS);
    }

    #[test]
    fn test_pacsi_nal_structure() {
        let cfg = SvcConfig::default();
        let pacsi = build_pacsi_nal(&cfg, 42, 3, 0x1234, true, 0x65, true);

        assert_eq!(pacsi[0], 0x7E);

        assert_eq!(pacsi[1], 0xC0);

        assert_eq!(pacsi[2], 0x80);

        assert_eq!(pacsi[3], 0x07);

        assert_eq!(pacsi[4], 0x20);
        assert_eq!(u16::from_be_bytes([pacsi[5], pacsi[6]]), 0x1234);
        assert_eq!(u16::from_be_bytes([pacsi[7], pacsi[8]]), 45);
        assert_eq!(pacsi[9], 0x06);
        assert_eq!(pacsi[10], 5);
        assert_eq!(pacsi[11], 42);
        assert_eq!(&pacsi[12..28], &STREAM_LAYOUT_UUID);

        let bitstream_size_offset = 54;
        assert_eq!(
            u16::from_be_bytes([
                pacsi[bitstream_size_offset],
                pacsi[bitstream_size_offset + 1]
            ]),
            21
        );
        let bitstream_nal_offset = bitstream_size_offset + 2;
        assert_eq!(pacsi[bitstream_nal_offset], 0x06);
        assert_eq!(pacsi[bitstream_nal_offset + 1], 5);
        assert_eq!(pacsi[bitstream_nal_offset + 2], 18);
        assert_eq!(
            &pacsi[bitstream_nal_offset + 3..bitstream_nal_offset + 19],
            &BITSTREAM_INFO_UUID
        );
        assert_eq!(pacsi[bitstream_nal_offset + 19], 42);
        assert_eq!(pacsi[bitstream_nal_offset + 20], 3);
    }

    #[test]
    fn test_pacsi_non_idr() {
        let cfg = SvcConfig::default();
        let pacsi = build_pacsi_nal(&cfg, 0, 1, 0x4321, false, 0x41, false);

        assert_eq!(pacsi[0], 0x5E);
        assert_eq!(pacsi[1], 0x80);
        assert_eq!(pacsi[4], 0x20);
        assert_eq!(u16::from_be_bytes([pacsi[5], pacsi[6]]), 0x4321);
    }

    #[test]
    fn test_pacsi_donc_advances_by_access_unit_nals() {
        let mut p = VideoPacketizer::new(0xAABBCCDD);
        p.cs_don = 0x1234;
        let first = p.packetize_frame(&[vec![0x67, 0x42], vec![0x68, 0xCE], vec![0x65, 0x88]]);
        let first_pacsi = &first[0][super::super::rtp::RTP_HEADER_SIZE..];
        assert_eq!(first_pacsi[4], 0x20);
        assert_eq!(u16::from_be_bytes([first_pacsi[5], first_pacsi[6]]), 0x1234);

        let second = p.packetize_frame(&[vec![0x41, 0x9a]]);
        let second_pacsi = &second[0][super::super::rtp::RTP_HEADER_SIZE..];
        assert_eq!(
            u16::from_be_bytes([second_pacsi[5], second_pacsi[6]]),
            0x1238
        );
    }

    #[test]
    fn test_stream_layout_is_initial_and_idr_metadata_not_every_p_frame() {
        let mut packetizer = VideoPacketizer::new(0x1234_5678);
        let p_slice = vec![0x41, 0x9a, 0x22];
        let idr = vec![0x65, 0x88, 0x84];

        let first = packetizer.packetize_frame(std::slice::from_ref(&p_slice));
        let first_pacsi = &first[0][super::super::rtp::RTP_HEADER_SIZE..];
        assert!(first_pacsi
            .windows(STREAM_LAYOUT_UUID.len())
            .any(|window| window == STREAM_LAYOUT_UUID));

        let second = packetizer.packetize_frame(std::slice::from_ref(&p_slice));
        let second_pacsi = &second[0][super::super::rtp::RTP_HEADER_SIZE..];
        assert!(!second_pacsi
            .windows(STREAM_LAYOUT_UUID.len())
            .any(|window| window == STREAM_LAYOUT_UUID));
        assert!(second_pacsi
            .windows(BITSTREAM_INFO_UUID.len())
            .any(|window| window == BITSTREAM_INFO_UUID));

        let idr_packets = packetizer.packetize_frame(std::slice::from_ref(&idr));
        let idr_pacsi = &idr_packets[0][super::super::rtp::RTP_HEADER_SIZE..];
        assert!(idr_pacsi
            .windows(STREAM_LAYOUT_UUID.len())
            .any(|window| window == STREAM_LAYOUT_UUID));
    }

    #[test]
    fn test_prefix_nal_structure() {
        // IDR reference slice with NRI=3.
        let prefix = build_prefix_nal(0x65, true);
        assert_eq!(prefix.len(), 5);
        assert_eq!(prefix[0], 0x6E);
        assert_eq!(prefix[1], 0xC0);
        assert_eq!(prefix[2], 0x80);
        assert_eq!(prefix[3], 0x07);
        assert_eq!(prefix[4], 0x20);
    }

    #[test]
    fn test_prefix_nal_non_idr() {
        // Non-IDR reference slice (type 1) with NRI=2.
        let prefix = build_prefix_nal(0x41, false);
        assert_eq!(prefix.len(), 5);
        assert_eq!(prefix[0], 0x4E);
        assert_eq!(prefix[1], 0x80);
        assert_eq!(prefix[4], 0x20);

        // A non-reference slice has no store_ref_base_pic_flag syntax.
        let prefix = build_prefix_nal(0x01, false);
        assert_eq!(prefix.len(), 4);
        assert_eq!(prefix[0], 0x0E);
    }

    #[test]
    fn test_bitstream_info_counts_generated_prefix_nals() {
        let mut packetizer = VideoPacketizer::new(0x1234_5678);
        let nals = vec![vec![0x67, 1], vec![0x68, 2], vec![0x65, 3]];
        let packets = packetizer.packetize_frame(&nals);
        let pacsi = &packets[0][super::super::rtp::RTP_HEADER_SIZE..];

        assert_eq!(*pacsi.last().unwrap(), 4);
    }

    #[test]
    fn test_ref_frm_cnt_increments_only_for_reference_vcl() {
        let mut p = VideoPacketizer::new(0xAABBCCDD);
        let idr = vec![0x65, 0x88, 0x80, 0x40];
        let sps = vec![0x67, 0x42, 0xC0, 0x0C];

        let ref_count = |pacsi: &[u8]| {
            let uuid_offset = pacsi
                .windows(BITSTREAM_INFO_UUID.len())
                .position(|window| window == BITSTREAM_INFO_UUID)
                .expect("PACSI missing Bitstream Info UUID");
            pacsi[uuid_offset + BITSTREAM_INFO_UUID.len()]
        };

        let pkts1 = p.packetize_frame(std::slice::from_ref(&idr));
        let cnt1 = ref_count(&pkts1[0][12..]);

        let pkts2 = p.packetize_frame(std::slice::from_ref(&sps));
        let cnt2 = ref_count(&pkts2[0][12..]);
        assert_eq!(cnt2, cnt1.wrapping_add(1));

        let pkts3 = p.packetize_frame(std::slice::from_ref(&idr));
        let cnt3 = ref_count(&pkts3[0][12..]);
        assert_eq!(cnt3, cnt2);
    }

    #[test]
    fn skipped_frames_preserve_sequence_and_stream_layout_but_advance_media_time() {
        let mut packetizer = VideoPacketizer::new(1);
        packetizer.timestamp = u32::MAX - FRAME_INTERVAL_TICKS;
        let initial_seq = packetizer.seq;
        let initial_ref_count = packetizer.ref_frm_cnt;

        assert!(packetizer.packetize_frame(&[]).is_empty());
        assert!(packetizer.packetize_frame(&[Vec::new()]).is_empty());
        assert_eq!(packetizer.seq, initial_seq);
        assert_eq!(packetizer.ref_frm_cnt, initial_ref_count);
        assert!(!packetizer.stream_layout_sent);

        let packets = packetizer.packetize_frame(&[vec![0x41, 0x9a, 0x22]]);
        let first = super::super::rtp::decode(&packets[0]).unwrap();
        assert_eq!(first.sequence_number, initial_seq);
        assert_eq!(first.timestamp, FRAME_INTERVAL_TICKS - 1);
        assert!(first
            .payload
            .windows(STREAM_LAYOUT_UUID.len())
            .any(|window| window == STREAM_LAYOUT_UUID));
    }

    #[test]
    fn test_marker_is_on_last_nonempty_nal() {
        let mut p = VideoPacketizer::new(0xAABBCCDD);
        let packets = p.packetize_frame(&[vec![0x67, 0x42], Vec::new()]);
        assert_eq!(packets.last().unwrap()[1] & 0x80, 0x80);
    }

    #[cfg(feature = "video-codec")]
    #[test]
    fn test_real_encoder_access_unit_obeys_h264uc_rtp_invariants() {
        let width = 64u32;
        let height = 64u32;
        let mut encoder =
            crate::calling::codec::H264Encoder::new(width, height, 30.0, 256).unwrap();
        let yuv = vec![128u8; (width * height * 3 / 2) as usize];
        let nals = encoder.encode(&yuv).unwrap();
        assert!(!nals.is_empty());

        let mut packetizer = VideoPacketizer::new(0x1234_5678);
        packetizer.set_frame_dimensions(width as u16, height as u16);
        let packets = packetizer.packetize_frame(&nals);
        assert!(!packets.is_empty());

        let first_nal_header = nals.iter().find_map(|nal| nal.first()).copied().unwrap();
        let pacsi_header = packets[0][super::super::rtp::RTP_HEADER_SIZE];
        assert_eq!(pacsi_header & 0x1f, NAL_TYPE_PACSI);
        assert_eq!(pacsi_header & 0xe0, first_nal_header & 0xe0);

        let timestamp = &packets[0][4..8];
        assert!(packets.iter().all(|packet| &packet[4..8] == timestamp));
        assert!(packets[..packets.len() - 1]
            .iter()
            .all(|packet| packet[1] & 0x80 == 0));
        assert_eq!(packets.last().unwrap()[1] & 0x80, 0x80);

        for pair in packets.windows(2) {
            let current = u16::from_be_bytes([pair[0][2], pair[0][3]]);
            let next = u16::from_be_bytes([pair[1][2], pair[1][3]]);
            assert_eq!(next, current.wrapping_add(1));
        }

        for (index, packet) in packets.iter().enumerate() {
            if packet[super::super::rtp::RTP_HEADER_SIZE] & 0x1f == NAL_TYPE_PREFIX {
                assert!(index + 1 < packets.len());
                if packet[super::super::rtp::RTP_HEADER_SIZE] & 0x60 != 0 {
                    assert_eq!(packet.len(), super::super::rtp::RTP_HEADER_SIZE + 5);
                    assert_eq!(*packet.last().unwrap(), 0x20);
                }
            }
        }
    }

    #[test]
    fn test_stream_layout_layer_description() {
        let cfg = SvcConfig {
            coded_width: 320,
            coded_height: 240,
            display_width: 320,
            display_height: 240,
            bitrate: 256000,
            fps_idx: 2,
            constrained_baseline: true,
        };
        let sei = build_stream_layout_sei(&cfg);

        assert_eq!(sei[0], 5);
        assert_eq!(sei[1], 42);

        assert_eq!(&sei[2..18], &STREAM_LAYOUT_UUID);

        assert_eq!(sei[18], 0x01);

        assert_eq!(sei[26], 0x01);

        assert_eq!(sei[27], 16);

        assert_eq!(sei[28], 0x01);
        assert_eq!(sei[29], 0x40);
        assert_eq!(sei[30], 0x00);
        assert_eq!(sei[31], 0xF0);

        let bitrate = u32::from_be_bytes([sei[36], sei[37], sei[38], sei[39]]);
        assert_eq!(bitrate, 256000);

        assert_eq!(sei[40], 0x10);

        assert_eq!(sei[41], 0x02);
    }

    #[test]
    fn packetizer_tracks_encoded_and_display_dimensions() {
        let mut packetizer = VideoPacketizer::new(1);
        packetizer.set_frame_dimensions(1920, 1080);

        assert_eq!(packetizer.svc_config.coded_width, 1920);
        assert_eq!(packetizer.svc_config.coded_height, 1088);
        assert_eq!(packetizer.svc_config.display_width, 1920);
        assert_eq!(packetizer.svc_config.display_height, 1080);
    }
}
