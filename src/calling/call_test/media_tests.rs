use super::*;
use std::hash::{Hash, Hasher};

#[tokio::test]
async fn camera_video_survives_encoder_frame_skips() {
    let audio_socket = Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let audio_receiver = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let video_socket = Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let receiver = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let material = srtp::SrtpKeyingMaterial {
        master_key: [1; 16],
        master_salt: [2; 14],
        tag: 1,
    };
    let mut receive_crypto = srtp::create_context(&material, &material).unwrap();
    let (camera_tx, camera_rx) = std::sync::mpsc::channel();
    let handles = spawn_media_leg(MediaLegSpawn {
        leg: MediaLeg {
            label: "camera-skip-regression".to_owned(),
            audio_socket,
            audio_srtp_ctx: Arc::new(Mutex::new(
                srtp::create_context(&material, &material).unwrap(),
            )),
            audio_remote_addr: audio_receiver.local_addr().unwrap(),
            audio_local_pwd: String::new(),
            audio_ssrc: 10,
            video_socket,
            video_srtp_ctx: Some(Arc::new(Mutex::new(
                srtp::create_context(&material, &material).unwrap(),
            ))),
            video_remote_addr: Some(receiver.local_addr().unwrap()),
            video_local_crypto_line: String::new(),
            video_local_ufrag: String::new(),
            video_local_pwd: String::new(),
            video_controlling: true,
            video_ssrc: 20,
            camera_rx: Some(camera_rx),
            #[cfg(feature = "video-capture")]
            display_tx: None,
            decode_remote_video: false,
        },
        cname: "camera-skip-regression",
        recorder: None,
        loopback: false,
        speaker_tx: None,
        mic_rx: None,
        tone_mode: false,
        progress: None,
    });
    let (width, height) = (640u32, 480u32);
    let mut yuv = vec![128u8; (width * height * 3 / 2) as usize];
    let mut state = 1u32;
    let mut source = tokio::time::interval(Duration::from_millis(33));
    let mut depacketizer = video::VideoDepacketizer::new();
    let mut decoder = codec::H264Decoder::new().unwrap();
    let mut buffer = [0u8; 1500];
    let mut supplied = 0;
    let mut decoded = 0;
    let mut decoded_images = std::collections::HashSet::new();
    let mut fallback_parameter_sets = 0;
    let mut wrong_dimensions = 0;
    let mut timestamps = Vec::new();
    let deadline = tokio::time::sleep(Duration::from_secs(6));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => break,
            _ = source.tick() => {
                for value in &mut yuv[..(width * height) as usize] {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    *value = 16 + (state % 220) as u8;
                }
                camera_tx.send(video::YuvFrame { width, height, data: yuv.clone() }).unwrap();
                supplied += 1;
            }
            received = receiver.recv(&mut buffer) => {
                let size = received.unwrap();
                if size < 12 || buffer[1] & 0x7f != video::PT_H264 {
                    continue;
                }
                let plain = srtp::unprotect(&mut receive_crypto, &buffer[..size]).unwrap();
                let packet = rtp::decode(&plain).unwrap();
                if packet.marker {
                    timestamps.push(packet.timestamp);
                }
                if let Some(nal) = depacketizer.depacketize(packet.payload, packet.marker).unwrap() {
                    if video::black_iframe_nals()[..2].contains(&nal) {
                        fallback_parameter_sets += 1;
                    }
                    if let Some(frame) = decoder.decode(&nal).unwrap() {
                        decoded += 1;
                        let mut hash = std::hash::DefaultHasher::new();
                        frame.data.hash(&mut hash);
                        decoded_images.insert(hash.finish());
                        wrong_dimensions += u32::from(frame.width != width || frame.height != height);
                    }
                }
            }
        }
    }
    handles.abort_all();
    let encoded = handles.camera_frames_sent.load(Ordering::Relaxed);
    eprintln!("supplied={supplied} encoded={encoded} decoded={decoded} fallback_parameter_sets={fallback_parameter_sets} wrong_dimensions={wrong_dimensions}");
    assert!(
        encoded > 1 && encoded < supplied / 2,
        "The real encoder must skip frames in this test"
    );
    assert_eq!(
        fallback_parameter_sets, 0,
        "Skipped camera frames must not replace the decoder parameter sets"
    );
    assert_eq!(wrong_dimensions, 0);
    assert!(
        decoded_images.len() > 1,
        "The receiver must decode changing camera pixels"
    );
    assert!(
        decoded >= encoded.saturating_sub(1),
        "Transmitted camera frames must remain decodable"
    );
    assert!(
        timestamps
            .windows(2)
            .any(|pair| pair[1].wrapping_sub(pair[0]) > video::FRAME_INTERVAL_TICKS),
        "Skipped frames must leave a gap in RTP media time"
    );
}
