import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:super_clipboard/super_clipboard.dart';

import 'app_log.dart';

class MessageClipboardContent {
  const MessageClipboardContent({
    this.text,
    this.imageBytes,
    this.imageContentType,
  });

  final String? text;
  final Uint8List? imageBytes;
  final String? imageContentType;
}

class MessageClipboard {
  static Future<void> writeImage(Uint8List bytes, String contentType) async {
    final clipboard = SystemClipboard.instance;
    if (clipboard == null) throw StateError('The clipboard is unavailable.');
    final item = DataWriterItem();
    item.add(Formats.plainText('Image'));
    switch (contentType) {
      case 'image/png':
        item.add(Formats.png(bytes));
      case 'image/jpeg':
        item.add(Formats.jpeg(bytes));
      case 'image/gif':
        item.add(Formats.gif(bytes));
      case 'image/webp':
        item.add(Formats.webp(bytes));
      default:
        throw UnsupportedError('Cannot copy $contentType images.');
    }
    await clipboard.write([item]);
  }

  @visibleForTesting
  static Future<MessageClipboardContent?> Function()? debugRead;

  static Future<MessageClipboardContent?> read() async {
    final testRead = debugRead;
    if (testRead != null) return testRead();
    try {
      final clipboard = SystemClipboard.instance;
      if (clipboard == null) return null;
      final reader = await clipboard.read();
      for (final (format, contentType) in [
        (Formats.gif, 'image/gif'),
        (Formats.png, 'image/png'),
        (Formats.jpeg, 'image/jpeg'),
        (Formats.webp, 'image/webp'),
      ]) {
        if (!reader.canProvide(format)) continue;
        final completer = Completer<Uint8List?>();
        reader.getFile(format, (file) async {
          try {
            final chunks = <int>[];
            await for (final chunk in file.getStream()) {
              chunks.addAll(chunk);
            }
            if (!completer.isCompleted) {
              completer.complete(Uint8List.fromList(chunks));
            }
          } catch (error, stackTrace) {
            AppLog.record('Read clipboard image', error, stackTrace);
            if (!completer.isCompleted) completer.complete(null);
          }
        });
        final bytes = await completer.future;
        if (bytes != null && bytes.isNotEmpty) {
          return MessageClipboardContent(
            imageBytes: bytes,
            imageContentType: contentType,
          );
        }
      }
      final text = reader.canProvide(Formats.plainText)
          ? await reader.readValue(Formats.plainText)
          : null;
      return text == null || text.isEmpty
          ? null
          : MessageClipboardContent(text: text);
    } catch (error, stackTrace) {
      AppLog.record('Read clipboard', error, stackTrace);
      return null;
    }
  }
}
