pub const H264UC_PAYLOAD_TYPE: u8 = 122;
pub const VIDEO_SOURCE_ANY: u32 = 0xFFFF_FFFE;

const PT_PSFB: u8 = 206;
const FMT_AFB: u8 = 15;
const VSR_AFB_TYPE: u16 = 1;
const VSR_ENTRY_LENGTH: u8 = 0x44;
const MAX_WIDTH: u16 = 640;
const MAX_HEIGHT: u16 = 480;
const MIN_BITRATE: u32 = 180_000;
const BITRATE_PER_LEVEL: u32 = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoSourceRequestFeedback {
    pub sender_ssrc: u32,
    pub requested_msi: u32,
    pub request_id: u16,
    pub keyframe_requested: bool,
}

pub fn parse_video_source_request(packet: &[u8]) -> Option<VideoSourceRequestFeedback> {
    if packet.len() < 32
        || packet[1] != PT_PSFB
        || packet[0] & 0x1f != FMT_AFB
        || u16::from_be_bytes(packet[12..14].try_into().ok()?) != VSR_AFB_TYPE
    {
        return None;
    }
    Some(VideoSourceRequestFeedback {
        sender_ssrc: u32::from_be_bytes(packet[4..8].try_into().ok()?),
        requested_msi: u32::from_be_bytes(packet[16..20].try_into().ok()?),
        request_id: u16::from_be_bytes(packet[20..22].try_into().ok()?),
        keyframe_requested: packet[25] & 0x80 != 0,
    })
}

pub fn video_source_request(sender_ssrc: u32, request_id: u16) -> Vec<u8> {
    let mut packet = Vec::with_capacity(100);
    packet.push(0x80 | FMT_AFB);
    packet.push(PT_PSFB);
    packet.extend_from_slice(&[0, 0]);
    packet.extend_from_slice(&sender_ssrc.to_be_bytes());
    packet.extend_from_slice(&0u32.to_be_bytes());
    packet.extend_from_slice(&VSR_AFB_TYPE.to_be_bytes());
    packet.extend_from_slice(&88u16.to_be_bytes());
    packet.extend_from_slice(&VIDEO_SOURCE_ANY.to_be_bytes());
    packet.extend_from_slice(&request_id.to_be_bytes());
    packet.extend_from_slice(&0u16.to_be_bytes());
    packet.push(0);
    packet.push(0x80);
    packet.push(1);
    packet.push(VSR_ENTRY_LENGTH);
    packet.extend_from_slice(&0u32.to_be_bytes());
    packet.push(H264UC_PAYLOAD_TYPE);
    packet.push(1);
    packet.push(0x02);
    packet.push(0x03);
    packet.extend_from_slice(&MAX_WIDTH.to_be_bytes());
    packet.extend_from_slice(&MAX_HEIGHT.to_be_bytes());
    packet.extend_from_slice(&MIN_BITRATE.to_be_bytes());
    packet.extend_from_slice(&0u32.to_be_bytes());
    packet.extend_from_slice(&BITRATE_PER_LEVEL.to_be_bytes());
    for bucket in 0..10 {
        packet.extend_from_slice(&(u16::from(bucket == 4)).to_be_bytes());
    }
    packet.extend_from_slice(&(1u32 << 4).to_be_bytes());
    packet.extend_from_slice(&1u16.to_be_bytes());
    packet.extend_from_slice(&0u16.to_be_bytes());
    packet.extend_from_slice(&[0u8; 16]);
    packet.extend_from_slice(&(u32::from(MAX_WIDTH) * u32::from(MAX_HEIGHT)).to_be_bytes());
    let length_words = (packet.len() / 4 - 1) as u16;
    packet[2..4].copy_from_slice(&length_words.to_be_bytes());
    packet
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_main_video_source_request() {
        let packet = video_source_request(0x1234_5678, 0x4321);

        assert_eq!(packet.len(), 100);
        assert_eq!(packet[0], 0x8f);
        assert_eq!(packet[1], 206);
        assert_eq!(u16::from_be_bytes([packet[2], packet[3]]), 24);
        assert_eq!(&packet[4..8], &0x1234_5678u32.to_be_bytes());
        assert_eq!(&packet[8..12], &0u32.to_be_bytes());
        assert_eq!(&packet[12..14], &1u16.to_be_bytes());
        assert_eq!(&packet[14..16], &88u16.to_be_bytes());
        assert_eq!(&packet[16..20], &VIDEO_SOURCE_ANY.to_be_bytes());
        assert_eq!(&packet[20..22], &0x4321u16.to_be_bytes());
        assert_eq!(packet[24], 0);
        assert_eq!(packet[25], 0x80);
        assert_eq!(packet[26], 1);
        assert_eq!(packet[27], 0x44);
        assert_eq!(packet[32], H264UC_PAYLOAD_TYPE);
        assert_eq!(packet[33], 1);
        assert_eq!(u16::from_be_bytes([packet[36], packet[37]]), 640);
        assert_eq!(u16::from_be_bytes([packet[38], packet[39]]), 480);
        assert_eq!(
            u32::from_be_bytes(packet[72..76].try_into().unwrap()),
            1 << 4
        );
        assert_eq!(
            u32::from_be_bytes(packet[96..100].try_into().unwrap()),
            640 * 480
        );
        assert_eq!(
            parse_video_source_request(&packet),
            Some(VideoSourceRequestFeedback {
                sender_ssrc: 0x1234_5678,
                requested_msi: VIDEO_SOURCE_ANY,
                request_id: 0x4321,
                keyframe_requested: true,
            })
        );
    }
}
