import 'dart:ui' as ui;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:microslop/desktop_notifications.dart';

Future<Uint8List> _avatarPng() async {
  final recorder = ui.PictureRecorder();
  final canvas = ui.Canvas(recorder);
  canvas.drawRect(
    const ui.Rect.fromLTWH(0, 0, 160, 100),
    ui.Paint()..color = const ui.Color(0xff3968d7),
  );
  canvas.drawCircle(
    const ui.Offset(80, 42),
    27,
    ui.Paint()..color = const ui.Color(0xffffd6b5),
  );
  canvas.drawRect(
    const ui.Rect.fromLTWH(46, 67, 68, 33),
    ui.Paint()..color = const ui.Color(0xffffd6b5),
  );
  final picture = recorder.endRecording();
  try {
    final image = await picture.toImage(160, 100);
    try {
      final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
      return bytes!.buffer.asUint8List(
        bytes.offsetInBytes,
        bytes.lengthInBytes,
      );
    } finally {
      image.dispose();
    }
  } finally {
    picture.dispose();
  }
}

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final notifications = FlutterDesktopNotifications();
  await notifications.showConversation(
    title: 'Microslop avatar test',
    body: 'Compact notification should show the circular avatar.',
    conversationId: 'notification-avatar-visual-probe',
    senderName: 'Avatar Test',
    senderAvatar: await _avatarPng(),
    groupConversation: false,
  );
  await Future<void>.delayed(const Duration(seconds: 2));
  await SystemNavigator.pop();
}
