package app.microslop

import android.app.Person
import android.content.Context
import android.content.Intent
import android.content.pm.ShortcutInfo
import android.content.pm.ShortcutManager
import android.graphics.BitmapFactory
import android.graphics.Color
import android.graphics.ImageFormat
import android.graphics.Rect
import android.graphics.SurfaceTexture
import android.graphics.YuvImage
import android.graphics.drawable.Icon
import android.hardware.camera2.CameraCaptureSession
import android.hardware.camera2.CameraCharacteristics
import android.hardware.camera2.CameraDevice
import android.hardware.camera2.CameraManager
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.media.Image
import android.media.ImageReader
import android.media.MediaRecorder
import android.media.ToneGenerator
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.HandlerThread
import android.util.Log
import android.view.Surface
import android.view.TextureView
import android.view.View
import android.widget.ImageView
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import io.flutter.plugin.common.StandardMessageCodec
import io.flutter.plugin.platform.PlatformView
import io.flutter.plugin.platform.PlatformViewFactory
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean

class MainActivity : FlutterActivity() {
    companion object {
        private const val CALL_AUDIO_CHANNEL = "microslop/call_audio"
        private const val CALL_VIDEO_CHANNEL = "microslop/call_video"
        private const val CALL_VIDEO_PREVIEW_VIEW = "microslop/call_camera_preview"
        private const val CALL_REMOTE_VIDEO_VIEW = "microslop/call_remote_video"
        private const val MESSAGE_MEDIA_CHANNEL = "microslop/message_media"
        private const val BACKGROUND_NOTIFICATIONS_CHANNEL = "microslop/background_notifications"
        private const val SHARE_TARGET_CHANNEL = "microslop/share_target"
        private const val CALL_AUDIO_TAG = "MicroslopCallAudio"
        private const val CALL_VIDEO_TAG = "MicroslopCallVideo"

        init {
            System.loadLibrary("ost_frb")
        }
    }

    private external fun initializeNativeStorage(context: Context, cacheDir: String)
    private external fun setNativeCallAudioDevice(deviceId: Int)
    private external fun pauseNativeCallAudio(): Boolean
    private external fun resumeNativeCallAudio(): Boolean
    private external fun pushNativeCameraFrame(width: Int, height: Int, data: ByteArray)
    private external fun nativeCameraFramesReceived(): Int
    private external fun nativeRemoteVideoFrame(): ByteArray?
    private external fun probeNativeCameraFrameEncoding(width: Int, height: Int, data: ByteArray): Boolean

    private val audioManager by lazy {
        getSystemService(Context.AUDIO_SERVICE) as AudioManager
    }
    private var ringback: ToneGenerator? = null
    private var cameraThread: HandlerThread? = null
    private var cameraDevice: CameraDevice? = null
    private var cameraSession: CameraCaptureSession? = null
    private var imageReader: ImageReader? = null
    private var cameraPreviewTexture: SurfaceTexture? = null
    private var cameraPreviewSurface: Surface? = null
    private var cameraFramesCaptured = 0
    private var latestCameraFrame: ByteArray? = null
    private var latestCameraWidth = 0
    private var latestCameraHeight = 0
    private var messageRecorder: MediaRecorder? = null
    private var messageRecordingFile: File? = null
    private var shareTargetChannel: MethodChannel? = null
    private var pendingShare: Map<String, Any>? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        initializeNativeStorage(applicationContext, applicationContext.cacheDir.absolutePath)
        pendingShare = sharedContent(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        val content = sharedContent(intent) ?: return
        val channel = shareTargetChannel
        if (channel == null) {
            pendingShare = content
        } else {
            pendingShare = null
            channel.invokeMethod("sharedContent", content)
        }
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        shareTargetChannel = MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger,
            SHARE_TARGET_CHANNEL,
        ).also { channel ->
            channel.setMethodCallHandler { call, result ->
                if (call.method == "takePendingShare") {
                    val content = pendingShare
                    pendingShare = null
                    result.success(content)
                } else {
                    result.notImplemented()
                }
            }
        }
        MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger,
            BACKGROUND_NOTIFICATIONS_CHANNEL,
        ).setMethodCallHandler { call, result ->
            when (call.method) {
                "setEnabled" -> {
                    val enabled = call.argument<Boolean>("enabled") == true
                    val intent = Intent(this, MessageWatchService::class.java)
                    if (enabled) {
                        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                            startForegroundService(intent)
                        } else {
                            startService(intent)
                        }
                    } else {
                        stopService(intent)
                    }
                    result.success(null)
                }
                "isRunning" -> result.success(MessageWatchService.running)
                "upsertConversationShortcut" -> {
                    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
                        result.success(false)
                    } else {
                        val shortcutId = call.argument<String>("shortcutId")?.trim().orEmpty()
                        val conversationId = call.argument<String>("conversationId")?.trim().orEmpty()
                        val label = call.argument<String>("label")?.trim().orEmpty()
                        val senderName = call.argument<String>("senderName")?.trim().orEmpty()
                        val avatar = call.argument<ByteArray>("avatar")
                        val bitmap = avatar?.let { BitmapFactory.decodeByteArray(it, 0, it.size) }
                        if (
                            shortcutId.isEmpty() ||
                            conversationId.isEmpty() ||
                            label.isEmpty() ||
                            senderName.isEmpty() ||
                            bitmap == null
                        ) {
                            result.success(false)
                        } else {
                            try {
                                val icon = Icon.createWithAdaptiveBitmap(bitmap)
                                val person = Person.Builder()
                                    .setName(senderName)
                                    .setKey(senderName)
                                    .setIcon(icon)
                                    .build()
                                val shortcut = ShortcutInfo.Builder(this, shortcutId)
                                    .setShortLabel(label)
                                    .setLongLabel(label)
                                    .setIcon(icon)
                                    .setIntent(
                                        Intent(this, MainActivity::class.java)
                                            .setAction(Intent.ACTION_VIEW)
                                            .putExtra("conversationId", conversationId),
                                    )
                                    .setLongLived(true)
                                    .setPerson(person)
                                    .setCategories(setOf(ShortcutInfo.SHORTCUT_CATEGORY_CONVERSATION))
                                    .build()
                                getSystemService(ShortcutManager::class.java)
                                    .pushDynamicShortcut(shortcut)
                                result.success(true)
                            } catch (error: Exception) {
                                Log.e("MicroslopNotifications", "Could not publish conversation shortcut", error)
                                result.success(false)
                            }
                        }
                    }
                }
                else -> result.notImplemented()
            }
        }
        MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger,
            MESSAGE_MEDIA_CHANNEL,
        ).setMethodCallHandler { call, result ->
            when (call.method) {
                "captureCameraFrame" -> {
                    val bytes = cameraFrameJpeg()
                    if (bytes == null) {
                        result.error("NO_CAMERA_FRAME", "The camera has not produced a frame yet.", null)
                    } else {
                        result.success(bytes)
                    }
                }
                "startAudioRecording" -> {
                    if (startMessageAudioRecording()) {
                        result.success(null)
                    } else {
                        result.error("AUDIO_RECORDING_FAILED", "Could not start audio recording.", null)
                    }
                }
                "stopAudioRecording" -> {
                    val recording = stopMessageAudioRecording()
                    if (recording == null) {
                        result.error("AUDIO_RECORDING_FAILED", "Could not finish audio recording.", null)
                    } else {
                        result.success(recording)
                    }
                }
                "cancelAudioRecording" -> {
                    cancelMessageAudioRecording()
                    result.success(null)
                }
                else -> result.notImplemented()
            }
        }
        MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger,
            CALL_AUDIO_CHANNEL,
        ).setMethodCallHandler { call, result ->
            when (call.method) {
                "setSpeakerphone" -> {
                    val enabled = call.argument<Boolean>("enabled") == true
                    Thread {
                        val routed = setSpeakerphone(enabled)
                        runOnUiThread {
                            if (routed) {
                                result.success(null)
                            } else {
                                result.error(
                                    "CALL_AUDIO_ROUTE_UNAVAILABLE",
                                    "Could not route call audio to ${if (enabled) "speaker" else "earpiece"}.",
                                    null,
                                )
                            }
                        }
                    }.start()
                }
                "startRingback" -> {
                    startRingback()
                    result.success(null)
                }
                "stopRingback" -> {
                    stopRingback()
                    result.success(null)
                }
                "reset" -> {
                    stopRingback()
                    resetCallAudio()
                    result.success(null)
                }
                else -> result.notImplemented()
            }
        }
        flutterEngine.platformViewsController.registry.registerViewFactory(
            CALL_VIDEO_PREVIEW_VIEW,
            object : PlatformViewFactory(StandardMessageCodec.INSTANCE) {
                override fun create(context: Context, viewId: Int, args: Any?): PlatformView {
                    val view = TextureView(context)
                    var disposed = false
                    view.surfaceTextureListener = object : TextureView.SurfaceTextureListener {
                        override fun onSurfaceTextureAvailable(
                            surfaceTexture: SurfaceTexture,
                            width: Int,
                            height: Int,
                        ) {
                            if (!disposed) attachCameraPreview(surfaceTexture)
                        }

                        override fun onSurfaceTextureSizeChanged(
                            surfaceTexture: SurfaceTexture,
                            width: Int,
                            height: Int,
                        ) = Unit

                        override fun onSurfaceTextureDestroyed(surfaceTexture: SurfaceTexture): Boolean {
                            detachCameraPreview(surfaceTexture)
                            return true
                        }

                        override fun onSurfaceTextureUpdated(surfaceTexture: SurfaceTexture) = Unit
                    }
                    return object : PlatformView {
                        override fun getView(): View = view

                        override fun dispose() {
                            disposed = true
                            view.surfaceTextureListener = null
                            view.surfaceTexture?.let(::detachCameraPreview)
                        }
                    }
                }
            },
        )
        flutterEngine.platformViewsController.registry.registerViewFactory(
            CALL_REMOTE_VIDEO_VIEW,
            object : PlatformViewFactory(StandardMessageCodec.INSTANCE) {
                override fun create(context: Context, viewId: Int, args: Any?): PlatformView {
                    val view = ImageView(context).apply {
                        scaleType = ImageView.ScaleType.CENTER_CROP
                        setBackgroundColor(Color.BLACK)
                    }
                    var disposed = false
                    val poll = object : Runnable {
                        override fun run() {
                            if (disposed) return
                            val packed = nativeRemoteVideoFrame()
                            if (packed != null && packed.size > 8) {
                                val width = ((packed[0].toInt() and 0xff) shl 24) or
                                    ((packed[1].toInt() and 0xff) shl 16) or
                                    ((packed[2].toInt() and 0xff) shl 8) or
                                    (packed[3].toInt() and 0xff)
                                val height = ((packed[4].toInt() and 0xff) shl 24) or
                                    ((packed[5].toInt() and 0xff) shl 16) or
                                    ((packed[6].toInt() and 0xff) shl 8) or
                                    (packed[7].toInt() and 0xff)
                                val frame = packed.copyOfRange(8, packed.size)
                                val ySize = width * height
                                val chromaSize = ySize / 4
                                if (width > 0 && height > 0 && frame.size >= ySize + chromaSize * 2) {
                                    val nv21 = ByteArray(ySize + chromaSize * 2)
                                    System.arraycopy(frame, 0, nv21, 0, ySize)
                                    val uOffset = ySize
                                    val vOffset = ySize + chromaSize
                                    repeat(chromaSize) { index ->
                                        nv21[ySize + index * 2] = frame[vOffset + index]
                                        nv21[ySize + index * 2 + 1] = frame[uOffset + index]
                                    }
                                    val output = ByteArrayOutputStream()
                                    if (YuvImage(nv21, ImageFormat.NV21, width, height, null).compressToJpeg(
                                            Rect(0, 0, width, height),
                                            85,
                                            output,
                                        )) {
                                        val jpeg = output.toByteArray()
                                        view.setImageBitmap(BitmapFactory.decodeByteArray(jpeg, 0, jpeg.size))
                                    }
                                }
                            }
                            view.postDelayed(this, 100)
                        }
                    }
                    view.post(poll)
                    return object : PlatformView {
                        override fun getView(): View = view

                        override fun dispose() {
                            disposed = true
                            view.removeCallbacks(poll)
                            view.setImageDrawable(null)
                        }
                    }
                }
            },
        )
        MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger,
            CALL_VIDEO_CHANNEL,
        ).setMethodCallHandler { call, result ->
            when (call.method) {
                "startCamera" -> startCamera { started ->
                    runOnUiThread { result.success(started) }
                }
                "stopCamera" -> {
                    stopCamera()
                    result.success(null)
                }
                "cameraFramesCaptured" -> result.success(cameraFramesCaptured)
                "nativeCameraFramesReceived" -> result.success(nativeCameraFramesReceived())
                "cameraEncoderVerified" -> {
                    val frame = latestCameraFrame
                    result.success(
                        frame != null && probeNativeCameraFrameEncoding(
                            latestCameraWidth,
                            latestCameraHeight,
                            frame,
                        ),
                    )
                }
                else -> result.notImplemented()
            }
        }
    }

    private fun sharedContent(intent: Intent?): Map<String, Any>? {
        if (intent?.action != Intent.ACTION_SEND) return null
        val text = intent.getStringExtra(Intent.EXTRA_TEXT)?.takeIf { it.isNotBlank() }
        val uri = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableExtra(Intent.EXTRA_STREAM) as? Uri
        }
        val bytes = uri?.let {
            runCatching { contentResolver.openInputStream(it)?.use { stream -> stream.readBytes() } }
                .getOrNull()
        }?.takeIf { it.isNotEmpty() }
        if (text == null && bytes == null) return null
        val content = mutableMapOf<String, Any>()
        if (text != null) content["text"] = text
        if (bytes != null) {
            content["data"] = bytes
            content["contentType"] = intent.type
                ?.takeIf { it.startsWith("image/") }
                ?: uri?.let(contentResolver::getType)
                ?: "image/jpeg"
        }
        return content
    }

    private fun startCamera(onReady: (Boolean) -> Unit) {
        stopCamera()
        val completed = AtomicBoolean(false)
        val finish = { started: Boolean ->
            if (completed.compareAndSet(false, true)) onReady(started)
        }
        try {
            val manager = getSystemService(Context.CAMERA_SERVICE) as CameraManager
            val cameraId = manager.cameraIdList.firstOrNull {
                manager.getCameraCharacteristics(it)
                    .get(CameraCharacteristics.LENS_FACING) == CameraCharacteristics.LENS_FACING_FRONT
            } ?: manager.cameraIdList.firstOrNull()
            if (cameraId == null) {
                finish(false)
                return
            }
            val characteristics = manager.getCameraCharacteristics(cameraId)
            val sizes = characteristics.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP)
                ?.getOutputSizes(ImageFormat.YUV_420_888)
                ?.toList()
                .orEmpty()
            val size = sizes.minByOrNull {
                kotlin.math.abs(it.width * it.height - 320 * 240)
            }
            if (size == null) {
                finish(false)
                return
            }
            val thread = HandlerThread("MicroslopCamera").also { it.start() }
            val handler = Handler(thread.looper)
            cameraThread = thread
            cameraFramesCaptured = 0
            latestCameraFrame = null
            latestCameraWidth = size.width
            latestCameraHeight = size.height
            cameraPreviewTexture?.setDefaultBufferSize(size.width, size.height)
            imageReader = ImageReader.newInstance(
                size.width,
                size.height,
                ImageFormat.YUV_420_888,
                2,
            ).also { reader ->
                reader.setOnImageAvailableListener({ source ->
                    val image = source.acquireLatestImage() ?: return@setOnImageAvailableListener
                    try {
                        val frame = imageToI420(image)
                        latestCameraFrame = frame
                        latestCameraWidth = image.width
                        latestCameraHeight = image.height
                        pushNativeCameraFrame(image.width, image.height, frame)
                        cameraFramesCaptured++
                    } finally {
                        image.close()
                    }
                }, handler)
            }
            manager.openCamera(cameraId, object : CameraDevice.StateCallback() {
                override fun onOpened(device: CameraDevice) {
                    cameraDevice = device
                    configureCameraSession(device, handler, finish)
                }

                override fun onDisconnected(device: CameraDevice) {
                    device.close()
                    cameraDevice = null
                    finish(false)
                }

                override fun onError(device: CameraDevice, error: Int) {
                    Log.e(CALL_VIDEO_TAG, "Camera error=$error")
                    device.close()
                    cameraDevice = null
                    finish(false)
                }
            }, handler)
        } catch (error: Exception) {
            Log.e(CALL_VIDEO_TAG, "Could not start camera", error)
            stopCamera()
            finish(false)
        }
    }

    private fun attachCameraPreview(surfaceTexture: SurfaceTexture) {
        cameraPreviewSurface?.release()
        cameraPreviewTexture = surfaceTexture
        if (latestCameraWidth > 0 && latestCameraHeight > 0) {
            surfaceTexture.setDefaultBufferSize(latestCameraWidth, latestCameraHeight)
        }
        cameraPreviewSurface = Surface(surfaceTexture)
        reconfigureCameraSession()
    }

    private fun detachCameraPreview(surfaceTexture: SurfaceTexture) {
        if (cameraPreviewTexture !== surfaceTexture) return
        cameraPreviewTexture = null
        cameraPreviewSurface?.release()
        cameraPreviewSurface = null
        reconfigureCameraSession()
    }

    private fun reconfigureCameraSession() {
        val device = cameraDevice ?: return
        val thread = cameraThread ?: return
        val handler = Handler(thread.looper)
        handler.post { configureCameraSession(device, handler) }
    }

    private fun configureCameraSession(
        device: CameraDevice,
        handler: Handler,
        onReady: ((Boolean) -> Unit)? = null,
    ) {
        val captureSurface = imageReader?.surface ?: return
        val surfaces = mutableListOf(captureSurface)
        cameraPreviewSurface?.takeIf { it.isValid }?.let(surfaces::add)
        cameraSession?.close()
        device.createCaptureSession(
            surfaces,
            object : CameraCaptureSession.StateCallback() {
                override fun onConfigured(session: CameraCaptureSession) {
                    if (cameraDevice !== device) {
                        session.close()
                        return
                    }
                    cameraSession = session
                    val request = device.createCaptureRequest(CameraDevice.TEMPLATE_RECORD).apply {
                        surfaces.forEach(::addTarget)
                    }.build()
                    session.setRepeatingRequest(request, null, handler)
                    onReady?.invoke(true)
                }

                override fun onConfigureFailed(session: CameraCaptureSession) {
                    Log.e(CALL_VIDEO_TAG, "Camera session configuration failed")
                    onReady?.invoke(false)
                }
            },
            handler,
        )
    }

    private fun cameraFrameJpeg(): ByteArray? {
        val frame = latestCameraFrame ?: return null
        val width = latestCameraWidth
        val height = latestCameraHeight
        if (width <= 0 || height <= 0) return null
        val ySize = width * height
        val chromaSize = ySize / 4
        if (frame.size < ySize + chromaSize * 2) return null
        val nv21 = ByteArray(ySize + chromaSize * 2)
        System.arraycopy(frame, 0, nv21, 0, ySize)
        val uOffset = ySize
        val vOffset = ySize + chromaSize
        repeat(chromaSize) { index ->
            nv21[ySize + index * 2] = frame[vOffset + index]
            nv21[ySize + index * 2 + 1] = frame[uOffset + index]
        }
        val output = ByteArrayOutputStream()
        if (!YuvImage(nv21, ImageFormat.NV21, width, height, null).compressToJpeg(
                Rect(0, 0, width, height),
                92,
                output,
            )) {
            return null
        }
        return output.toByteArray()
    }

    private fun startMessageAudioRecording(): Boolean {
        cancelMessageAudioRecording()
        val file = File(cacheDir, "voice-${System.currentTimeMillis()}.m4a")
        val recorder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            MediaRecorder(this)
        } else {
            @Suppress("DEPRECATION")
            MediaRecorder()
        }
        return try {
            recorder.setAudioSource(MediaRecorder.AudioSource.MIC)
            recorder.setOutputFormat(MediaRecorder.OutputFormat.MPEG_4)
            recorder.setAudioEncoder(MediaRecorder.AudioEncoder.AAC)
            recorder.setAudioEncodingBitRate(128_000)
            recorder.setAudioSamplingRate(44_100)
            recorder.setOutputFile(file.absolutePath)
            recorder.prepare()
            recorder.start()
            messageRecorder = recorder
            messageRecordingFile = file
            true
        } catch (error: Exception) {
            Log.e(CALL_AUDIO_TAG, "Could not start message audio recording", error)
            recorder.release()
            file.delete()
            false
        }
    }

    private fun stopMessageAudioRecording(): Map<String, Any>? {
        val recorder = messageRecorder ?: return null
        val file = messageRecordingFile ?: return null
        messageRecorder = null
        messageRecordingFile = null
        return try {
            recorder.stop()
            recorder.release()
            val bytes = file.readBytes()
            file.delete()
            if (bytes.isEmpty()) null else mapOf(
                "data" to bytes,
                "name" to file.name,
                "contentType" to "audio/mp4",
            )
        } catch (error: Exception) {
            Log.e(CALL_AUDIO_TAG, "Could not finish message audio recording", error)
            recorder.release()
            file.delete()
            null
        }
    }

    private fun cancelMessageAudioRecording() {
        val recorder = messageRecorder
        messageRecorder = null
        messageRecordingFile?.delete()
        messageRecordingFile = null
        if (recorder != null) {
            try {
                recorder.stop()
            } catch (_: Exception) {
            }
            recorder.release()
        }
    }

    private fun imageToI420(image: Image): ByteArray {
        val width = image.width
        val height = image.height
        val output = ByteArray(width * height * 3 / 2)
        copyPlane(image.planes[0], width, height, output, 0)
        val chromaSize = width * height / 4
        copyPlane(image.planes[1], width / 2, height / 2, output, width * height)
        copyPlane(image.planes[2], width / 2, height / 2, output, width * height + chromaSize)
        return output
    }

    private fun copyPlane(
        plane: Image.Plane,
        width: Int,
        height: Int,
        output: ByteArray,
        outputOffset: Int,
    ) {
        val buffer = plane.buffer
        var target = outputOffset
        repeat(height) { row ->
            repeat(width) { column ->
                output[target++] = buffer.get(row * plane.rowStride + column * plane.pixelStride)
            }
        }
    }

    private fun stopCamera() {
        val session = cameraSession
        val device = cameraDevice
        val reader = imageReader
        val thread = cameraThread
        cameraSession = null
        cameraDevice = null
        imageReader = null
        cameraThread = null
        session?.close()
        device?.close()
        reader?.close()
        thread?.quitSafely()
    }

    private fun setSpeakerphone(enabled: Boolean): Boolean {
        val desiredType = if (enabled) {
            AudioDeviceInfo.TYPE_BUILTIN_SPEAKER
        } else {
            AudioDeviceInfo.TYPE_BUILTIN_EARPIECE
        }
        val device = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            audioManager.availableCommunicationDevices.firstOrNull { it.type == desiredType }
        } else {
            audioManager.getDevices(AudioManager.GET_DEVICES_OUTPUTS)
                .firstOrNull { it.type == desiredType }
        }
        if (device == null) {
            Log.w(CALL_AUDIO_TAG, "No communication device for enabled=$enabled type=$desiredType")
            return false
        }

        if (!pauseNativeCallAudio()) {
            Log.w(CALL_AUDIO_TAG, "Could not pause native call audio for route change")
            return false
        }
        var routeApplied = false
        var resumed = false
        try {
            audioManager.mode = AudioManager.MODE_IN_COMMUNICATION
            setNativeCallAudioDevice(device.id)
            routeApplied = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                audioManager.setCommunicationDevice(device)
            } else {
                @Suppress("DEPRECATION")
                run {
                    audioManager.isSpeakerphoneOn = enabled
                }
                true
            }
            if (!routeApplied) {
                setNativeCallAudioDevice(0)
            }
            Log.i(
                CALL_AUDIO_TAG,
                "route enabled=$enabled id=${device.id} type=${device.type} name=${device.productName} applied=$routeApplied",
            )
        } finally {
            resumed = resumeNativeCallAudio()
            val active = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                audioManager.communicationDevice
            } else {
                null
            }
            Log.i(
                CALL_AUDIO_TAG,
                "route resumed=$resumed activeId=${active?.id} activeType=${active?.type} activeName=${active?.productName}",
            )
        }
        if (!routeApplied || !resumed) {
            Log.w(CALL_AUDIO_TAG, "Call audio route change did not complete successfully")
            return false
        }
        return true
    }

    private fun startRingback() {
        stopRingback()
        ringback = ToneGenerator(AudioManager.STREAM_MUSIC, 90).also {
            it.startTone(ToneGenerator.TONE_SUP_RINGTONE)
        }
        Log.i(CALL_AUDIO_TAG, "ringback started")
    }

    private fun stopRingback() {
        ringback?.stopTone()
        ringback?.release()
        ringback = null
        Log.i(CALL_AUDIO_TAG, "ringback stopped")
    }

    private fun resetCallAudio() {
        setNativeCallAudioDevice(0)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            audioManager.clearCommunicationDevice()
        } else {
            @Suppress("DEPRECATION")
            run {
                audioManager.isSpeakerphoneOn = false
            }
        }
        audioManager.mode = AudioManager.MODE_NORMAL
    }

    override fun onDestroy() {
        stopRingback()
        stopCamera()
        cancelMessageAudioRecording()
        resetCallAudio()
        shareTargetChannel = null
        super.onDestroy()
    }
}
