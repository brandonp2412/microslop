import 'package:flutter/foundation.dart';

class AppLog {
  AppLog._();

  /// Error-only entries retained for the compact debug-log flow.
  static final entries = ValueNotifier<List<String>>([]);

  static void record(String operation, Object error, [StackTrace? stackTrace]) {
    final timestamp = DateTime.now().toIso8601String();
    if (kDebugMode) {
      debugPrint('$operation: $error\n${stackTrace ?? StackTrace.current}');
    }
    final updated = List<String>.from(entries.value)
      ..add('$timestamp | $operation | $error');
    entries.value = updated.length > 100
        ? updated.sublist(updated.length - 100)
        : updated;
  }

  static void info(String operation, Object message) {
    if (kDebugMode) debugPrint('$operation | $message');
  }

  static void debug(String operation, Object message) {
    if (kDebugMode) debugPrint('$operation | $message');
  }
}
