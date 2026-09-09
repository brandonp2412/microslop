import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:microslop/call_gateway.dart';
import 'package:microslop/platform_backend.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('Windows USB headset speaker leaks into its microphone', (
    tester,
  ) async {
    expect(Platform.isWindows, isTrue);
    await initializePlatformBackend();

    final calls = createPlatformCallGateway();
    expect(calls, isA<MediaDeviceCallGateway>());
    final media = calls as MediaDeviceCallGateway;

    final microphones = await media.mediaDevices(MediaDeviceKind.microphone);
    final speakers = await media.mediaDevices(MediaDeviceKind.speaker);
    final microphone = microphones.firstWhere(
      (device) => device.label.contains('USB Audio Device'),
    );
    final speaker = speakers.firstWhere(
      (device) => device.label.contains('USB Audio Device'),
    );
    await media.selectMediaDevice(MediaDeviceKind.microphone, microphone.id);
    await media.selectMediaDevice(MediaDeviceKind.speaker, speaker.id);

    final baseline = await media.previewMicrophone();
    final coupledMic = media.previewMicrophone();
    await Future<void>.delayed(const Duration(milliseconds: 300));
    final speakerPlayed = await media.previewSpeaker();
    final coupled = await coupledMic;

    debugPrint('MICROSLOP_ACOUSTIC_BASELINE=$baseline');
    debugPrint('MICROSLOP_ACOUSTIC_COUPLED=$coupled');
    debugPrint('MICROSLOP_ACOUSTIC_SPEAKER_PLAYED=$speakerPlayed');

    expect(speakerPlayed, isTrue);
    expect(coupled, greaterThan(baseline + 25));
  });
}
