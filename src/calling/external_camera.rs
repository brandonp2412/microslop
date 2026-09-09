use super::video::YuvFrame;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    mpsc, Mutex, OnceLock,
};

static CAMERA_SENDER: OnceLock<Mutex<Option<mpsc::SyncSender<YuvFrame>>>> = OnceLock::new();
static FRAMES_RECEIVED: AtomicU32 = AtomicU32::new(0);

pub fn subscribe() -> mpsc::Receiver<YuvFrame> {
    let (tx, rx) = mpsc::sync_channel(2);
    *CAMERA_SENDER.get_or_init(Default::default).lock().unwrap() = Some(tx);
    rx
}

pub fn push_frame(width: u32, height: u32, data: Vec<u8>) {
    FRAMES_RECEIVED.fetch_add(1, Ordering::Relaxed);
    let sender = CAMERA_SENDER
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .clone();
    if let Some(sender) = sender {
        let _ = sender.try_send(YuvFrame {
            width,
            height,
            data,
        });
    }
}

pub fn frames_received() -> u32 {
    FRAMES_RECEIVED.load(Ordering::Relaxed)
}

pub fn clear() {
    *CAMERA_SENDER.get_or_init(Default::default).lock().unwrap() = None;
}
