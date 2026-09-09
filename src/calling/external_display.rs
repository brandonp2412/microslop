use std::sync::{Mutex, OnceLock};

#[derive(Clone)]
pub struct RemoteVideoFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

static LATEST_FRAME: OnceLock<Mutex<Option<RemoteVideoFrame>>> = OnceLock::new();
static LATEST_LOCAL_FRAME: OnceLock<Mutex<Option<RemoteVideoFrame>>> = OnceLock::new();

pub fn push_frame(width: u32, height: u32, data: Vec<u8>) {
    *LATEST_FRAME.get_or_init(Default::default).lock().unwrap() = Some(RemoteVideoFrame {
        width,
        height,
        data,
    });
}

pub fn latest_frame() -> Option<RemoteVideoFrame> {
    LATEST_FRAME
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .clone()
}

pub fn push_local_frame(width: u32, height: u32, data: Vec<u8>) {
    *LATEST_LOCAL_FRAME
        .get_or_init(Default::default)
        .lock()
        .unwrap() = Some(RemoteVideoFrame {
        width,
        height,
        data,
    });
}

pub fn latest_local_frame() -> Option<RemoteVideoFrame> {
    LATEST_LOCAL_FRAME
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .clone()
}

pub fn clear_local() {
    *LATEST_LOCAL_FRAME
        .get_or_init(Default::default)
        .lock()
        .unwrap() = None;
}

pub fn clear() {
    *LATEST_FRAME.get_or_init(Default::default).lock().unwrap() = None;
    clear_local();
}

#[cfg(test)]
mod tests {
    use super::{latest_frame, latest_local_frame, push_frame, push_local_frame};

    #[test]
    fn local_and_remote_frames_are_kept_separately() {
        push_frame(2, 2, vec![1]);
        push_local_frame(3, 3, vec![2]);

        assert_eq!(latest_frame().unwrap().data, vec![1]);
        assert_eq!(latest_local_frame().unwrap().data, vec![2]);
    }
}
