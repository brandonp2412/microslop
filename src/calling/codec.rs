//! H.264 encode/decode wrappers around the `openh264` crate (v0.9 API).
//!
//! Encoder: takes I420 YUV frames, produces H.264 NAL units.
//! Decoder: takes H.264 NAL units, produces I420 YUV frames.

use anyhow::{Context, Result};
use openh264::decoder::{Decoder, DecoderConfig};
use openh264::encoder::{BitRate, Encoder, EncoderConfig, FrameRate, IntraFramePeriod};
use openh264::formats::YUVSource;
use openh264::OpenH264API;

/// Wrapper to pass raw I420 data directly to openh264 encoder.
struct RawI420<'a> {
    data: &'a [u8],
    width: usize,
    height: usize,
}

impl<'a> YUVSource for RawI420<'a> {
    fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }
    fn strides(&self) -> (usize, usize, usize) {
        (self.width, self.width / 2, self.width / 2)
    }
    fn y(&self) -> &[u8] {
        &self.data[..self.width * self.height]
    }
    fn u(&self) -> &[u8] {
        let y_size = self.width * self.height;
        let uv_size = (self.width / 2) * (self.height / 2);
        &self.data[y_size..y_size + uv_size]
    }
    fn v(&self) -> &[u8] {
        let y_size = self.width * self.height;
        let uv_size = (self.width / 2) * (self.height / 2);
        &self.data[y_size + uv_size..y_size + uv_size * 2]
    }
}

/// Decoded YUV frame ready for display.
pub struct DecodedFrame {
    pub width: u32,
    pub height: u32,
    /// I420 planar data: Y (w*h) + U (w*h/4) + V (w*h/4)
    pub data: Vec<u8>,
}

pub struct H264Encoder {
    encoder: Encoder,
    width: u32,
    height: u32,
}

impl H264Encoder {
    pub fn new(width: u32, height: u32, fps: f32, bitrate_kbps: u32) -> Result<Self> {
        let api = OpenH264API::from_source();
        let config = EncoderConfig::new()
            .max_frame_rate(FrameRate::from_hz(fps))
            .bitrate(BitRate::from_bps(bitrate_kbps * 1000))
            .intra_frame_period(IntraFramePeriod::from_num_frames(
                (fps.max(1.0) * 2.0).round() as u32,
            ));

        let encoder =
            Encoder::with_api_config(api, config).context("Failed to create openh264 encoder")?;

        Ok(Self {
            encoder,
            width,
            height,
        })
    }

    pub fn force_intra_frame(&mut self) {
        self.encoder.force_intra_frame();
    }

    /// Encode a raw I420 YUV frame into H.264 NAL units.
    ///
    /// Returns a Vec of NAL unit byte vectors (without start codes).
    pub fn encode(&mut self, yuv_data: &[u8]) -> Result<Vec<Vec<u8>>> {
        let expected_size = (self.width * self.height * 3 / 2) as usize;
        if yuv_data.len() < expected_size {
            anyhow::bail!(
                "YUV frame too small: {} bytes, expected {}",
                yuv_data.len(),
                expected_size
            );
        }

        let yuv = RawI420 {
            data: yuv_data,
            width: self.width as usize,
            height: self.height as usize,
        };

        let bitstream = self
            .encoder
            .encode(&yuv)
            .context("openh264 encode failed")?;

        let mut nals = Vec::new();
        for layer_idx in 0..bitstream.num_layers() {
            let layer = bitstream.layer(layer_idx);
            if let Some(layer) = layer {
                for nal_idx in 0..layer.nal_count() {
                    if let Some(nal_data) = layer.nal_unit(nal_idx) {
                        // openh264 returns NAL units with start codes -- strip them
                        let nal = strip_start_code(nal_data);
                        if !nal.is_empty() {
                            nals.push(nal.to_vec());
                        }
                    }
                }
            }
        }

        Ok(nals)
    }
}

pub struct H264Decoder {
    decoder: Decoder,
    annexb: Vec<u8>,
}

impl H264Decoder {
    pub fn new() -> Result<Self> {
        let api = OpenH264API::from_source();
        let decoder = Decoder::with_api_config(api, DecoderConfig::new())
            .context("Failed to create openh264 decoder")?;
        Ok(Self {
            decoder,
            annexb: Vec::new(),
        })
    }

    ///
    /// Returns a decoded YUV frame if the decoder produced output, None if
    /// it needs more data (e.g., SPS/PPS before IDR).
    pub fn decode(&mut self, nal: &[u8]) -> Result<Option<DecodedFrame>> {
        self.annexb.clear();
        self.annexb.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        self.annexb.extend_from_slice(nal);

        match self.decoder.decode(&self.annexb) {
            Ok(Some(yuv)) => {
                let (w, h) = yuv.dimensions();
                let (y_stride, u_stride, v_stride) = yuv.strides();

                let y_size = w * h;
                let uv_size = (w / 2) * (h / 2);
                let mut data = vec![0u8; y_size + uv_size * 2];

                for row in 0..h {
                    let src_start = row * y_stride;
                    let dst_start = row * w;
                    data[dst_start..dst_start + w]
                        .copy_from_slice(&yuv.y()[src_start..src_start + w]);
                }

                let half_w = w / 2;
                let half_h = h / 2;
                for row in 0..half_h {
                    let src_start = row * u_stride;
                    let dst_start = y_size + row * half_w;
                    data[dst_start..dst_start + half_w]
                        .copy_from_slice(&yuv.u()[src_start..src_start + half_w]);
                }

                for row in 0..half_h {
                    let src_start = row * v_stride;
                    let dst_start = y_size + uv_size + row * half_w;
                    data[dst_start..dst_start + half_w]
                        .copy_from_slice(&yuv.v()[src_start..src_start + half_w]);
                }

                Ok(Some(DecodedFrame {
                    width: w as u32,
                    height: h as u32,
                    data,
                }))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                tracing::debug!("openh264 decode error: {:?}", e);
                Ok(None)
            }
        }
    }
}

/// Strip Annex B start codes (0x00 0x00 0x01 or 0x00 0x00 0x00 0x01) from NAL data.
fn strip_start_code(data: &[u8]) -> &[u8] {
    if data.starts_with(&[0x00, 0x00, 0x00, 0x01]) {
        &data[4..]
    } else if data.starts_with(&[0x00, 0x00, 0x01]) {
        &data[3..]
    } else {
        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gray_i420(width: usize, height: usize) -> Vec<u8> {
        let mut frame = vec![128; width * height * 3 / 2];
        frame[..width * height].fill(16);
        frame
    }

    #[test]
    fn encoder_emits_periodic_idr_frames() {
        let mut encoder = H264Encoder::new(64, 64, 30.0, 256).unwrap();
        let frame = gray_i420(64, 64);
        let mut idr_frames = 0;

        for _ in 0..65 {
            let nals = encoder.encode(&frame).unwrap();
            if nals
                .iter()
                .any(|nal| nal.first().is_some_and(|byte| byte & 0x1f == 5))
            {
                idr_frames += 1;
            }
        }

        assert!(idr_frames >= 2, "expected startup and periodic IDR frames");
    }

    #[test]
    fn encoder_emits_constrained_baseline_sps() {
        let mut encoder = H264Encoder::new(64, 64, 30.0, 256).unwrap();
        let frame = gray_i420(64, 64);
        let nals = encoder.encode(&frame).unwrap();
        let sps = nals
            .iter()
            .find(|nal| nal.first().is_some_and(|byte| byte & 0x1f == 7))
            .expect("startup frame should contain an SPS");

        assert!(sps.len() >= 4);
        assert_eq!(sps[1], 66, "expected H.264 Baseline profile_idc");
        assert_ne!(
            sps[2] & 0x40,
            0,
            "constraint_set1_flag must be set for constrained baseline"
        );
    }

    #[test]
    fn force_intra_frame_emits_idr() {
        let mut encoder = H264Encoder::new(64, 64, 30.0, 256).unwrap();
        let frame = gray_i420(64, 64);
        let _ = encoder.encode(&frame).unwrap();
        encoder.force_intra_frame();

        let nals = encoder.encode(&frame).unwrap();
        assert!(nals
            .iter()
            .any(|nal| nal.first().is_some_and(|byte| byte & 0x1f == 5)));
    }
}
