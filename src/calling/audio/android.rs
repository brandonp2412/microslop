use super::*;

use oboe::{
    AudioInputCallback, AudioInputStreamSafe, AudioOutputCallback, AudioOutputStreamSafe,
    AudioStream, AudioStreamBuilder, ContentType, DataCallbackResult, InputPreset, Mono,
    PerformanceMode, SharingMode, Usage,
};
use std::collections::VecDeque;
use std::sync::{atomic::AtomicI32, Arc, Mutex as StdMutex};

#[cfg(target_os = "android")]
#[derive(Clone, Copy)]
enum AndroidAudioAction {
    Pause,
    Resume,
}

#[cfg(target_os = "android")]
struct AndroidAudioCommand {
    action: AndroidAudioAction,
    ack: mpsc::SyncSender<bool>,
}

#[cfg(target_os = "android")]
type AndroidControlSlot = OnceLock<StdMutex<Option<mpsc::Sender<AndroidAudioCommand>>>>;

#[cfg(target_os = "android")]
static ANDROID_CAPTURE_CONTROL: AndroidControlSlot = OnceLock::new();
#[cfg(target_os = "android")]
static ANDROID_PLAYBACK_CONTROL: AndroidControlSlot = OnceLock::new();
#[cfg(target_os = "android")]
static ANDROID_OUTPUT_DEVICE_ID: AtomicI32 = AtomicI32::new(0);

#[cfg(target_os = "android")]
fn android_control_slot(
    slot: &AndroidControlSlot,
) -> &StdMutex<Option<mpsc::Sender<AndroidAudioCommand>>> {
    slot.get_or_init(|| StdMutex::new(None))
}

#[cfg(target_os = "android")]
fn control_android_stream(slot: &AndroidControlSlot, action: AndroidAudioAction) -> bool {
    let sender = android_control_slot(slot).lock().unwrap().clone();
    let Some(sender) = sender else {
        return true;
    };
    let (ack_tx, ack_rx) = mpsc::sync_channel(1);
    if sender
        .send(AndroidAudioCommand {
            action,
            ack: ack_tx,
        })
        .is_err()
    {
        return true;
    }
    ack_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap_or(false)
}

/// Set the Android AudioDeviceInfo id that Oboe playback should bind to.
/// A value <= 0 means use the system default route.
#[cfg(target_os = "android")]
pub fn set_android_output_device_id(device_id: i32) {
    ANDROID_OUTPUT_DEVICE_ID.store(device_id, Ordering::Relaxed);
    tracing::info!("Android call output device id set to {}", device_id);
}

/// Stop the live Android Oboe devices while keeping the RTP media channels alive.
/// This is used immediately before Android changes the communication output route.
#[cfg(target_os = "android")]
pub fn pause_android_call_audio() -> bool {
    let capture_ok = control_android_stream(&ANDROID_CAPTURE_CONTROL, AndroidAudioAction::Pause);
    let playback_ok = control_android_stream(&ANDROID_PLAYBACK_CONTROL, AndroidAudioAction::Pause);
    capture_ok && playback_ok
}

/// Reopen Android Oboe devices after a communication-route change. Playback is
/// deliberately opened before capture, matching the stable initial call startup.
#[cfg(target_os = "android")]
pub fn resume_android_call_audio() -> bool {
    let playback_ok = control_android_stream(&ANDROID_PLAYBACK_CONTROL, AndroidAudioAction::Resume);
    let capture_ok = control_android_stream(&ANDROID_CAPTURE_CONTROL, AndroidAudioAction::Resume);
    playback_ok && capture_ok
}

#[cfg(target_os = "android")]
struct AndroidCaptureCallback {
    frame_tx: mpsc::SyncSender<Vec<i16>>,
    pending: Vec<i16>,
}

#[cfg(target_os = "android")]
impl AudioInputCallback for AndroidCaptureCallback {
    type FrameType = (i16, Mono);

    fn on_audio_ready(
        &mut self,
        _audio_stream: &mut dyn AudioInputStreamSafe,
        audio_data: &[i16],
    ) -> DataCallbackResult {
        self.pending.extend_from_slice(audio_data);
        while self.pending.len() >= FRAME_SAMPLES {
            let frame: Vec<i16> = self.pending.drain(..FRAME_SAMPLES).collect();
            let _ = self.frame_tx.try_send(frame);
        }
        DataCallbackResult::Continue
    }

    fn on_error_before_close(
        &mut self,
        _audio_stream: &mut dyn AudioInputStreamSafe,
        error: oboe::Error,
    ) {
        tracing::warn!("Android voice input stream error: {:?}", error);
    }
}

#[cfg(target_os = "android")]
struct AndroidPlaybackCallback {
    frame_rx: Arc<StdMutex<mpsc::Receiver<Vec<i16>>>>,
    pending: VecDeque<i16>,
    stats: AudioPlaybackStats,
}

#[cfg(target_os = "android")]
impl AudioOutputCallback for AndroidPlaybackCallback {
    type FrameType = (i16, Mono);

    fn on_audio_ready(
        &mut self,
        _audio_stream: &mut dyn AudioOutputStreamSafe,
        audio_data: &mut [i16],
    ) -> DataCallbackResult {
        while self.pending.len() < audio_data.len() {
            let next = self.frame_rx.lock().unwrap().try_recv();
            match next {
                Ok(frame) => {
                    self.stats.frames_received.fetch_add(1, Ordering::Relaxed);
                    self.pending.extend(frame);
                }
                Err(_) => break,
            }
        }
        for sample in audio_data.iter_mut() {
            let next = self.pending.pop_front();
            if next.is_some() {
                self.stats.samples_rendered.fetch_add(1, Ordering::Relaxed);
            }
            *sample = next.unwrap_or(0);
        }
        DataCallbackResult::Continue
    }

    fn on_error_before_close(
        &mut self,
        _audio_stream: &mut dyn AudioOutputStreamSafe,
        error: oboe::Error,
    ) {
        self.stats.stream_errors.fetch_add(1, Ordering::Relaxed);
        tracing::warn!("Android voice output stream error: {:?}", error);
    }
}

#[cfg(target_os = "android")]
pub(super) fn start_android_capture() -> Option<(AudioCapture, mpsc::Receiver<Vec<i16>>)> {
    let (frame_tx, frame_rx) = mpsc::sync_channel::<Vec<i16>>(50);
    let (keep_tx, keep_rx) = mpsc::channel::<()>();
    let (startup_tx, startup_rx) = mpsc::sync_channel::<bool>(1);
    let (control_tx, control_rx) = mpsc::channel::<AndroidAudioCommand>();
    *android_control_slot(&ANDROID_CAPTURE_CONTROL)
        .lock()
        .unwrap() = Some(control_tx);

    let stream_thread = thread::spawn(move || {
        let open_stream = || {
            let callback = AndroidCaptureCallback {
                frame_tx: frame_tx.clone(),
                pending: Vec::with_capacity(FRAME_SAMPLES * 2),
            };
            let mut stream = match AudioStreamBuilder::default()
                .set_input()
                .set_format::<i16>()
                .set_mono()
                .set_sample_rate(TARGET_RATE as i32)
                .set_frames_per_callback(FRAME_SAMPLES as i32)
                .set_sharing_mode(SharingMode::Shared)
                .set_performance_mode(PerformanceMode::LowLatency)
                .set_input_preset(InputPreset::VoiceCommunication)
                .set_callback(callback)
                .open_stream()
            {
                Ok(stream) => stream,
                Err(error) => {
                    tracing::warn!("Failed to open Android voice input stream: {:?}", error);
                    return None;
                }
            };
            if let Err(error) = stream.start() {
                tracing::warn!("Failed to start Android voice input stream: {:?}", error);
                return None;
            }
            tracing::info!("Android voice capture started at {}Hz mono", TARGET_RATE);
            Some(stream)
        };

        let mut stream = match open_stream() {
            Some(stream) => Some(stream),
            None => {
                let _ = startup_tx.send(false);
                return;
            }
        };
        let _ = startup_tx.send(true);

        loop {
            match control_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(command) => {
                    let ok = match command.action {
                        AndroidAudioAction::Pause => {
                            if let Some(mut active) = stream.take() {
                                let _ = active.stop();
                                drop(active);
                            }
                            true
                        }
                        AndroidAudioAction::Resume => {
                            if stream.is_none() {
                                stream = open_stream();
                            }
                            stream.is_some()
                        }
                    };
                    let _ = command.ack.send(ok);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if matches!(keep_rx.try_recv(), Err(mpsc::TryRecvError::Disconnected)) {
                break;
            }
        }

        if let Some(mut active) = stream {
            let _ = active.stop();
        }
    });

    match startup_rx.recv_timeout(std::time::Duration::from_secs(3)) {
        Ok(true) => Some((
            AudioCapture {
                _keep_alive: Some(keep_tx),
                _stream_thread: Some(stream_thread),
                stats: AudioCaptureStats::default(),
            },
            frame_rx,
        )),
        Ok(false) => {
            drop(keep_tx);
            let _ = stream_thread.join();
            None
        }
        Err(error) => {
            tracing::warn!("Timed out starting Android voice input stream: {}", error);
            drop(keep_tx);
            let _ = stream_thread.join();
            None
        }
    }
}

#[cfg(target_os = "android")]
pub(super) fn start_android_playback() -> Option<(AudioPlayback, mpsc::SyncSender<Vec<i16>>)> {
    let (frame_tx, frame_rx) = mpsc::sync_channel::<Vec<i16>>(50);
    let frame_rx = Arc::new(StdMutex::new(frame_rx));
    let (keep_tx, keep_rx) = mpsc::channel::<()>();
    let (startup_tx, startup_rx) = mpsc::sync_channel::<bool>(1);
    let (control_tx, control_rx) = mpsc::channel::<AndroidAudioCommand>();
    let stats = AudioPlaybackStats::default();
    let callback_stats = stats.clone();
    *android_control_slot(&ANDROID_PLAYBACK_CONTROL)
        .lock()
        .unwrap() = Some(control_tx);

    let stream_thread = thread::spawn(move || {
        let open_stream = || {
            let callback = AndroidPlaybackCallback {
                frame_rx: frame_rx.clone(),
                pending: VecDeque::with_capacity(FRAME_SAMPLES * 10),
                stats: callback_stats.clone(),
            };
            let builder = AudioStreamBuilder::default()
                .set_output()
                .set_format::<i16>()
                .set_mono()
                .set_sample_rate(TARGET_RATE as i32)
                .set_frames_per_callback(FRAME_SAMPLES as i32)
                .set_sharing_mode(SharingMode::Shared)
                .set_performance_mode(PerformanceMode::LowLatency)
                .set_usage(Usage::VoiceCommunication)
                .set_content_type(ContentType::Speech);
            let desired_device_id = ANDROID_OUTPUT_DEVICE_ID.load(Ordering::Relaxed);
            let builder = if desired_device_id > 0 {
                tracing::info!(
                    "Opening Android voice playback on device id {}",
                    desired_device_id
                );
                builder.set_device_id(desired_device_id)
            } else {
                builder
            };
            let mut stream = match builder.set_callback(callback).open_stream() {
                Ok(stream) => stream,
                Err(error) => {
                    tracing::warn!("Failed to open Android voice output stream: {:?}", error);
                    return None;
                }
            };
            if let Err(error) = stream.start() {
                tracing::warn!("Failed to start Android voice output stream: {:?}", error);
                return None;
            }
            tracing::info!("Android voice playback started at {}Hz mono", TARGET_RATE);
            Some(stream)
        };

        let mut stream = match open_stream() {
            Some(stream) => Some(stream),
            None => {
                let _ = startup_tx.send(false);
                return;
            }
        };
        let _ = startup_tx.send(true);

        loop {
            match control_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(command) => {
                    let ok = match command.action {
                        AndroidAudioAction::Pause => {
                            if let Some(mut active) = stream.take() {
                                let _ = active.stop();
                                drop(active);
                            }
                            true
                        }
                        AndroidAudioAction::Resume => {
                            if stream.is_none() {
                                stream = open_stream();
                            }
                            stream.is_some()
                        }
                    };
                    let _ = command.ack.send(ok);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if matches!(keep_rx.try_recv(), Err(mpsc::TryRecvError::Disconnected)) {
                break;
            }
        }

        if let Some(mut active) = stream {
            let _ = active.stop();
        }
    });

    match startup_rx.recv_timeout(std::time::Duration::from_secs(3)) {
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
            tracing::warn!("Timed out starting Android voice output stream: {}", error);
            drop(keep_tx);
            let _ = stream_thread.join();
            None
        }
    }
}
