import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import 'src/rust/api/calls.dart' as rust;

const microsoftTestCallConversationId = '__microslop_test_call__';
final _linuxVideoDevicePattern = RegExp(r'/video\d+$');

enum MediaDeviceKind { microphone, speaker, camera }

class MediaDevice {
  const MediaDevice(this.id, this.label);

  final String id;
  final String label;
}

abstract interface class MediaDeviceCallGateway {
  Future<List<MediaDevice>> mediaDevices(MediaDeviceKind kind);
  Future<void> selectMediaDevice(MediaDeviceKind kind, String id);
  Future<int> previewMicrophone();
  Future<bool> previewSpeaker();
  Future<CallVideoFrame?> previewCamera();
}

class CallVideoFrame {
  const CallVideoFrame(this.width, this.height, this.rgba);

  final int width;
  final int height;
  final Uint8List rgba;
}

class PlatformCallVideo {
  PlatformCallVideo._();

  static const _channel = MethodChannel('microslop/call_video');
  @visibleForTesting
  static bool? debugSupportedOverride;

  static bool get supported =>
      debugSupportedOverride ?? (!kIsWeb && Platform.isAndroid);
  static bool get remoteSupported =>
      !kIsWeb && (Platform.isAndroid || Platform.isWindows);

  static Future<bool> startCamera() async {
    if (!supported) return true;
    return await _channel.invokeMethod<bool>('startCamera') ?? false;
  }

  static Future<void> stopCamera() async {
    if (!supported) return;
    await _channel.invokeMethod<void>('stopCamera');
  }

  static Future<int> cameraFramesCaptured() async {
    if (!supported) return 0;
    return await _channel.invokeMethod<int>('cameraFramesCaptured') ?? 0;
  }

  static Future<int> nativeCameraFramesReceived() async {
    if (!supported) return 0;
    return await _channel.invokeMethod<int>('nativeCameraFramesReceived') ?? 0;
  }

  static Future<bool> cameraEncoderVerified() async {
    if (!supported) return false;
    return await _channel.invokeMethod<bool>('cameraEncoderVerified') ?? false;
  }

  static Future<CallVideoFrame?> remoteFrame() async {
    if (kIsWeb || !Platform.isWindows) return null;
    final frame = await rust.remoteVideoFrame();
    if (frame == null) return null;
    return CallVideoFrame(frame.width, frame.height, frame.rgba);
  }

  static Future<CallVideoFrame?> localFrame() async {
    if (kIsWeb || !Platform.isWindows) return null;
    final frame = await rust.localVideoFrame();
    if (frame == null) return null;
    return CallVideoFrame(frame.width, frame.height, frame.rgba);
  }
}

class PlatformCallAudio {
  PlatformCallAudio._();

  static const _channel = MethodChannel('microslop/call_audio');

  static bool get supportsSpeakerRouting => !kIsWeb && Platform.isAndroid;
  static bool get _supportsDesktopAudio => !kIsWeb && Platform.isWindows;

  static Future<void> setSpeakerphone(bool enabled) async {
    if (!supportsSpeakerRouting) return;
    await _channel.invokeMethod<void>('setSpeakerphone', {'enabled': enabled});
  }

  static Future<void> startRingback() async {
    if (supportsSpeakerRouting) {
      await _channel.invokeMethod<void>('startRingback');
    } else if (_supportsDesktopAudio) {
      await rust.startCall(conversationId: '__microslop_call_ringback_on__');
    }
  }

  static Future<void> stopRingback() async {
    if (supportsSpeakerRouting) {
      await _channel.invokeMethod<void>('stopRingback');
    } else if (_supportsDesktopAudio) {
      await rust.startCall(conversationId: '__microslop_call_ringback_off__');
    }
  }

  static Future<void> reset() async {
    if (supportsSpeakerRouting) {
      await _channel.invokeMethod<void>('reset');
    } else if (_supportsDesktopAudio) {
      await rust.startCall(conversationId: '__microslop_call_ringback_off__');
    }
  }
}

enum CallUpdateKind { incoming, dialing, ringing, connected, ended, error }

class TestCallResult {
  const TestCallResult({
    required this.callPlaced,
    required this.callAccepted,
    required this.packetsSent,
    required this.packetsReceived,
    required this.microphoneFrames,
    required this.microphoneNonSilentFrames,
    required this.microphonePeak,
    required this.speakerFramesReceived,
    required this.speakerSamplesRendered,
    required this.speakerStreamErrors,
    required this.videoPacketsSent,
    required this.videoPacketsReceived,
    required this.cameraFramesSent,
    required this.echoDetected,
    required this.echoDelayMs,
    required this.echoCorrelation,
    this.rejectionReason,
  });

  final bool callPlaced;
  final bool callAccepted;
  final int packetsSent;
  final int packetsReceived;
  final int microphoneFrames;
  final int microphoneNonSilentFrames;
  final int microphonePeak;
  final int speakerFramesReceived;
  final int speakerSamplesRendered;
  final int speakerStreamErrors;
  final int videoPacketsSent;
  final int videoPacketsReceived;
  final int cameraFramesSent;
  final bool echoDetected;
  final double echoDelayMs;
  final double echoCorrelation;
  final String? rejectionReason;

  bool get passed =>
      callPlaced &&
      callAccepted &&
      packetsSent > 0 &&
      packetsReceived > 0 &&
      microphoneFrames > 0 &&
      microphonePeak > 0 &&
      speakerFramesReceived > 0 &&
      speakerSamplesRendered > 0 &&
      speakerStreamErrors == 0;

  bool get cameraPassed => cameraFramesSent > 0 && videoPacketsSent > 0;
}

class CallUpdate {
  const CallUpdate({
    required this.kind,
    required this.callId,
    this.conversationId,
    this.displayName,
    this.detail,
  });

  final CallUpdateKind kind;
  final String callId;
  final String? conversationId;
  final String? displayName;
  final String? detail;
}

abstract interface class CallGateway {
  Stream<CallUpdate> events();
  Future<void> startCall(
    String conversationId, {
    bool video = false,
    String? calleeUserId,
  });
  Future<void> hangUp();
  Future<void> acceptCall(String callId);
  Future<void> declineCall(String callId);
  Future<void> setMicrophoneEnabled(bool enabled);
  Future<void> setSpeakerEnabled(bool enabled);
  Future<void> stopEvents();
}

class NoopCallGateway implements CallGateway {
  const NoopCallGateway();

  @override
  Stream<CallUpdate> events() => const Stream.empty();

  @override
  Future<void> startCall(
    String conversationId, {
    bool video = false,
    String? calleeUserId,
  }) async {}

  @override
  Future<void> hangUp() async {}

  @override
  Future<void> acceptCall(String callId) async {}

  @override
  Future<void> declineCall(String callId) async {}

  @override
  Future<void> setMicrophoneEnabled(bool enabled) async {}

  @override
  Future<void> setSpeakerEnabled(bool enabled) async {}

  @override
  Future<void> stopEvents() async {}
}

TestCallResult parseMicrosoftTestCallResult(String? detail) {
  final values = <String, String>{};
  for (final field in (detail ?? '').split(';').skip(1)) {
    final separator = field.indexOf('=');
    if (separator > 0) {
      values[field.substring(0, separator)] = field.substring(separator + 1);
    }
  }
  final accepted = values['accepted'] == 'true';
  final sent = int.tryParse(values['sent'] ?? '') ?? 0;
  final received = int.tryParse(values['received'] ?? '') ?? 0;
  return TestCallResult(
    callPlaced: true,
    callAccepted: accepted,
    packetsSent: sent,
    packetsReceived: received,
    microphoneFrames: int.tryParse(values['mic_frames'] ?? '') ?? 0,
    microphoneNonSilentFrames:
        int.tryParse(values['mic_non_silent'] ?? '') ?? 0,
    microphonePeak: int.tryParse(values['mic_peak'] ?? '') ?? 0,
    speakerFramesReceived: int.tryParse(values['speaker_frames'] ?? '') ?? 0,
    speakerSamplesRendered: int.tryParse(values['speaker_samples'] ?? '') ?? 0,
    speakerStreamErrors: int.tryParse(values['speaker_errors'] ?? '') ?? 0,
    videoPacketsSent: int.tryParse(values['video_sent'] ?? '') ?? 0,
    videoPacketsReceived: int.tryParse(values['video_received'] ?? '') ?? 0,
    cameraFramesSent: int.tryParse(values['camera_frames'] ?? '') ?? 0,
    echoDetected: values['echo'] == 'true',
    echoDelayMs: double.tryParse(values['delay_ms'] ?? '') ?? 0,
    echoCorrelation: double.tryParse(values['correlation'] ?? '') ?? 0,
    rejectionReason: (values['reason']?.isNotEmpty ?? false)
        ? values['reason']
        : null,
  );
}

class RustCallGateway implements CallGateway, MediaDeviceCallGateway {
  @override
  Future<List<MediaDevice>> mediaDevices(MediaDeviceKind kind) async {
    if (Platform.isWindows) {
      return [
        for (final device in await rust.mediaDevices(kind: kind.name))
          MediaDevice(device.id, device.label),
      ];
    }
    if (!Platform.isLinux) return const [MediaDevice('', 'System default')];
    if (kind != MediaDeviceKind.camera) {
      try {
        final result = await Process.run(
          kind == MediaDeviceKind.microphone ? 'arecord' : 'aplay',
          const ['-L'],
        );
        final names = (result.stdout as String)
            .split('\n')
            .where((line) => line.isNotEmpty && !line.startsWith(' '));
        return [
          const MediaDevice('', 'System default'),
          for (final name in names) MediaDevice(name, name),
        ];
      } on ProcessException {
        return const [MediaDevice('', 'System default')];
      }
    }
    final paths = Directory('/dev').listSync().whereType<File>().where(
      (file) => _linuxVideoDevicePattern.hasMatch(file.path),
    );
    return [
      const MediaDevice('', 'System default'),
      for (final path in paths)
        MediaDevice(path.path, path.path.split('/').last),
    ];
  }

  @override
  Future<void> selectMediaDevice(
    MediaDeviceKind kind,
    String id,
  ) => rust.startCall(
    conversationId:
        '__microslop_device_${kind.name}_${base64Url.encode(utf8.encode(id))}',
  );

  @override
  Future<int> previewMicrophone() => rust.previewMicrophonePeak();

  @override
  Future<bool> previewSpeaker() => rust.previewSpeaker();

  @override
  Future<CallVideoFrame?> previewCamera() async {
    final frame = await rust.previewCameraFrame();
    if (frame == null) return null;
    return CallVideoFrame(frame.width, frame.height, frame.rgba);
  }

  @override
  Stream<CallUpdate> events() => rust.listenCallEvents().map(
    (event) => CallUpdate(
      kind: switch (event.kind) {
        'incoming' => CallUpdateKind.incoming,
        'dialing' => CallUpdateKind.dialing,
        'ringing' => CallUpdateKind.ringing,
        'connected' => CallUpdateKind.connected,
        'ended' => CallUpdateKind.ended,
        _ => CallUpdateKind.error,
      },
      callId: event.callId,
      conversationId: event.conversationId,
      displayName: event.displayName,
      detail: event.detail,
    ),
  );

  @override
  Future<void> startCall(
    String conversationId, {
    bool video = false,
    String? calleeUserId,
  }) {
    var target = conversationId;
    if (calleeUserId != null && calleeUserId.trim().isNotEmpty) {
      target =
          '__microslop_callee_${base64Url.encode(utf8.encode(calleeUserId.trim())).replaceAll('=', '')}:$target';
    }
    if (video) target = '__microslop_video_call__$target';
    return rust.startCall(conversationId: target);
  }

  @override
  Future<void> hangUp() => rust.hangUp();

  @override
  Future<void> acceptCall(String callId) => rust.acceptCall(callId: callId);

  @override
  Future<void> declineCall(String callId) => rust.declineCall(callId: callId);

  @override
  Future<void> setMicrophoneEnabled(bool enabled) => rust.startCall(
    conversationId: enabled
        ? '__microslop_call_mic_on__'
        : '__microslop_call_mic_off__',
  );

  @override
  Future<void> setSpeakerEnabled(bool enabled) => rust.startCall(
    conversationId: enabled
        ? '__microslop_call_speaker_on__'
        : '__microslop_call_speaker_off__',
  );

  @override
  Future<void> stopEvents() => rust.stopCallEvents();
}
