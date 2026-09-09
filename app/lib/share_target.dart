import 'dart:async';

import 'package:flutter/services.dart';

class SharedContent {
  const SharedContent({this.text, this.imageBytes, this.contentType});

  final String? text;
  final Uint8List? imageBytes;
  final String? contentType;

  bool get hasText => text?.trim().isNotEmpty == true;
  bool get hasImage => imageBytes?.isNotEmpty == true;

  static SharedContent? fromMap(Map<Object?, Object?>? value) {
    if (value == null) return null;
    final text = value['text'] as String?;
    final data = value['data'];
    final bytes = switch (data) {
      Uint8List bytes => bytes,
      List<Object?> values => Uint8List.fromList(values.cast<int>()),
      _ => null,
    };
    final contentType = value['contentType'] as String?;
    final content = SharedContent(
      text: text,
      imageBytes: bytes,
      contentType: contentType,
    );
    return content.hasText || content.hasImage ? content : null;
  }
}

class PlatformShareTarget {
  static const _channel = MethodChannel('microslop/share_target');
  static final _events = StreamController<SharedContent>.broadcast(sync: true);
  static bool _initialized = false;

  static Stream<SharedContent> get events {
    _initialize();
    return _events.stream;
  }

  static Future<SharedContent?> takePending() async {
    _initialize();
    try {
      final value = await _channel.invokeMethod<Map<Object?, Object?>>(
        'takePendingShare',
      );
      return SharedContent.fromMap(value);
    } on MissingPluginException {
      return null;
    }
  }

  static void _initialize() {
    if (_initialized) return;
    _initialized = true;
    _channel.setMethodCallHandler((call) async {
      if (call.method != 'sharedContent') return;
      final content = SharedContent.fromMap(
        (call.arguments as Map<Object?, Object?>?),
      );
      if (content != null) _events.add(content);
    });
  }
}
