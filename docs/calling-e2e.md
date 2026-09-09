# Calling end-to-end tests

The repository already contains a live Teams call harness in
`src/calling/call_test.rs`. It negotiates Trouter, creates the Teams calling
conversation, exchanges SDP/ICE/SRTP media, invites Microsoft's Echo / Call
Quality Tester bot, sends media, and reports packet and echo statistics.

These are authenticated live tests. Run `just login` first and verify the
session with `just status`.

## Deterministic audio transport test

Use a generated tone so the result does not depend on a microphone:

```bash
cargo run --features audio -- call-test --echo --tone --duration 20
```

The run should report `call_accepted=true`, non-zero `audio_packets_sent` and
`audio_packets_received`, and ideally `echo_detected=true`. A missing echo can
still be diagnosed using `echo_correlation` and the packet counts.

## Real microphone and speaker test

```bash
just mic-test
just call-echo
```

This validates local capture/playback before exercising the same Echo call
with the actual audio devices.

## Video test

```bash
just cam-test
just call-echo-video
```

The video build requires V4L2, SDL2, and OpenH264. `cam-test` isolates local
camera/display setup; `call-echo-video` then enables both camera transmission
and received-video display. Inspect `video_packets_sent` and
`video_packets_received` independently of the audio counters.

## Work-account camera frame skips

OpenH264 can return an empty access unit when rate control skips an input frame.
The native work-account sender must leave a gap in RTP media time without sending
another stream's SPS/PPS or a PACSI-only access unit. Camera startup and encoder
errors must also preserve the camera stream's decoder state.

The regression runs the production media sender over local UDP/SRTP, decrypts
and depacketizes the received H.264UC, then decodes the camera video. It uses
640×480 synthetic noise at the production 256 kbit/s limit to force real encoder
skips without mocking the codec:

```bash
cargo test --lib --features audio,video-capture camera_video_survives_encoder_frame_skips -- --nocapture
```

Before the fix, a six-second run encoded four camera frames but decoded only one,
with 356 fallback SPS/PPS NAL units injected. After the fix, all four encoded
camera frames decoded, with no fallback parameter sets and no dimension changes.
The test also checks that RTP timestamps retain gaps for skipped frames.

[OpenH264's skip-frame setting](https://docs.rs/openh264/latest/openh264/encoder/struct.EncoderConfig.html#method.skip_frames)
allows rate control to omit frames.
[MS-H264PF](https://learn.microsoft.com/en-us/openspecs/office_protocols/ms-h264pf/f3024858-350e-49b0-9866-99c4fd2b912d)
defines the H.264UC stream layout and depacketization rules.

## Evidence required for visible video

`work_video_call_e2e_test.dart` checks transport feedback only. An authenticated
RTCP Receiver Report mentioning the outbound SSRC demonstrates reception on the
remote media leg; it does not establish that Teams decoded or displayed video.
A connected call, working local preview, outgoing RTP counts, and inbound video
counts are also insufficient to prove outgoing video rendering.

For work-account rendering acceptance, verify the patched sender's camera image
in the receiving official Teams client and confirm movement over time. When the
receiving client is instrumented, record advancing decoded/rendered frame counts
and changing image pixels. The earlier personal Teams test used a separate
WebRTC path with a synthetic 320×240 source at 700 kbit/s, so it did not cover
native work-account H.264UC camera-frame skips.

The local regression proves the corrected encoder-to-decoder path. A new live
work-account receiver rendering check remains required.

## Flutter application coverage

The Flutter bridge is built with audio and H.264 codec support, with Linux also
enabling the V4L2/SDL2 video-capture feature. `CallGateway` carries an explicit
video mode and the application exposes one call action with an audio/video
picker; Android presents that picker as a bottom sheet.

Android outgoing video uses the native camera bridge and feeds I420 frames into
the Rust H.264/SRTP path. Incoming video negotiates a dedicated video ICE/SRTP
session and renders decoded frames through the Android remote-video view.
Incoming video is receive-only today: accepting a caller with video enabled does
not turn on the local camera.

The Microsoft Test Call harness remains the deterministic live media check. A
real-person incoming Teams video call is still required to prove the complete
production receive path, and TURN relay support remains incomplete for networks
where host/server-reflexive ICE candidates cannot connect directly.
