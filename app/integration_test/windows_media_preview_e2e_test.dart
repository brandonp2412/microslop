import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:microslop/call_gateway.dart';
import 'package:microslop/platform_backend.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('Windows selected media devices provide live previews', (
    tester,
  ) async {
    expect(Platform.isWindows, isTrue);
    await initializePlatformBackend();

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
      (device) =>
          device.label == 'Microslop Synthetic Camera' ||
          device.label.toLowerCase().contains('obs'),
      orElse: () => const MediaDevice('', 'System default'),
    );
    expect(microphone.id, isNotEmpty, reason: microphones.toString());
    expect(speaker.id, isNotEmpty, reason: speakers.toString());
    expect(camera.id, isNotEmpty, reason: cameras.toString());

    await media.selectMediaDevice(MediaDeviceKind.microphone, microphone.id);
    await media.selectMediaDevice(MediaDeviceKind.speaker, speaker.id);
    await media.selectMediaDevice(MediaDeviceKind.camera, camera.id);

    final microphonePeak = await media.previewMicrophone();
    debugPrint('MICROSLOP_WINDOWS_PREVIEW_MIC_PEAK=$microphonePeak');
    expect(await media.previewSpeaker(), isTrue);

    final frames = <CallVideoFrame>[];
    for (var index = 0; index < 20; index++) {
      final frame = await media.previewCamera();
      expect(frame, isNotNull);
      expect(frame!.width, greaterThan(0));
      expect(frame.height, greaterThan(0));
      expect(frame.rgba.length, frame.width * frame.height * 4);
      frames.add(frame);
      await Future<void>.delayed(const Duration(milliseconds: 40));
    }
    expect(
      List.generate(
        frames.length - 1,
        (index) => !listEquals(frames[index].rgba, frames[index + 1].rgba),
      ).where((changed) => changed).length,
      greaterThanOrEqualTo(12),
      reason: 'Windows camera preview is below the expected live update rate',
    );
  });
}
