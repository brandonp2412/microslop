import 'package:flutter_test/flutter_test.dart';
import 'package:microslop/src/rust/api/auth.dart' as auth;
import 'package:microslop/src/rust/frb_generated.dart';

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  testWidgets('initialises the packaged Rust library', (tester) async {
    expect(RustLib.instance, isNotNull);
  });

  testWidgets('restores a cached work session when available', (tester) async {
    final existingSession = await auth.getAuthStatus();
    if (!existingSession.signedIn) return;

    final restoredSession = await auth.restoreWorkSession();
    expect(restoredSession.signedIn, isTrue);
  });
}
