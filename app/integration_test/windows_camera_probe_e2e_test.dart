import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:microslop/call_gateway.dart';
import 'package:microslop/platform_backend.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('Windows camera preview produces live frames', (tester) async {
    expect(Platform.isWindows, isTrue);
    await initializePlatformBackend();
    final calls = createPlatformCallGateway();
    final media = calls as MediaDeviceCallGateway;
    final cameras = await media.mediaDevices(MediaDeviceKind.camera);
    debugPrint(
      'MICROSLOP_WINDOWS_CAMERAS=${cameras.map((camera) => camera.label).join('|')}',
    );
    expect(cameras.where((camera) => camera.id.isNotEmpty), isNotEmpty);
    final camera = cameras.firstWhere((camera) => camera.id.isNotEmpty);
    await media.selectMediaDevice(MediaDeviceKind.camera, camera.id);

    final first = await media.previewCamera();
    expect(first, isNotNull);
    await Future<void>.delayed(const Duration(milliseconds: 700));
    final second = await media.previewCamera();
    expect(second, isNotNull);

    int checksum(CallVideoFrame frame) => frame.rgba.fold<int>(
      0,
      (sum, value) => (sum * 31 + value) & 0x7fffffff,
    );
    final firstChecksum = checksum(first!);
    final secondChecksum = checksum(second!);
    debugPrint(
      'MICROSLOP_WINDOWS_CAMERA_FRAME1=${first.width}x${first.height}:$firstChecksum',
    );
    debugPrint(
      'MICROSLOP_WINDOWS_CAMERA_FRAME2=${second.width}x${second.height}:$secondChecksum',
    );
  });
}
