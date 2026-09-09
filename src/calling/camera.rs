//! V4L2 camera capture — reads YUV420 (or YUYV) frames from /dev/video0.
//!
//! Uses the `v4l` crate with mmap streaming. Converts YUYV to I420 if needed.
//! Runs a capture thread that sends raw YUV frames over a channel.

use super::video::YuvFrame;
#[cfg(target_os = "linux")]
use anyhow::bail;
use anyhow::{Context, Result};
#[cfg(target_os = "windows")]
use nokhwa::{
    pixel_format::{FormatDecoder, RgbFormat},
    utils::{ApiBackend, RequestedFormat, RequestedFormatType},
    Camera,
};
use std::sync::{mpsc, Mutex, OnceLock};
#[cfg(target_os = "linux")]
use v4l::buffer::Type;
#[cfg(target_os = "linux")]
use v4l::io::mmap::Stream;
#[cfg(target_os = "linux")]
use v4l::io::traits::CaptureStream;
#[cfg(target_os = "linux")]
use v4l::video::Capture;
#[cfg(target_os = "linux")]
use v4l::{Device, FourCC};

static CAMERA_DEVICE: OnceLock<Mutex<Option<String>>> = OnceLock::new();
#[cfg(target_os = "windows")]
const SYNTHETIC_CAMERA_NAME: &str = "Microslop Synthetic Camera";

#[cfg(target_os = "windows")]
fn synthetic_camera_enabled() -> bool {
    std::env::var("MICROSLOP_SYNTHETIC_CAMERA").as_deref() == Ok("1")
}

pub fn set_camera_device(path: Option<String>) {
    *CAMERA_DEVICE.get_or_init(Default::default).lock().unwrap() = path;
}

#[cfg(target_os = "windows")]
pub fn device_names() -> Vec<String> {
    let mut devices: Vec<String> = nokhwa::query(ApiBackend::MediaFoundation)
        .map(|devices| {
            devices
                .into_iter()
                .map(|device| device.human_name())
                .collect()
        })
        .unwrap_or_default();
    if synthetic_camera_enabled() {
        devices.insert(0, SYNTHETIC_CAMERA_NAME.to_string());
    }
    devices
}

#[cfg(target_os = "linux")]
pub fn device_names() -> Vec<String> {
    let mut devices = std::fs::read_dir("/dev")
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.strip_prefix("video")
                .and_then(|suffix| suffix.parse::<u32>().ok())
                .map(|_| entry.path().to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>();
    devices.sort();
    devices
}

pub struct CameraCapture {
    _handle: std::thread::JoinHandle<()>,
}

#[cfg(target_os = "linux")]
impl CameraCapture {
    pub fn start(
        device_path: Option<&str>,
        width: u32,
        height: u32,
        fps: u32,
    ) -> Result<(Self, mpsc::Receiver<YuvFrame>)> {
        let selected = CAMERA_DEVICE
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .clone();
        let path = device_path.or(selected.as_deref()).unwrap_or("/dev/video0");
        let dev = Device::with_path(path)
            .with_context(|| format!("Failed to open camera at {}", path))?;

        let mut fmt = dev.format().context("Failed to get camera format")?;
        fmt.width = width;
        fmt.height = height;
        let mut actual_fmt = None;
        for fourcc in [FourCC::new(b"YUYV"), FourCC::new(b"YU12")] {
            fmt.fourcc = fourcc;
            match dev.set_format(&fmt) {
                Ok(candidate) if candidate.fourcc == fourcc => {
                    actual_fmt = Some(candidate);
                    break;
                }
                _ => {}
            }
        }
        let actual_fmt = actual_fmt.context("Camera does not support YUYV or YU12 capture")?;

        let actual_fourcc = actual_fmt.fourcc;
        let actual_w = actual_fmt.width;
        let actual_h = actual_fmt.height;
        tracing::info!(
            "Camera opened: {}x{} fourcc={} (requested {}x{} @ {}fps)",
            actual_w,
            actual_h,
            actual_fourcc,
            width,
            height,
            fps,
        );

        if let Ok(mut params) = dev.params() {
            params.interval = v4l::Fraction::new(1, fps);
            let _ = dev.set_params(&params);
        }

        let (tx, rx) = mpsc::sync_channel::<YuvFrame>(2);
        let handle = std::thread::spawn(move || {
            if let Err(e) = capture_loop(dev, actual_w, actual_h, actual_fourcc, tx) {
                tracing::error!("Camera capture loop exited: {:#}", e);
            }
        });

        Ok((CameraCapture { _handle: handle }, rx))
    }
}

#[cfg(target_os = "windows")]
impl CameraCapture {
    pub fn start(
        device_name: Option<&str>,
        width: u32,
        height: u32,
        fps: u32,
    ) -> Result<(Self, mpsc::Receiver<YuvFrame>)> {
        let selected = device_name.map(str::to_owned).or_else(|| {
            CAMERA_DEVICE
                .get_or_init(Default::default)
                .lock()
                .unwrap()
                .clone()
        });
        if synthetic_camera_enabled()
            && selected
                .as_deref()
                .is_none_or(|name| name == SYNTHETIC_CAMERA_NAME)
        {
            return Ok(start_synthetic_camera(width, height, fps));
        }
        let (tx, rx) = mpsc::sync_channel::<YuvFrame>(2);
        let (ready_tx, ready_rx) = mpsc::sync_channel::<std::result::Result<(), String>>(1);
        let handle = std::thread::spawn(move || {
            if let Err(error) =
                windows_capture_loop(selected.as_deref(), width, height, fps, tx, &ready_tx)
            {
                let _ = ready_tx.try_send(Err(format!("{error:#}")));
                tracing::error!("Camera capture loop exited: {error:#}");
            }
        });
        match ready_rx
            .recv()
            .context("Windows camera thread exited during startup")?
        {
            Ok(()) => Ok((CameraCapture { _handle: handle }, rx)),
            Err(error) => {
                let _ = handle.join();
                anyhow::bail!(error)
            }
        }
    }
}

/// Main capture loop — runs on a dedicated thread.
#[cfg(target_os = "linux")]
fn capture_loop(
    dev: Device,
    width: u32,
    height: u32,
    fourcc: FourCC,
    tx: mpsc::SyncSender<YuvFrame>,
) -> Result<()> {
    let mut stream = Stream::with_buffers(&dev, Type::VideoCapture, 4)
        .context("Failed to start V4L2 mmap stream")?;

    loop {
        let (buf, _meta) = stream.next().context("Failed to read camera frame")?;

        let yuv_data = if fourcc == FourCC::new(b"YUYV") {
            yuyv_to_i420(buf, width, height)
        } else if fourcc == FourCC::new(b"YU12") {
            let expected = width as usize * height as usize * 3 / 2;
            if buf.len() < expected {
                bail!("Camera returned a truncated YU12 frame");
            }
            buf[..expected].to_vec()
        } else {
            bail!("Unsupported camera format {fourcc}");
        };

        let frame = YuvFrame {
            width,
            height,
            data: yuv_data,
        };

        match tx.try_send(frame) {
            Ok(()) => {}
            Err(std::sync::mpsc::TrySendError::Full(_)) => {}
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                break;
            }
        }
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn windows_capture_loop(
    device_name: Option<&str>,
    width: u32,
    height: u32,
    fps: u32,
    tx: mpsc::SyncSender<YuvFrame>,
    ready: &mpsc::SyncSender<std::result::Result<(), String>>,
) -> Result<()> {
    let devices = nokhwa::query(ApiBackend::MediaFoundation)
        .context("Failed to enumerate Windows cameras")?;
    let info = device_name
        .and_then(|name| {
            devices
                .iter()
                .find(|device| device.human_name() == name || device.misc() == name)
        })
        .or_else(|| devices.first())
        .cloned()
        .context("No Windows camera is available")?;
    let requested =
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let mut camera =
        Camera::with_backend(info.index().clone(), requested, ApiBackend::MediaFoundation)
            .with_context(|| format!("Failed to open Windows camera {}", info.human_name()))?;
    if let Some(format) = camera
        .compatible_camera_formats()?
        .into_iter()
        .filter(|format| RgbFormat::FORMATS.contains(&format.format()))
        .min_by_key(|format| {
            (
                format.frame_rate() < fps,
                format.width().abs_diff(width) + format.height().abs_diff(height),
                format.frame_rate().abs_diff(fps),
            )
        })
    {
        camera.set_camera_requset(RequestedFormat::new::<RgbFormat>(
            RequestedFormatType::Exact(format),
        ))?;
    }
    camera
        .open_stream()
        .context("Failed to start Windows camera")?;
    let actual = camera.camera_format();
    tracing::info!(
        "Camera opened: {} {} (requested {}x{} @ {}fps)",
        info.human_name(),
        actual,
        width,
        height,
        fps,
    );
    ready
        .send(Ok(()))
        .context("Windows camera startup receiver closed")?;

    loop {
        let buffer = camera
            .frame()
            .context("Failed to read Windows camera frame")?;
        let resolution = buffer.resolution();
        let image = buffer
            .decode_image::<RgbFormat>()
            .context("Failed to decode Windows camera frame")?;
        let width = resolution.width() & !1;
        let height = resolution.height() & !1;
        let frame = YuvFrame {
            width,
            height,
            data: rgb_to_i420(image.as_raw(), resolution.width(), width, height),
        };
        match tx.try_send(frame) {
            Ok(()) => {}
            Err(mpsc::TrySendError::Full(_)) => {}
            Err(mpsc::TrySendError::Disconnected(_)) => break,
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn start_synthetic_camera(
    width: u32,
    height: u32,
    fps: u32,
) -> (CameraCapture, mpsc::Receiver<YuvFrame>) {
    let width = width.max(2) & !1;
    let height = height.max(2) & !1;
    let (tx, rx) = mpsc::sync_channel::<YuvFrame>(2);
    let handle = std::thread::spawn(move || {
        let interval = std::time::Duration::from_millis(1000 / u64::from(fps.max(1)));
        let mut sequence = 0u32;
        loop {
            match tx.try_send(synthetic_frame(width, height, sequence)) {
                Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
                Err(mpsc::TrySendError::Disconnected(_)) => break,
            }
            sequence = sequence.wrapping_add(1);
            std::thread::sleep(interval);
        }
    });
    (CameraCapture { _handle: handle }, rx)
}

#[cfg(target_os = "windows")]
fn synthetic_frame(width: u32, height: u32, sequence: u32) -> YuvFrame {
    let width_usize = width as usize;
    let height_usize = height as usize;
    let y_size = width_usize * height_usize;
    let chroma_size = y_size / 4;
    let mut data = vec![0u8; y_size + chroma_size * 2];
    let bar = ((sequence * 7) % width) as usize;
    for row in 0..height_usize {
        for col in 0..width_usize {
            let gradient = 16 + ((col * 180) / width_usize) as u8;
            let distance = col.abs_diff(bar);
            data[row * width_usize + col] = if distance < 12 { 235 } else { gradient };
        }
    }
    data[y_size..y_size + chroma_size].fill(96 + (sequence % 48) as u8);
    data[y_size + chroma_size..].fill(160 - (sequence % 48) as u8);
    YuvFrame {
        width,
        height,
        data,
    }
}

#[cfg(target_os = "windows")]
fn rgb_to_i420(rgb: &[u8], stride_width: u32, width: u32, height: u32) -> Vec<u8> {
    let stride = stride_width as usize;
    let width = width as usize;
    let height = height as usize;
    let y_size = width * height;
    let uv_stride = width / 2;
    let mut output = vec![0u8; y_size + y_size / 2];
    let (y_plane, uv) = output.split_at_mut(y_size);
    let (u_plane, v_plane) = uv.split_at_mut(y_size / 4);

    for row in (0..height).step_by(2) {
        for col in (0..width).step_by(2) {
            let mut r_sum = 0i32;
            let mut g_sum = 0i32;
            let mut b_sum = 0i32;
            for y in 0..2 {
                for x in 0..2 {
                    let pixel_row = row + y;
                    let pixel_col = col + x;
                    let offset = (pixel_row * stride + pixel_col) * 3;
                    let r = rgb[offset] as i32;
                    let g = rgb[offset + 1] as i32;
                    let b = rgb[offset + 2] as i32;
                    y_plane[pixel_row * width + pixel_col] =
                        (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(0, 255) as u8;
                    r_sum += r;
                    g_sum += g;
                    b_sum += b;
                }
            }
            let r = r_sum / 4;
            let g = g_sum / 4;
            let b = b_sum / 4;
            let index = (row / 2) * uv_stride + col / 2;
            u_plane[index] = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
            v_plane[index] = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
        }
    }

    output
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::synthetic_frame;

    #[test]
    fn synthetic_camera_frames_change() {
        let first = synthetic_frame(320, 240, 0);
        let second = synthetic_frame(320, 240, 1);
        assert_eq!(first.data.len(), 320 * 240 * 3 / 2);
        assert_ne!(first.data, second.data);
    }
}

#[cfg(target_os = "windows")]
pub fn cam_test() -> anyhow::Result<()> {
    use std::collections::HashSet;
    use std::hash::{DefaultHasher, Hash, Hasher};

    let (_capture, rx) = CameraCapture::start(None, 320, 240, 15)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut frames = 0usize;
    let mut fingerprints = HashSet::new();
    while std::time::Instant::now() < deadline {
        match rx.recv_timeout(std::time::Duration::from_millis(250)) {
            Ok(frame) => {
                let mut hasher = DefaultHasher::new();
                frame.data.hash(&mut hasher);
                fingerprints.insert(hasher.finish());
                frames += 1;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    println!("camera_frames={frames}");
    println!("camera_unique_frames={}", fingerprints.len());
    anyhow::ensure!(
        frames >= 10,
        "Too few Windows camera frames captured: {frames}"
    );
    anyhow::ensure!(
        fingerprints.len() >= 2,
        "Windows camera frames are not changing"
    );
    Ok(())
}

/// Capture ~3s of video from V4L2 camera, then play it back in an SDL2 window.
#[cfg(target_os = "linux")]
pub fn cam_test() -> anyhow::Result<()> {
    use super::display::{DisplayFrame, VideoDisplay};
    use anyhow::bail;

    println!("=== Camera Test ===");
    println!("Capturing 3 seconds of video...\n");

    let (capture, cam_rx) =
        CameraCapture::start(None, 320, 240, 15).context("Failed to start camera")?;

    let mut frames: Vec<YuvFrame> = Vec::with_capacity(45);
    let start = std::time::Instant::now();
    let capture_duration = std::time::Duration::from_secs(3);

    while start.elapsed() < capture_duration {
        match cam_rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(frame) => {
                if frames.is_empty() {
                    println!("  First frame: {}x{}", frame.width, frame.height);
                }
                frames.push(frame);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                bail!("Camera disconnected during capture");
            }
        }
    }
    drop(capture);

    let capture_elapsed = start.elapsed().as_secs_f64();
    let capture_fps = frames.len() as f64 / capture_elapsed;
    println!(
        "\nCaptured {} frames in {:.3}s ({:.1} fps)",
        frames.len(),
        capture_elapsed,
        capture_fps,
    );

    if frames.is_empty() {
        bail!("No frames captured");
    }

    println!("Playing back...\n");
    let (display, display_tx) =
        VideoDisplay::start("Camera Test Playback").context("Failed to start video display")?;

    let playback_start = std::time::Instant::now();
    let frame_interval_us = (capture_elapsed * 1_000_000.0 / frames.len() as f64) as u64;
    let frame_interval = std::time::Duration::from_micros(frame_interval_us);
    let n_frames = frames.len();
    for (i, frame) in frames.into_iter().enumerate() {
        let df = DisplayFrame {
            width: frame.width,
            height: frame.height,
            data: frame.data,
        };
        if display_tx.send(df).is_err() {
            break;
        }
        let target = playback_start + frame_interval * (i + 1) as u32;
        let now = std::time::Instant::now();
        if target > now {
            std::thread::sleep(target - now);
        }
    }
    let playback_elapsed = playback_start.elapsed().as_secs_f64();
    let playback_fps = n_frames as f64 / playback_elapsed;

    std::thread::sleep(std::time::Duration::from_millis(500));
    drop(display_tx);
    display.join();

    println!(
        "Recording : {:.3}s  ({} frames, {:.1} fps)",
        capture_elapsed, n_frames, capture_fps
    );
    println!(
        "Playback  : {:.3}s  ({} frames, {:.1} fps)",
        playback_elapsed, n_frames, playback_fps
    );
    let ratio = playback_elapsed / capture_elapsed;
    if ratio > 1.05 {
        println!(
            "Playback was {:.0}% slower than recording (frame interval too long)",
            (ratio - 1.0) * 100.0
        );
    } else if ratio < 0.95 {
        println!(
            "Playback was {:.0}% faster than recording",
            (1.0 - ratio) * 100.0
        );
    } else {
        println!("Playback matched recording duration");
    }
    Ok(())
}

/// Convert YUYV (YUV 4:2:2 packed) to I420 (YUV 4:2:0 planar).
fn yuyv_to_i420(yuyv: &[u8], width: u32, height: u32) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let y_size = w * h;
    let uv_size = (w / 2) * (h / 2);
    let mut out = vec![0u8; y_size + uv_size * 2];

    let (y_plane, uv_planes) = out.split_at_mut(y_size);
    let (u_plane, v_plane) = uv_planes.split_at_mut(uv_size);

    let row_bytes = w * 2;
    for (row, source) in yuyv.chunks(row_bytes).take(h).enumerate() {
        let target = &mut y_plane[row * w..(row + 1) * w];
        let uv_start = (row / 2) * (w / 2);
        for (pair, pixel) in source.as_chunks::<4>().0.iter().enumerate() {
            let col = pair * 2;
            target[col] = pixel[0];
            target[col + 1] = pixel[2];
            if row % 2 == 0 {
                u_plane[uv_start + pair] = pixel[1];
                v_plane[uv_start + pair] = pixel[3];
            }
        }
    }

    out
}
