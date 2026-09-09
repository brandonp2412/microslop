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
    'Windows synthetic camera stays live during Microsoft test call',
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
      final media = calls as MediaDeviceCallGateway;
      final cameras = await media.mediaDevices(MediaDeviceKind.camera);
      final camera = cameras.firstWhere(
        (device) => device.label == 'Microslop Synthetic Camera',
      );
      await media.selectMediaDevice(MediaDeviceKind.camera, camera.id);

      final microphones = await media.mediaDevices(MediaDeviceKind.microphone);
      final speakers = await media.mediaDevices(MediaDeviceKind.speaker);
      final microphone = microphones.firstWhere(
        (device) => device.label.contains('USB Audio Device'),
        orElse: () => const MediaDevice('', 'System default'),
      );
      final speaker = speakers.firstWhere(
        (device) => device.label.contains('USB Audio Device'),
        orElse: () => const MediaDevice('', 'System default'),
      );
      await media.selectMediaDevice(MediaDeviceKind.microphone, microphone.id);
      await media.selectMediaDevice(MediaDeviceKind.speaker, speaker.id);

      int checksum(CallVideoFrame frame) => frame.rgba.fold<int>(
        0,
        (sum, value) => (sum * 31 + value) & 0x7fffffff,
      );

      final first = await media.previewCamera();
      expect(first, isNotNull);
      await Future<void>.delayed(const Duration(milliseconds: 700));
      final second = await media.previewCamera();
      expect(second, isNotNull);
      final firstChecksum = checksum(first!);
      final secondChecksum = checksum(second!);
      debugPrint('MICROSLOP_VIDEO_PREVIEW_FRAME1=$firstChecksum');
      debugPrint('MICROSLOP_VIDEO_PREVIEW_FRAME2=$secondChecksum');
      expect(secondChecksum, isNot(firstChecksum));

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
        await calls.startCall(microsoftTestCallConversationId);
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

        final duringCallFirst = await media.previewCamera();
        expect(duringCallFirst, isNotNull);
        await Future<void>.delayed(const Duration(milliseconds: 700));
        final duringCallSecond = await media.previewCamera();
        expect(duringCallSecond, isNotNull);
        final duringCallFirstChecksum = checksum(duringCallFirst!);
        final duringCallSecondChecksum = checksum(duringCallSecond!);
        debugPrint(
          'MICROSLOP_VIDEO_DURING_CALL_FRAME1=$duringCallFirstChecksum',
        );
        debugPrint(
          'MICROSLOP_VIDEO_DURING_CALL_FRAME2=$duringCallSecondChecksum',
        );
        expect(duringCallSecondChecksum, isNot(duringCallFirstChecksum));

        await Future.any([
          Future<void>.delayed(const Duration(seconds: 20)),
          finished.future.then((_) {}),
        ]);
        if (!finished.isCompleted) await calls.hangUp();

        final terminal = await finished.future.timeout(
          const Duration(seconds: 45),
        );
        debugPrint('MICROSLOP_WINDOWS_CAMERA_TEST_RESULT=${terminal.detail}');
        expect(terminal.kind, CallUpdateKind.ended, reason: terminal.detail);
        final result = parseMicrosoftTestCallResult(terminal.detail);
        expect(result.passed, isTrue, reason: terminal.detail);
        expect(result.videoPacketsSent, 0, reason: terminal.detail);
        expect(result.cameraFramesSent, 0, reason: terminal.detail);
      } finally {
        await calls.hangUp();
        await calls.stopEvents();
        await subscription.cancel();
      }
    },
  );
}
