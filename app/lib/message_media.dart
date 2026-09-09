import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

class MessageMediaFile {
  const MessageMediaFile({
    required this.data,
    required this.name,
    required this.contentType,
  });

  final Uint8List data;
  final String name;
  final String contentType;
}

class PlatformMessageMedia {
  PlatformMessageMedia._();

  static const _channel = MethodChannel('microslop/message_media');

  static bool get supported =>
      !kIsWeb && defaultTargetPlatform == TargetPlatform.android;

  static Future<Uint8List> captureCameraFrame() async {
    final data = await _channel.invokeMethod<Uint8List>('captureCameraFrame');
    if (data == null || data.isEmpty) {
      throw StateError('The camera did not produce a photo.');
    }
    return data;
  }

  static Future<void> startAudioRecording() async {
    await _channel.invokeMethod<void>('startAudioRecording');
  }

  static Future<MessageMediaFile> stopAudioRecording() async {
    final result = await _channel.invokeMapMethod<String, dynamic>(
      'stopAudioRecording',
    );
    final data = result?['data'];
    final name = result?['name'];
    final contentType = result?['contentType'];
    if (data is! Uint8List ||
        data.isEmpty ||
        name is! String ||
        contentType is! String) {
      throw StateError('The audio recording was empty.');
    }
    return MessageMediaFile(data: data, name: name, contentType: contentType);
  }

  static Future<void> cancelAudioRecording() async {
    await _channel.invokeMethod<void>('cancelAudioRecording');
  }
}
