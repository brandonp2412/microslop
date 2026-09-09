import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:microslop/call_gateway.dart';
import 'package:microslop/platform_backend.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('Windows USB headset completes Microsoft test call audio', (
    tester,
  ) async {
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
    final microphone = microphones.firstWhere(
      (device) => device.label.contains('USB Audio Device'),
      orElse: () => const MediaDevice('', 'System default'),
    );
    final speaker = speakers.firstWhere(
      (device) => device.label.contains('USB Audio Device'),
      orElse: () => const MediaDevice('', 'System default'),
    );
    debugPrint('MICROSLOP_WINDOWS_HEADSET_MIC=${microphone.label}');
    debugPrint('MICROSLOP_WINDOWS_HEADSET_SPEAKER=${speaker.label}');
    await media.selectMediaDevice(MediaDeviceKind.microphone, microphone.id);
    await media.selectMediaDevice(MediaDeviceKind.speaker, speaker.id);
    expect(await media.previewSpeaker(), isTrue);

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
      await calls.startCall(microsoftTestCallConversationId, video: false);
      final firstOutcome = await Future.any([
        connected.future,
        finished.future,
      ]).timeout(const Duration(seconds: 90));
      expect(
        firstOutcome.kind,
        CallUpdateKind.connected,
        reason: firstOutcome.detail ?? 'Test call ended before media connected',
      );

      final terminal = await finished.future.timeout(
        const Duration(seconds: 75),
      );
      debugPrint('MICROSLOP_WINDOWS_HEADSET_RESULT=${terminal.detail}');
      expect(terminal.kind, CallUpdateKind.ended, reason: terminal.detail);
      final result = parseMicrosoftTestCallResult(terminal.detail);
      expect(result.callAccepted, isTrue, reason: terminal.detail);
      expect(result.microphoneFrames, greaterThan(0), reason: terminal.detail);
      expect(result.microphonePeak, greaterThan(0), reason: terminal.detail);
      expect(result.packetsSent, greaterThan(0), reason: terminal.detail);
      expect(result.packetsReceived, greaterThan(0), reason: terminal.detail);
      expect(
        result.speakerFramesReceived,
        greaterThan(0),
        reason: terminal.detail,
      );
      expect(
        result.speakerSamplesRendered,
        greaterThan(0),
        reason: terminal.detail,
      );
      expect(result.speakerStreamErrors, 0, reason: terminal.detail);
      expect(result.passed, isTrue, reason: terminal.detail);
    } finally {
      await calls.hangUp();
      await calls.stopEvents();
      await subscription.cancel();
    }
  });
}
