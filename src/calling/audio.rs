//! Audio capture and playback using cpal.
//!
//! Opens the default input/output devices at 8000 Hz mono i16 (PCMU native rate).
//! If the device doesn't support 8000 Hz, captures/plays at the device's native
//! rate.
//!
//! Gated behind `#[cfg(feature = "audio")]` — when the feature is off, the
//! public types are not compiled and media.rs falls back to silence mode.

use std::sync::{
    atomic::{AtomicU32, Ordering},
    mpsc, Mutex, OnceLock,
};
use std::thread;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, SampleRate, StreamConfig};

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
pub use android::{
    pause_android_call_audio, resume_android_call_audio, set_android_output_device_id,
};
#[cfg(target_os = "android")]
use android::{start_android_capture, start_android_playback};

/// Number of PCM samples per 20ms frame at 8000 Hz.
const FRAME_SAMPLES: usize = 160;

/// Target sample rate for PCMU.
const TARGET_RATE: u32 = 8000;

#[derive(Clone, Default)]
pub struct AudioCaptureStats {
    produced: std::sync::Arc<AtomicU32>,
    enqueued: std::sync::Arc<AtomicU32>,
    full: std::sync::Arc<AtomicU32>,
    disconnected: std::sync::Arc<AtomicU32>,
    stream_errors: std::sync::Arc<AtomicU32>,
}

#[derive(Clone, Default)]
pub struct AudioPlaybackStats {
    frames_received: std::sync::Arc<AtomicU32>,
    samples_rendered: std::sync::Arc<AtomicU32>,
    stream_errors: std::sync::Arc<AtomicU32>,
}

impl AudioPlaybackStats {
    pub fn frames_received(&self) -> u32 {
        self.frames_received.load(Ordering::Relaxed)
    }

    pub fn samples_rendered(&self) -> u32 {
        self.samples_rendered.load(Ordering::Relaxed)
    }

    pub fn stream_errors(&self) -> u32 {
        self.stream_errors.load(Ordering::Relaxed)
    }
}

impl AudioCaptureStats {
    pub fn produced(&self) -> u32 {
        self.produced.load(Ordering::Relaxed)
    }

    pub fn enqueued(&self) -> u32 {
        self.enqueued.load(Ordering::Relaxed)
    }

    pub fn full(&self) -> u32 {
        self.full.load(Ordering::Relaxed)
    }

    pub fn disconnected(&self) -> u32 {
        self.disconnected.load(Ordering::Relaxed)
    }

    pub fn stream_errors(&self) -> u32 {
        self.stream_errors.load(Ordering::Relaxed)
    }
}

static INPUT_DEVICE: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static OUTPUT_DEVICE: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static RINGBACK_STOP: OnceLock<Mutex<Option<mpsc::Sender<()>>>> = OnceLock::new();
static RINGBACK_FRAMES: OnceLock<Vec<Vec<i16>>> = OnceLock::new();
#[cfg(target_os = "windows")]
static WINDOWS_AUDIO_OWNER: OnceLock<mpsc::Sender<()>> = OnceLock::new();

pub fn set_input_device(name: Option<String>) {
    *INPUT_DEVICE.get_or_init(Default::default).lock().unwrap() = name;
}

pub fn set_output_device(name: Option<String>) {
    *OUTPUT_DEVICE.get_or_init(Default::default).lock().unwrap() = name;
}

fn ringback_frames() -> &'static [Vec<i16>] {
    RINGBACK_FRAMES.get_or_init(|| {
        (0..5)
            .map(|frame_index| {
                (0..FRAME_SAMPLES)
                    .map(|sample_offset| {
                        let sample_index = frame_index * FRAME_SAMPLES + sample_offset;
                        let t = sample_index as f64 / TARGET_RATE as f64;
                        (((std::f64::consts::TAU * 440.0 * t).sin()
                            + (std::f64::consts::TAU * 480.0 * t).sin())
                            * 0.15
                            * i16::MAX as f64) as i16
                    })
                    .collect()
            })
            .collect()
    })
}

pub fn start_ringback() {
    stop_ringback();
    let Some((playback, sender)) = AudioPlayback::start() else {
        return;
    };
    let (stop_tx, stop_rx) = mpsc::channel();
    *RINGBACK_STOP.get_or_init(Default::default).lock().unwrap() = Some(stop_tx);
    thread::spawn(move || {
        let _playback = playback;
        loop {
            for index in 0..100 {
                if stop_rx.try_recv().is_ok() {
                    return;
                }
                if sender.send(ringback_frames()[index % 5].clone()).is_err() {
                    return;
                }
            }
            if stop_rx
                .recv_timeout(std::time::Duration::from_secs(4))
                .is_ok()
            {
                return;
            }
        }
    });
}

pub fn stop_ringback() {
    if let Some(stop) = RINGBACK_STOP
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .take()
    {
        let _ = stop.send(());
    }
}

#[cfg(not(target_os = "android"))]
pub fn device_names(input: bool) -> Vec<String> {
    #[cfg(target_os = "windows")]
    ensure_windows_audio_owner();
    let host = cpal::default_host();
    let devices = if input {
        host.input_devices()
    } else {
        host.output_devices()
    };
    devices
        .map(|devices| devices.filter_map(|device| device.name().ok()).collect())
        .unwrap_or_default()
}

#[cfg(target_os = "android")]
pub fn device_names(_input: bool) -> Vec<String> {
    Vec::new()
}

#[cfg(target_os = "windows")]
fn ensure_windows_audio_owner() {
    WINDOWS_AUDIO_OWNER.get_or_init(|| {
        let (keep_tx, keep_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let host = cpal::default_host();
            let _ = host.default_input_device();
            let _ = host.default_output_device();
            let _ = ready_tx.send(());
            let _ = keep_rx.recv();
        });
        ready_rx
            .recv()
            .expect("Windows audio initialization failed");
        keep_tx
    });
}

fn selected_device(host: &cpal::Host, input: bool) -> Option<Device> {
    let selected = if input { &INPUT_DEVICE } else { &OUTPUT_DEVICE }
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .clone()
        .or_else(|| {
            std::env::var(if input {
                "MICROSLOP_AUDIO_INPUT"
            } else {
                "MICROSLOP_AUDIO_OUTPUT"
            })
            .ok()
        });
    if let Some(name) = selected {
        let devices = if input {
            host.input_devices().ok()?
        } else {
            host.output_devices().ok()?
        };
        devices
            .into_iter()
            .find(|device| device.name().ok().as_deref() == Some(&name))
    } else if input {
        host.default_input_device()
    } else {
        host.default_output_device()
    }
}

struct AudioResampler {
    source_rate: u32,
    target_rate: u32,
    samples: Vec<i16>,
    position: u64,
    downsample_kernel: Option<(Vec<f64>, f64)>,
}

impl AudioResampler {
    fn new(source_rate: u32, target_rate: u32) -> Self {
        let downsample_kernel =
            (source_rate > target_rate && source_rate.is_multiple_of(target_rate)).then(|| {
                let ratio = source_rate as f64 / target_rate as f64;
                let cutoff = 0.45 / ratio;
                let mut coefficients = Vec::with_capacity(65);
                let mut weight = 0.0;
                for offset in -32..=32 {
                    let distance = -(offset as f64);
                    let phase = 2.0 * std::f64::consts::PI * cutoff * distance;
                    let sinc = if phase.abs() < 1e-9 {
                        1.0
                    } else {
                        phase.sin() / phase
                    };
                    let window = 0.5 + 0.5 * (std::f64::consts::PI * distance / 32.0).cos();
                    let coefficient = sinc * window;
                    coefficients.push(coefficient);
                    weight += coefficient;
                }
                (coefficients, weight)
            });
        Self {
            source_rate,
            target_rate,
            samples: Vec::new(),
            position: 0,
            downsample_kernel,
        }
    }

    fn process(&mut self, input: &[i16]) -> Vec<i16> {
        if self.source_rate == self.target_rate {
            return input.to_vec();
        }
        self.samples.extend_from_slice(input);
        let ratio = self.source_rate as f64 / self.target_rate as f64;
        let cutoff = 0.45 / ratio;
        let mut output = Vec::new();
        while self.position < self.samples.len() as u64 * u64::from(self.target_rate) {
            let value = if let Some((coefficients, weight)) = &self.downsample_kernel {
                let center = (self.position / u64::from(self.target_rate)) as isize - 32;
                let mut sum = 0.0;
                for (offset, coefficient) in coefficients.iter().enumerate() {
                    let index = center - 32 + offset as isize;
                    let sample = usize::try_from(index)
                        .ok()
                        .and_then(|index| self.samples.get(index))
                        .copied()
                        .unwrap_or(0);
                    sum += f64::from(sample) * coefficient;
                }
                sum / weight
            } else if ratio > 1.0 {
                let center = self.position as f64 / self.target_rate as f64 - 32.0;
                let mut sum = 0.0;
                let mut weight = 0.0;
                for index in (center - 32.0).ceil() as isize..=(center + 32.0).floor() as isize {
                    let distance = center - index as f64;
                    let phase = 2.0 * std::f64::consts::PI * cutoff * distance;
                    let sinc = if phase.abs() < 1e-9 {
                        1.0
                    } else {
                        phase.sin() / phase
                    };
                    let window = 0.5 + 0.5 * (std::f64::consts::PI * distance / 32.0).cos();
                    let coefficient = sinc * window;
                    let sample = usize::try_from(index)
                        .ok()
                        .and_then(|index| self.samples.get(index))
                        .copied()
                        .unwrap_or(0);
                    sum += f64::from(sample) * coefficient;
                    weight += coefficient;
                }
                sum / weight
            } else {
                let center = self.position as f64 / self.target_rate as f64 - 1.0;
                let index = center.floor() as isize;
                let fraction = center - index as f64;
                let left = usize::try_from(index)
                    .ok()
                    .and_then(|index| self.samples.get(index))
                    .copied()
                    .unwrap_or(0);
                let right = usize::try_from(index + 1)
                    .ok()
                    .and_then(|index| self.samples.get(index))
                    .copied()
                    .unwrap_or(0);
                f64::from(left) + fraction * f64::from(i32::from(right) - i32::from(left))
            };
            output.push(value.round().clamp(i16::MIN as f64, i16::MAX as f64) as i16);
            self.position += u64::from(self.source_rate);
        }
        let discard = self.samples.len().saturating_sub(65);
        self.samples.drain(..discard);
        self.position -= discard as u64 * u64::from(self.target_rate);
        output
    }
}

struct CaptureBuffer {
    samples: Vec<i16>,
    resampler: AudioResampler,
}

/// Captures audio from the default input device and delivers 160-sample
///
/// The cpal Stream is kept alive on a dedicated OS thread so that
/// `AudioCapture` is Send + Sync (required for MediaSession which lives
/// across await points in tokio::spawn).
pub struct AudioCapture {
    _keep_alive: Option<std::sync::mpsc::Sender<()>>,
    _stream_thread: Option<thread::JoinHandle<()>>,
    stats: AudioCaptureStats,
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        drop(self._keep_alive.take());
        if let Some(stream_thread) = self._stream_thread.take() {
            let _ = stream_thread.join();
        }
    }
}

impl AudioCapture {
    pub fn stats(&self) -> AudioCaptureStats {
        self.stats.clone()
    }

    /// Try to open the default input device. Returns `None` (with a warning log)
    /// if no device is available or the stream cannot be built.
    ///
    /// The returned `Receiver` yields `Vec<i16>` frames of approximately 160
    pub fn start() -> Option<(Self, mpsc::Receiver<Vec<i16>>)> {
        #[cfg(target_os = "android")]
        {
            return start_android_capture();
        }

        #[cfg(target_os = "windows")]
        ensure_windows_audio_owner();
        let host = cpal::default_host();
        let device = match selected_device(&host, true) {
            Some(device) => device,
            None => {
                tracing::warn!("No audio input device found — mic capture disabled");
                return None;
            }
        };
        let dev_name = device.name().unwrap_or_else(|_| "unknown".into());
        tracing::info!("Audio input device: {}", dev_name);
        let (config, device_rate, sample_format) = match pick_input_config(&device) {
            Some(config) => config,
            None => {
                tracing::warn!("Cannot find suitable input config for {}", dev_name);
                return None;
            }
        };
        let channels = config.channels as usize;
        let frame_device_samples = (device_rate as usize * 20) / 1000 * channels;
        let need_resample = device_rate != TARGET_RATE;
        let (frame_tx, frame_rx) = mpsc::sync_channel::<Vec<i16>>(50);
        let stats = AudioCaptureStats::default();
        let stream_stats = stats.clone();
        let (keep_tx, keep_rx) = mpsc::channel::<()>();
        let (startup_tx, startup_rx) = mpsc::sync_channel::<bool>(1);

        let stream_thread = thread::spawn(move || {
            let buf = std::sync::Arc::new(std::sync::Mutex::new(CaptureBuffer {
                samples: Vec::with_capacity(frame_device_samples * 2),
                resampler: AudioResampler::new(device_rate, TARGET_RATE),
            }));
            let stream = match match sample_format {
                SampleFormat::I16 => device.build_input_stream(
                    &config,
                    {
                        let buf = buf.clone();
                        let frame_tx = frame_tx.clone();
                        let stats = stream_stats.clone();
                        move |data: &[i16], _: &cpal::InputCallbackInfo| {
                            push_input_samples(
                                data.iter().copied(),
                                &buf,
                                channels,
                                frame_device_samples,
                                need_resample,
                                &frame_tx,
                                &stats,
                            );
                        }
                    },
                    {
                        let stats = stream_stats.clone();
                        move |err| {
                            stats.stream_errors.fetch_add(1, Ordering::Relaxed);
                            tracing::warn!("Audio input stream error: {}", err);
                        }
                    },
                    None,
                ),
                SampleFormat::F32 => device.build_input_stream(
                    &config,
                    {
                        let buf = buf.clone();
                        let frame_tx = frame_tx.clone();
                        let stats = stream_stats.clone();
                        move |data: &[f32], _: &cpal::InputCallbackInfo| {
                            push_input_samples(
                                data.iter()
                                    .map(|&sample| (sample.clamp(-1.0, 1.0) * 32767.0) as i16),
                                &buf,
                                channels,
                                frame_device_samples,
                                need_resample,
                                &frame_tx,
                                &stats,
                            );
                        }
                    },
                    {
                        let stats = stream_stats.clone();
                        move |err| {
                            stats.stream_errors.fetch_add(1, Ordering::Relaxed);
                            tracing::warn!("Audio input stream error: {}", err);
                        }
                    },
                    None,
                ),
                SampleFormat::U16 => device.build_input_stream(
                    &config,
                    {
                        let buf = buf.clone();
                        let frame_tx = frame_tx.clone();
                        let stats = stream_stats.clone();
                        move |data: &[u16], _: &cpal::InputCallbackInfo| {
                            push_input_samples(
                                data.iter().map(|&sample| (sample as i32 - 32768) as i16),
                                &buf,
                                channels,
                                frame_device_samples,
                                need_resample,
                                &frame_tx,
                                &stats,
                            );
                        }
                    },
                    {
                        let stats = stream_stats.clone();
                        move |err| {
                            stats.stream_errors.fetch_add(1, Ordering::Relaxed);
                            tracing::warn!("Audio input stream error: {}", err);
                        }
                    },
                    None,
                ),
                SampleFormat::I32 => device.build_input_stream(
                    &config,
                    {
                        let buf = buf.clone();
                        let frame_tx = frame_tx.clone();
                        let stats = stream_stats.clone();
                        move |data: &[i32], _: &cpal::InputCallbackInfo| {
                            push_input_samples(
                                convert_input_samples(data),
                                &buf,
                                channels,
                                frame_device_samples,
                                need_resample,
                                &frame_tx,
                                &stats,
                            );
                        }
                    },
                    {
                        let stats = stream_stats.clone();
                        move |err| {
                            stats.stream_errors.fetch_add(1, Ordering::Relaxed);
                            tracing::warn!("Audio input stream error: {}", err);
                        }
                    },
                    None,
                ),
                SampleFormat::F64 => device.build_input_stream(
                    &config,
                    {
                        let buf = buf.clone();
                        let frame_tx = frame_tx.clone();
                        let stats = stream_stats.clone();
                        move |data: &[f64], _: &cpal::InputCallbackInfo| {
                            push_input_samples(
                                convert_input_samples(data),
                                &buf,
                                channels,
                                frame_device_samples,
                                need_resample,
                                &frame_tx,
                                &stats,
                            );
                        }
                    },
                    {
                        let stats = stream_stats.clone();
                        move |err| {
                            stats.stream_errors.fetch_add(1, Ordering::Relaxed);
                            tracing::warn!("Audio input stream error: {}", err);
                        }
                    },
                    None,
                ),
                _ => Err(cpal::BuildStreamError::StreamConfigNotSupported),
            } {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("Failed to build audio input stream: {}", e);
                    let _ = startup_tx.send(false);
                    return;
                }
            };

            if let Err(e) = stream.play() {
                tracing::warn!("Failed to start audio input stream: {}", e);
                let _ = startup_tx.send(false);
                return;
            }

            tracing::info!(
                "Audio capture started (device {}Hz {}ch, target {}Hz mono)",
                device_rate,
                channels,
                TARGET_RATE
            );
            let _ = startup_tx.send(true);

            let _ = keep_rx.recv();
            drop(stream);
        });

        match startup_rx.recv_timeout(std::time::Duration::from_secs(2)) {
            Ok(true) => Some((
                AudioCapture {
                    _keep_alive: Some(keep_tx),
                    _stream_thread: Some(stream_thread),
                    stats,
                },
                frame_rx,
            )),
            Ok(false) | Err(_) => {
                abandon_failed_capture_startup(keep_tx, stream_thread);
                None
            }
        }
    }
}

/// Plays audio to the default output device. Accepts 160-sample (20ms at 8kHz)
///
/// Like AudioCapture, the cpal Stream lives on a dedicated OS thread.
pub struct AudioPlayback {
    _keep_alive: Option<std::sync::mpsc::Sender<()>>,
    _stream_thread: Option<thread::JoinHandle<()>>,
    stats: AudioPlaybackStats,
}

impl Drop for AudioPlayback {
    fn drop(&mut self) {
        drop(self._keep_alive.take());
        if let Some(stream_thread) = self._stream_thread.take() {
            let _ = stream_thread.join();
        }
    }
}

impl AudioPlayback {
    pub fn stats(&self) -> AudioPlaybackStats {
        self.stats.clone()
    }

    /// Try to open the default output device. Returns `None` (with a warning log)
    /// if no device is available or the stream cannot be built.
    ///
    /// Send `Vec<i16>` frames of 160 samples (20ms at 8000 Hz) into the
    /// returned `SyncSender`.
    pub fn start() -> Option<(Self, mpsc::SyncSender<Vec<i16>>)> {
        #[cfg(target_os = "android")]
        {
            return start_android_playback();
        }

        #[cfg(target_os = "windows")]
        ensure_windows_audio_owner();
        let host = cpal::default_host();
        let device = match selected_device(&host, false) {
            Some(device) => device,
            None => {
                tracing::warn!("No audio output device found — speaker playback disabled");
                return None;
            }
        };
        let dev_name = device.name().unwrap_or_else(|_| "unknown".into());
        tracing::info!("Audio output device: {}", dev_name);
        let (config, device_rate, sample_format) = match pick_output_config(&device) {
            Some(config) => config,
            None => {
                tracing::warn!("Cannot find suitable output config for {}", dev_name);
                return None;
            }
        };
        let need_resample = device_rate != TARGET_RATE;
        let (frame_tx, frame_rx) = mpsc::sync_channel::<Vec<i16>>(50);
        let (keep_tx, keep_rx) = mpsc::channel::<()>();
        let stats = AudioPlaybackStats::default();
        let feeder_stats = stats.clone();
        let render_stats = stats.clone();
        let error_stats = stats.clone();
        let (startup_tx, startup_rx) = mpsc::sync_channel::<bool>(1);

        let stream_thread = thread::spawn(move || {
            // Ring buffer fed by a feeder thread, drained by the output callback.
            let ring = std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::VecDeque::<i16>::with_capacity(
                    (device_rate as usize / 1000) * 200,
                ),
            ));
            let ring2 = ring.clone();
            let prebuffer_samples = (device_rate as usize * 60) / 1000;

            thread::spawn(move || {
                let mut resampler = AudioResampler::new(TARGET_RATE, device_rate);
                while let Ok(frame) = frame_rx.recv() {
                    feeder_stats.frames_received.fetch_add(1, Ordering::Relaxed);
                    let samples = if need_resample {
                        resampler.process(&frame)
                    } else {
                        frame
                    };
                    let mut r = ring2.lock().unwrap();
                    r.extend(samples.iter());
                }
            });

            let stream = match match sample_format {
                SampleFormat::I16 => build_playback_stream::<i16>(
                    &device,
                    &config,
                    ring,
                    prebuffer_samples,
                    render_stats,
                    error_stats,
                ),
                SampleFormat::F32 => build_playback_stream::<f32>(
                    &device,
                    &config,
                    ring,
                    prebuffer_samples,
                    render_stats,
                    error_stats,
                ),
                SampleFormat::U16 => build_playback_stream::<u16>(
                    &device,
                    &config,
                    ring,
                    prebuffer_samples,
                    render_stats,
                    error_stats,
                ),
                SampleFormat::I32 => build_playback_stream::<i32>(
                    &device,
                    &config,
                    ring,
                    prebuffer_samples,
                    render_stats,
                    error_stats,
                ),
                SampleFormat::F64 => build_playback_stream::<f64>(
                    &device,
                    &config,
                    ring,
                    prebuffer_samples,
                    render_stats,
                    error_stats,
                ),
                _ => unreachable!(),
            } {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("Failed to build audio output stream: {}", e);
                    let _ = startup_tx.send(false);
                    return;
                }
            };

            if let Err(e) = stream.play() {
                tracing::warn!("Failed to start audio output stream: {}", e);
                let _ = startup_tx.send(false);
                return;
            }

            tracing::info!(
                "Audio playback started (device {}Hz {}ch, target {}Hz mono)",
                device_rate,
                config.channels,
                TARGET_RATE
            );
            let _ = startup_tx.send(true);

            let _ = keep_rx.recv();
            drop(stream);
        });

        match startup_rx.recv_timeout(std::time::Duration::from_secs(2)) {
            Ok(true) => Some((
                AudioPlayback {
                    _keep_alive: Some(keep_tx),
                    _stream_thread: Some(stream_thread),
                    stats,
                },
                frame_tx,
            )),
            Ok(false) => {
                drop(keep_tx);
                let _ = stream_thread.join();
                None
            }
            Err(error) => {
                tracing::warn!("Timed out starting audio output stream: {}", error);
                drop(keep_tx);
                let _ = stream_thread.join();
                None
            }
        }
    }
}

fn convert_input_samples<T>(data: &[T]) -> impl Iterator<Item = i16> + '_
where
    T: cpal::Sample,
    i16: cpal::FromSample<T>,
{
    data.iter().map(|sample| sample.to_sample())
}

fn push_input_samples(
    data: impl IntoIterator<Item = i16>,
    buffer: &std::sync::Arc<std::sync::Mutex<CaptureBuffer>>,
    channels: usize,
    frame_device_samples: usize,
    need_resample: bool,
    frame_tx: &mpsc::SyncSender<Vec<i16>>,
    stats: &AudioCaptureStats,
) {
    let mut acc = buffer.lock().unwrap();
    acc.samples.extend(data);
    while acc.samples.len() >= frame_device_samples {
        let mono = downmix_input_samples(&acc.samples[..frame_device_samples], channels);
        acc.samples.drain(..frame_device_samples);
        let frame = if need_resample {
            acc.resampler.process(&mono)
        } else {
            mono
        };
        enqueue_frame(frame_tx, frame, stats);
    }
}

fn enqueue_frame(
    frame_tx: &mpsc::SyncSender<Vec<i16>>,
    frame: Vec<i16>,
    stats: &AudioCaptureStats,
) {
    stats.produced.fetch_add(1, Ordering::Relaxed);
    match frame_tx.try_send(frame) {
        Ok(()) => {
            stats.enqueued.fetch_add(1, Ordering::Relaxed);
        }
        Err(mpsc::TrySendError::Full(_)) => {
            stats.full.fetch_add(1, Ordering::Relaxed);
        }
        Err(mpsc::TrySendError::Disconnected(_)) => {
            stats.disconnected.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn abandon_failed_capture_startup(
    keep_tx: mpsc::Sender<()>,
    stream_thread: thread::JoinHandle<()>,
) {
    drop(keep_tx);
    drop(stream_thread);
}

fn downmix_input_samples(samples: &[i16], channels: usize) -> Vec<i16> {
    if channels <= 1 {
        return samples.to_vec();
    }
    if channels == 2 {
        let (frames, _) = samples.as_chunks::<2>();
        let (mut left, mut right) = (0u64, 0u64);
        for frame in frames {
            left += i64::from(frame[0]).unsigned_abs();
            right += i64::from(frame[1]).unsigned_abs();
        }
        let channel = usize::from(right > left);
        return frames.iter().map(|frame| frame[channel]).collect();
    }
    let mut channel_energy = vec![0u64; channels];
    for frame in samples.chunks_exact(channels) {
        for (channel, &sample) in frame.iter().enumerate() {
            channel_energy[channel] += i64::from(sample).unsigned_abs();
        }
    }
    let channel = channel_energy
        .iter()
        .enumerate()
        .max_by_key(|(_, energy)| *energy)
        .map(|(channel, _)| channel)
        .unwrap_or(0);
    samples
        .chunks_exact(channels)
        .map(|frame| frame[channel])
        .collect()
}

fn build_playback_stream<T>(
    device: &Device,
    config: &StreamConfig,
    ring: std::sync::Arc<Mutex<std::collections::VecDeque<i16>>>,
    prebuffer_samples: usize,
    render_stats: AudioPlaybackStats,
    error_stats: AudioPlaybackStats,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::SizedSample + cpal::FromSample<i16>,
{
    let channels = config.channels as usize;
    let mut primed = false;
    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mut samples = ring.lock().unwrap();
            primed |= samples.len() >= prebuffer_samples;
            for frame in data.chunks_mut(channels) {
                let sample = if primed { samples.pop_front() } else { None };
                if sample.is_some() {
                    render_stats
                        .samples_rendered
                        .fetch_add(1, Ordering::Relaxed);
                } else {
                    primed = false;
                }
                frame.fill(T::from_sample(sample.unwrap_or(0)));
            }
        },
        move |error| {
            error_stats.stream_errors.fetch_add(1, Ordering::Relaxed);
            tracing::warn!("Audio output stream error: {error}");
        },
        None,
    )
}

fn pick_output_config(device: &Device) -> Option<(StreamConfig, u32, SampleFormat)> {
    if let Ok(config) = device.default_output_config() {
        if supports_audio_format(config.sample_format()) {
            return Some((
                config.config(),
                config.sample_rate().0,
                config.sample_format(),
            ));
        }
    }
    device
        .supported_output_configs()
        .ok()?
        .filter(|config| supports_audio_format(config.sample_format()))
        .map(|config| config.with_max_sample_rate())
        .next()
        .map(|config| {
            (
                config.config(),
                config.sample_rate().0,
                config.sample_format(),
            )
        })
}

fn supports_audio_format(format: SampleFormat) -> bool {
    matches!(
        format,
        SampleFormat::I16
            | SampleFormat::F32
            | SampleFormat::U16
            | SampleFormat::I32
            | SampleFormat::F64
    )
}

fn pick_input_config(device: &Device) -> Option<(StreamConfig, u32, SampleFormat)> {
    if let Ok(config) = device.default_input_config() {
        if supports_audio_format(config.sample_format()) {
            return Some((
                config.config(),
                config.sample_rate().0,
                config.sample_format(),
            ));
        }
    }

    let configs: Vec<cpal::SupportedStreamConfigRange> =
        device.supported_input_configs().ok()?.collect();
    for format in [
        SampleFormat::I16,
        SampleFormat::F32,
        SampleFormat::U16,
        SampleFormat::I32,
        SampleFormat::F64,
    ] {
        for cfg in &configs {
            if supports_audio_format(cfg.sample_format())
                && cfg.sample_format() == format
                && cfg.channels() == 1
                && cfg.min_sample_rate() <= SampleRate(TARGET_RATE)
                && cfg.max_sample_rate() >= SampleRate(TARGET_RATE)
            {
                return Some((
                    (*cfg).with_sample_rate(SampleRate(TARGET_RATE)).into(),
                    TARGET_RATE,
                    format,
                ));
            }
        }
    }
    for cfg in &configs {
        if supports_audio_format(cfg.sample_format()) {
            let rate = cfg.max_sample_rate().0;
            return Some((
                (*cfg).with_sample_rate(SampleRate(rate)).into(),
                rate,
                cfg.sample_format(),
            ));
        }
    }
    None
}

/// Capture 3 seconds of microphone audio, then play it back through the speaker.
///
/// Prints a VU meter bar every 100ms during capture so you can see the level.
pub fn mic_test() -> anyhow::Result<()> {
    use anyhow::bail;

    println!("=== Microphone Test ===");
    println!("Recording for 3 seconds — speak now!\n");

    let (capture, mic_rx) = match AudioCapture::start() {
        Some(c) => c,
        None => bail!("No audio input device found"),
    };

    let mut frames: Vec<Vec<i16>> = Vec::with_capacity(150);
    let start = std::time::Instant::now();
    let mut last_vu = start;

    while start.elapsed() < std::time::Duration::from_secs(3) {
        match mic_rx.recv_timeout(std::time::Duration::from_millis(25)) {
            Ok(frame) => {
                if last_vu.elapsed() >= std::time::Duration::from_millis(100) {
                    let rms = rms_level(&frame);
                    let db = if rms > 0.0 { 20.0 * rms.log10() } else { -60.0 };
                    let bar_len = ((db + 60.0) / 60.0 * 30.0).clamp(0.0, 30.0) as usize;
                    let bar: String = "█".repeat(bar_len) + &"░".repeat(30 - bar_len);
                    print!("\r  [{bar}] {db:5.1} dBFS ");
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                    last_vu = std::time::Instant::now();
                }
                frames.push(frame);
            }
            Err(_) => continue,
        }
    }
    drop(capture);
    let captured_peak = frames
        .iter()
        .flat_map(|frame| frame.iter())
        .map(|sample| i32::from(*sample).unsigned_abs())
        .max()
        .unwrap_or(0);
    println!(
        "\n\nCaptured {} frames ({:.1}s), peak {}",
        frames.len(),
        frames.len() as f64 * 0.02,
        captured_peak
    );

    println!("Playing back...\n");
    let (playback, speaker_tx) = match AudioPlayback::start() {
        Some(p) => p,
        None => bail!("No audio output device found"),
    };

    for frame in &frames {
        let _ = speaker_tx.send(frame.clone());
        thread::sleep(std::time::Duration::from_millis(20));
    }
    thread::sleep(std::time::Duration::from_millis(200));
    drop(playback);

    println!("Done.");
    Ok(())
}

/// Compute RMS level of a frame, normalized to 0.0–1.0 range.
fn rms_level(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum_sq / samples.len() as f64).sqrt() / 32768.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling_is_continuous_across_device_callbacks() {
        for (source, target) in [(48000, 8000), (44100, 8000), (8000, 48000)] {
            let input: Vec<i16> = (0..2048)
                .map(|index| ((index as f64 * 0.13).sin() * 10000.0) as i16)
                .collect();
            let expected = AudioResampler::new(source, target).process(&input);
            let mut resampler = AudioResampler::new(source, target);
            let actual: Vec<i16> = input
                .chunks(137)
                .flat_map(|chunk| resampler.process(chunk))
                .collect();
            assert_eq!(actual.len(), expected.len());
            assert!(actual
                .iter()
                .zip(&expected)
                .all(|(left, right)| (i32::from(*left) - i32::from(*right)).abs() <= 1));
        }
    }

    #[test]
    fn microphone_downsampling_rejects_aliases_without_losing_speech() {
        for (frequency, minimum, maximum) in [(1000.0, 6500.0, 7500.0), (6000.0, 0.0, 100.0)] {
            let input: Vec<i16> = (0..4800)
                .map(|index| {
                    (10000.0
                        * (2.0 * std::f64::consts::PI * frequency * index as f64 / 48000.0).sin())
                        as i16
                })
                .collect();
            let output = AudioResampler::new(48000, 8000).process(&input);
            let rms = rms_level(&output[32..]) * 32768.0;
            assert!(
                rms >= minimum && rms <= maximum,
                "frequency={frequency}, rms={rms}"
            );
        }
    }

    #[test]
    fn enqueue_frame_records_each_queue_outcome() {
        let stats = AudioCaptureStats::default();
        let (tx, rx) = mpsc::sync_channel(1);

        enqueue_frame(&tx, vec![1], &stats);
        enqueue_frame(&tx, vec![2], &stats);
        drop(rx);
        enqueue_frame(&tx, vec![3], &stats);

        assert_eq!(stats.produced(), 3);
        assert_eq!(stats.enqueued(), 1);
        assert_eq!(stats.full(), 1);
        assert_eq!(stats.disconnected(), 1);
    }

    #[test]
    fn stalled_capture_startup_does_not_block_the_caller() {
        let (keep_tx, _keep_rx) = mpsc::channel();
        let stream_thread = thread::spawn(|| {
            thread::sleep(std::time::Duration::from_millis(250));
        });
        let started = std::time::Instant::now();

        abandon_failed_capture_startup(keep_tx, stream_thread);

        assert!(started.elapsed() < std::time::Duration::from_millis(100));
    }

    #[test]
    fn accepts_common_windows_microphone_sample_formats() {
        assert!(supports_audio_format(SampleFormat::I16));
        assert!(supports_audio_format(SampleFormat::F32));
        assert!(supports_audio_format(SampleFormat::U16));
        assert!(supports_audio_format(SampleFormat::F64));
        assert!(supports_audio_format(SampleFormat::I32));
    }

    #[test]
    fn preserves_signal_from_strongest_stereo_microphone_channel() {
        let samples = downmix_input_samples(&[100, 1000, 100, 1000], 2);
        assert_eq!(samples, vec![1000, 1000]);
    }

    #[test]
    fn preserves_signal_from_strongest_multichannel_microphone_channel() {
        let samples = downmix_input_samples(&[0, 1000, 0, 0, 1000, 0], 3);
        assert_eq!(samples, vec![1000, 1000]);
    }

    #[test]
    fn test_resample_identity() {
        let input: Vec<i16> = (0..160).collect();
        let out = AudioResampler::new(8000, 8000).process(&input);
        assert_eq!(out, input);
    }

    #[test]
    fn test_resample_upsample_6x() {
        let input: Vec<i16> = vec![0, 1000, 2000, 0];
        let out = AudioResampler::new(8000, 48000).process(&input);
        assert_eq!(out.len(), 24);
        assert_eq!(out[0], 0);
        assert!(out[9] > 0 && out[9] < 1000);
    }

    #[test]
    fn test_resample_downsample_6x() {
        let input: Vec<i16> = (0..48).map(|i| (i * 100) as i16).collect();
        let out = AudioResampler::new(48000, 8000).process(&input);
        assert_eq!(out.len(), 8);
        assert_eq!(out[0], 0);
    }

    #[test]
    fn test_resample_empty() {
        let out = AudioResampler::new(48000, 8000).process(&[]);
        assert!(out.is_empty());
    }

    #[test]
    fn test_audio_capture_graceful_on_headless() {
        let result = AudioCapture::start();
        if result.is_none() {
            tracing::info!("No audio input device (expected on headless)");
        }
    }

    #[test]
    fn test_audio_playback_graceful_on_headless() {
        let result = AudioPlayback::start();
        if result.is_none() {
            tracing::info!("No audio output device (expected on headless)");
        }
    }
}
