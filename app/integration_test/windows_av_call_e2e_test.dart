import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:microslop/call_gateway.dart';
import 'package:microslop/platform_backend.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'Windows headset and live virtual camera reach Microsoft test call',
    (tester) async {
      expect(Platform.isWindows, isTrue);
      await initializePlatformBackend();

      final auth = createPlatformAuthGateway();
      final session = await auth.restoreWorkSession();
      expect(
        session.signedIn,
        isTrue,
        reason: 'Cached Windows work session required',
      );

      final calls = createPlatformCallGateway();
      expect(calls, isA<MediaDeviceCallGateway>());
      final media = calls as MediaDeviceCallGateway;
      final microphones = await media.mediaDevices(MediaDeviceKind.microphone);
      final speakers = await media.mediaDevices(MediaDeviceKind.speaker);
      final cameras = await media.mediaDevices(MediaDeviceKind.camera);
      final microphone = microphones.firstWhere(
        (device) => device.label.contains('USB Audio Device'),
        orElse: () => const MediaDevice('', 'System default'),
      );
      final speaker = speakers.firstWhere(
        (device) => device.label.contains('USB Audio Device'),
        orElse: () => const MediaDevice('', 'System default'),
      );
      final camera = cameras.firstWhere(
        (device) => device.label.toLowerCase().contains('obs'),
        orElse: () => const MediaDevice('', 'System default'),
      );
      debugPrint('MICROSLOP_WINDOWS_AV_MIC=${microphone.label}');
      debugPrint('MICROSLOP_WINDOWS_AV_SPEAKER=${speaker.label}');
      debugPrint('MICROSLOP_WINDOWS_AV_CAMERA=${camera.label}');
      expect(microphone.id, isNotEmpty, reason: microphones.toString());
      expect(speaker.id, isNotEmpty, reason: speakers.toString());
      expect(camera.id, isNotEmpty, reason: cameras.toString());
      await media.selectMediaDevice(MediaDeviceKind.microphone, microphone.id);
      await media.selectMediaDevice(MediaDeviceKind.speaker, speaker.id);
      await media.selectMediaDevice(MediaDeviceKind.camera, camera.id);

      final previewMicrophonePeak = await media.previewMicrophone();
      debugPrint(
        'MICROSLOP_WINDOWS_AV_MIC_PREVIEW_PEAK=$previewMicrophonePeak',
      );
      expect(await media.previewSpeaker(), isTrue);
      final previewOne = await media.previewCamera();
      expect(previewOne, isNotNull);
      await Future<void>.delayed(const Duration(milliseconds: 250));
      final previewTwo = await media.previewCamera();
      expect(previewTwo, isNotNull);
      expect(
        listEquals(previewOne!.rgba, previewTwo!.rgba),
        isFalse,
        reason: 'Windows camera preview is static rather than live',
      );

      final connected = Completer<CallUpdate>();
      final finished = Completer<CallUpdate>();
      final subscription = calls.events().listen((event) {
        if (event.conversationId != microsoftTestCallConversationId) return;
        if (event.kind == CallUpdateKind.connected && !connected.isCompleted) {
          connected.complete(event);
        }
        if ((event.kind == CallUpdateKind.ended ||
                event.kind == CallUpdateKind.error) &&
            !finished.isCompleted) {
          finished.complete(event);
        }
      });

      try {
        await calls.startCall(microsoftTestCallConversationId, video: true);
        final firstOutcome = await Future.any([
          connected.future,
          finished.future,
        ]).timeout(const Duration(seconds: 90));
        expect(
          firstOutcome.kind,
          CallUpdateKind.connected,
          reason:
              firstOutcome.detail ?? 'Test call ended before media connected',
        );

        await Future.any([
          Future<void>.delayed(const Duration(seconds: 20)),
          finished.future.then((_) {}),
        ]);
        if (!finished.isCompleted) await calls.hangUp();

        final terminal = await finished.future.timeout(
          const Duration(seconds: 45),
        );
        debugPrint('MICROSLOP_WINDOWS_AV_RESULT=${terminal.detail}');
        expect(
          terminal.kind,
          CallUpdateKind.ended,
          reason: terminal.detail ?? 'Microsoft test call failed',
        );
        final result = parseMicrosoftTestCallResult(terminal.detail);
        expect(result.passed, isTrue, reason: terminal.detail);
        expect(result.cameraPassed, isTrue, reason: terminal.detail);
        expect(
          result.microphoneFrames,
          greaterThan(0),
          reason: terminal.detail,
        );
        expect(result.microphonePeak, greaterThan(0), reason: terminal.detail);
        expect(
          result.cameraFramesSent,
          greaterThan(0),
          reason: terminal.detail,
        );
        expect(result.videoPacketsSent, greaterThan(0));
      } finally {
        await calls.hangUp();
        await calls.stopEvents();
        await subscription.cancel();
      }
    },
  );
}
