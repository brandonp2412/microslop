import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:microslop/auth_gateway.dart';
import 'package:microslop/main.dart';
import 'package:microslop/src/rust/frb_generated.dart';
import 'package:microslop/teams_gateway.dart';

const _selfChatName = String.fromEnvironment(
  'MICROSLOP_E2E_SELF_CHAT',
  defaultValue: 'brandonp2412',
);

Future<void> _pumpUntil(
  WidgetTester tester,
  bool Function() condition, {
  Duration timeout = const Duration(seconds: 45),
}) async {
  final deadline = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(deadline) && !condition()) {
    await tester.pump(const Duration(milliseconds: 250));
  }
  expect(condition(), isTrue);
}

Finder _messageBubbles() => find.byWidgetPredicate(
  (widget) =>
      widget.key is ValueKey<String> &&
      ((widget.key! as ValueKey<String>).value).startsWith('message-bubble-'),
);

Map<String, double> _visibleBubbleTops(WidgetTester tester) {
  final screen = tester.getRect(find.byType(Scaffold).first);
  return {
    for (final element in _messageBubbles().evaluate())
      if (tester.getRect(find.byWidget(element.widget)).bottom > screen.top &&
          tester.getRect(find.byWidget(element.widget)).top < screen.bottom)
        ((element.widget.key! as ValueKey<String>).value): tester
            .getTopLeft(find.byWidget(element.widget))
            .dy,
  };
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(RustLib.init);

  testWidgets('self-chat image loading does not shift historical messages', (
    tester,
  ) async {
    final auth = RustAuthGateway(restoreTimeout: const Duration(seconds: 45));
    final teams = RustTeamsGateway(chatTimeout: const Duration(seconds: 90));
    final session = await auth.restoreWorkSession();
    expect(session.signedIn, isTrue);

    await tester.pumpWidget(OstApp(gateway: auth, teamsGateway: teams));
    final search = find.byWidgetPredicate(
      (widget) =>
          widget is TextField && widget.decoration?.hintText == 'Search...',
    );
    await _pumpUntil(tester, () => search.evaluate().isNotEmpty);
    await tester.enterText(search, _selfChatName);
    await tester.pump(const Duration(milliseconds: 500));
    await _pumpUntil(
      tester,
      () => find.text(_selfChatName).evaluate().isNotEmpty,
    );
    await tester.tap(find.text(_selfChatName).first);
    final composer = find.byWidgetPredicate(
      (widget) =>
          widget is TextField &&
          widget.decoration?.hintText == 'Write a message',
    );
    await _pumpUntil(tester, () => composer.evaluate().isNotEmpty);
    await tester.pump(const Duration(seconds: 3));

    var comparedFrames = 0;
    var sawImage = false;
    for (var page = 0; page < 8; page++) {
      final bubbles = _messageBubbles();
      expect(bubbles, findsWidgets);
      final messageList = find.ancestor(
        of: bubbles.first,
        matching: find.byType(ListView),
      );
      expect(messageList, findsOneWidget);
      await tester.drag(messageList, const Offset(0, 520));
      await tester.pump();
      final before = _visibleBubbleTops(tester);
      sawImage =
          sawImage ||
          find
              .descendant(of: bubbles, matching: find.byType(Image))
              .evaluate()
              .isNotEmpty;
      await tester.pump(const Duration(seconds: 1));
      final after = _visibleBubbleTops(tester);
      for (final entry in before.entries) {
        final next = after[entry.key];
        if (next == null) continue;
        expect(
          (next - entry.value).abs(),
          lessThanOrEqualTo(0.5),
          reason: '${entry.key} shifted while image data loaded',
        );
        comparedFrames++;
      }
    }
    expect(comparedFrames, greaterThan(0));
    expect(sawImage, isTrue);
  });
}
