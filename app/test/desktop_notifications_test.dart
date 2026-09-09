import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter_test/flutter_test.dart';
import 'package:microslop/desktop_notifications.dart';

Future<Uint8List> _solidPng(int width, int height) async {
  final recorder = ui.PictureRecorder();
  final canvas = ui.Canvas(recorder);
  canvas.drawRect(
    ui.Rect.fromLTWH(0, 0, width.toDouble(), height.toDouble()),
    ui.Paint()..color = const ui.Color(0xff44aa66),
  );
  final picture = recorder.endRecording();
  try {
    final image = await picture.toImage(width, height);
    try {
      final data = await image.toByteData(format: ui.ImageByteFormat.png);
      return data!.buffer.asUint8List(data.offsetInBytes, data.lengthInBytes);
    } finally {
      image.dispose();
    }
  } finally {
    picture.dispose();
  }
}

int _alphaAt(ByteData rgba, int width, int x, int y) =>
    rgba.getUint8((y * width + x) * 4 + 3);

void main() {
  testWidgets('notification avatars are center-cropped into a smooth circle', (
    tester,
  ) async {
    await tester.runAsync(() async {
      final source = await _solidPng(160, 80);
      final rounded = await createCircularNotificationAvatar(source);

      expect(rounded, isNotNull);
      final codec = await ui.instantiateImageCodec(rounded!);
      try {
        final frame = await codec.getNextFrame();
        final image = frame.image;
        try {
          expect(image.width, 96);
          expect(image.height, 96);
          final rgba = await image.toByteData(
            format: ui.ImageByteFormat.rawRgba,
          );
          expect(rgba, isNotNull);
          expect(_alphaAt(rgba!, image.width, 0, 0), 0);
          expect(_alphaAt(rgba, image.width, 48, 48), 255);
          expect(_alphaAt(rgba, image.width, 95, 95), 0);
        } finally {
          image.dispose();
        }
      } finally {
        codec.dispose();
      }
    });
  });

  testWidgets('notification badges extend outside the avatar circle', (
    tester,
  ) async {
    await tester.runAsync(() async {
      final source = await _solidPng(160, 80);
      final rounded = await createCircularNotificationAvatar(
        source,
        badge: source,
      );
      expect(rounded, isNotNull);
      final codec = await ui.instantiateImageCodec(rounded!);
      ui.Image? image;
      try {
        image = (await codec.getNextFrame()).image;
        final rgba = (await image.toByteData(
          format: ui.ImageByteFormat.rawRgba,
        ))!;
        expect(_alphaAt(rgba, image.width, 0, 0), 0);
        expect(_alphaAt(rgba, image.width, 95, 95), 255);
      } finally {
        image?.dispose();
        codec.dispose();
      }
    });
  });

  testWidgets('invalid avatars and badges fall back without throwing', (
    tester,
  ) async {
    await tester.runAsync(() async {
      final invalid = Uint8List.fromList([1, 2, 3]);
      expect(await createCircularNotificationAvatar(invalid), isNull);
      final source = await _solidPng(96, 96);
      expect(
        await createCircularNotificationAvatar(source, badge: invalid),
        isNull,
      );
      expect(await createCircularNotificationAvatar(source), isNotNull);
    });
  });

  testWidgets('notification avatar preparation accepts no avatar', (
    tester,
  ) async {
    expect(await createCircularNotificationAvatar(null), isNull);
    expect(await createCircularNotificationAvatar(Uint8List(0)), isNull);
  });
}
