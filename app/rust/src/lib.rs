#![deny(clippy::allow_attributes)]

pub mod api;
mod frb_generated;
pub mod web_bridge;

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_app_microslop_MainActivity_initializeNativeStorage(
    mut env: jni::JNIEnv,
    _activity: jni::objects::JObject,
    context: jni::objects::JObject,
    cache_dir: jni::objects::JString,
) {
    let Ok(cache_dir) = env.get_string(&cache_dir) else {
        return;
    };
    ost_platform::initialize_android_context(env, context, cache_dir.to_string_lossy().as_ref());
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_app_microslop_MainActivity_setNativeCallAudioDevice(
    _env: jni::JNIEnv,
    _activity: jni::objects::JObject,
    device_id: jni::sys::jint,
) {
    teams_cli::calling::audio::set_android_output_device_id(device_id);
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_app_microslop_MainActivity_pauseNativeCallAudio(
    _env: jni::JNIEnv,
    _activity: jni::objects::JObject,
) -> jni::sys::jboolean {
    teams_cli::calling::audio::pause_android_call_audio() as jni::sys::jboolean
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_app_microslop_MainActivity_resumeNativeCallAudio(
    _env: jni::JNIEnv,
    _activity: jni::objects::JObject,
) -> jni::sys::jboolean {
    teams_cli::calling::audio::resume_android_call_audio() as jni::sys::jboolean
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_app_microslop_MainActivity_pushNativeCameraFrame(
    mut env: jni::JNIEnv,
    _activity: jni::objects::JObject,
    width: jni::sys::jint,
    height: jni::sys::jint,
    data: jni::objects::JByteArray,
) {
    if let Ok(bytes) = env.convert_byte_array(data) {
        teams_cli::calling::external_camera::push_frame(width as u32, height as u32, bytes);
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_app_microslop_MainActivity_nativeCameraFramesReceived(
    _env: jni::JNIEnv,
    _activity: jni::objects::JObject,
) -> jni::sys::jint {
    teams_cli::calling::external_camera::frames_received() as jni::sys::jint
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_app_microslop_MainActivity_nativeRemoteVideoFrame(
    env: jni::JNIEnv,
    _activity: jni::objects::JObject,
) -> jni::sys::jbyteArray {
    let Some(frame) = teams_cli::calling::external_display::latest_frame() else {
        return std::ptr::null_mut();
    };
    let mut packed = Vec::with_capacity(frame.data.len() + 8);
    packed.extend_from_slice(&frame.width.to_be_bytes());
    packed.extend_from_slice(&frame.height.to_be_bytes());
    packed.extend_from_slice(&frame.data);
    env.byte_array_from_slice(&packed)
        .map(jni::objects::JByteArray::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_app_microslop_MainActivity_probeNativeCameraFrameEncoding(
    mut env: jni::JNIEnv,
    _activity: jni::objects::JObject,
    width: jni::sys::jint,
    height: jni::sys::jint,
    data: jni::objects::JByteArray,
) -> jni::sys::jboolean {
    let Ok(bytes) = env.convert_byte_array(data) else {
        return 0;
    };
    let Ok(mut encoder) =
        teams_cli::calling::codec::H264Encoder::new(width as u32, height as u32, 15.0, 256)
    else {
        return 0;
    };
    encoder.encode(&bytes).is_ok_and(|nals| !nals.is_empty()) as jni::sys::jboolean
}
