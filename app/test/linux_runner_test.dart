import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test('normal Linux launches share one application instance', () {
    final source = File('linux/runner/my_application.cc').readAsStringSync();

    expect(source, contains('"flags", G_APPLICATION_DEFAULT_FLAGS'));
    expect(source, isNot(contains('g_getenv("DWL_APP_ID")')));
  });

  test('Linux builds keep the canonical application identity', () {
    final source = File('linux/CMakeLists.txt').readAsStringSync();

    expect(source, contains('set(APPLICATION_ID "app.microslop")'));
    expect(RegExp(r'set\(APPLICATION_ID ').allMatches(source), hasLength(1));
  });
}
