import 'package:flutter/material.dart';
import 'package:microslop/call_gateway.dart';
import 'package:permission_handler/permission_handler.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  var status = 'MICROSLOP_CAMERA_SMOKE_FAIL';
  try {
    if (await Permission.camera.request() != PermissionStatus.granted) {
      throw StateError('camera permission denied');
    }
    if (!await PlatformCallVideo.startCamera()) {
      throw StateError('camera unavailable');
    }
    await Future<void>.delayed(const Duration(seconds: 3));
    final frames = await PlatformCallVideo.cameraFramesCaptured();
    final nativeFrames = await PlatformCallVideo.nativeCameraFramesReceived();
    final encoded = await PlatformCallVideo.cameraEncoderVerified();
    await PlatformCallVideo.stopCamera();
    if (frames <= 0) throw StateError('no camera frames');
    if (nativeFrames <= 0) throw StateError('camera frames did not reach Rust');
    if (!encoded) throw StateError('OpenH264 could not encode camera frame');
    status =
        'MICROSLOP_CAMERA_SMOKE_PASS frames=$frames native=$nativeFrames encoded=$encoded';
  } catch (error) {
    status = 'MICROSLOP_CAMERA_SMOKE_FAIL $error';
  }
  debugPrint(status);
  runApp(MaterialApp(home: Center(child: Text(status))));
}
