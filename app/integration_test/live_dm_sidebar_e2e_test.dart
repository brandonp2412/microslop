import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:microslop/main.dart';
import 'package:microslop/platform_backend.dart';
import 'package:shared_preferences/shared_preferences.dart';

Finder _searchField() => find.byWidgetPredicate(
  (widget) => widget is TextField && widget.decoration?.hintText == 'Search...',
  description: 'workspace search field',
);

Finder _directMessageList() =>
    find.byKey(const PageStorageKey<String>('direct-chat-list'));

Set<String> _renderedDirectMessageIds(WidgetTester tester) => tester
    .widgetList<ListTile>(
      find.descendant(
        of: _directMessageList(),
        matching: find.byType(ListTile),
      ),
    )
    .map((tile) => tile.key)
    .whereType<ValueKey<String>>()
    .map((key) => key.value)
    .where((key) => key.startsWith('conversation-tile-chat-'))
    .map((key) => key.substring('conversation-tile-chat-'.length))
    .toSet();

Future<void> _pumpUntil(
  WidgetTester tester,
  bool Function() condition, {
  Duration timeout = const Duration(seconds: 90),
}) async {
  final deadline = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(deadline)) {
    await tester.pump(const Duration(milliseconds: 250));
    if (condition()) return;
  }
  fail('Timed out waiting for the live workspace.');
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(initializePlatformBackend);

  testWidgets('live Linux mobile drawer renders more than four direct messages', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(400, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    SharedPreferences.setMockInitialValues({});
    final auth = createPlatformAuthGateway();
    final teams = createPlatformTeamsGateway();

    final session = await auth.restoreWorkSession();
    expect(
      session.signedIn,
      isTrue,
      reason: 'A cached work session is required.',
    );

    await tester.pumpWidget(OstApp(gateway: auth, teamsGateway: teams));
    await _pumpUntil(
      tester,
      () =>
          find.byTooltip('Open navigation').evaluate().isNotEmpty ||
          find.text('Unable to load Teams').evaluate().isNotEmpty,
    );
    expect(find.text('Unable to load Teams'), findsNothing);
    await tester.tap(find.byTooltip('Open navigation'));
    await _pumpUntil(
      tester,
      () =>
          _directMessageList().evaluate().isNotEmpty ||
          find.text('Unable to load Teams').evaluate().isNotEmpty,
    );
    expect(find.text('Unable to load Teams'), findsNothing);
    expect(_searchField(), findsOneWidget);
    expect(_directMessageList(), findsOneWidget);
    final renderedIds = <String>{..._renderedDirectMessageIds(tester)};
    final initiallyRendered = renderedIds.length;

    for (var attempt = 0; attempt < 20; attempt++) {
      final scrollable = find.descendant(
        of: _directMessageList(),
        matching: find.byType(Scrollable),
      );
      expect(scrollable, findsOneWidget);
      final position = tester.state<ScrollableState>(scrollable).position;
      final beforePixels = position.pixels;
      final beforeMax = position.maxScrollExtent;

      await tester.drag(_directMessageList(), const Offset(0, -600));
      await tester.pump(const Duration(milliseconds: 750));
      renderedIds.addAll(_renderedDirectMessageIds(tester));

      final updatedPosition = tester
          .state<ScrollableState>(scrollable)
          .position;
      if (updatedPosition.pixels >= updatedPosition.maxScrollExtent - 1 &&
          updatedPosition.pixels == beforePixels &&
          updatedPosition.maxScrollExtent == beforeMax) {
        break;
      }
    }

    // ignore: avoid_print
    print('LIVE_DM_INITIAL_VISIBLE=$initiallyRendered');
    expect(
      initiallyRendered,
      greaterThan(4),
      reason:
          'The real mobile Direct messages drawer still initially exposes only four DMs.',
    );
    expect(
      renderedIds.length,
      greaterThan(4),
      reason: 'The real Direct messages sidebar still exposes only four DMs.',
    );

    final backendChats = await teams.listChats(limit: 50);
    final backendDirectMessages = backendChats
        .where((chat) => !chat.isGroup)
        .length;
    expect(backendDirectMessages, greaterThan(4));

    // ignore: avoid_print
    print(
      'LIVE_DM_SIDEBAR_PROOF initialVisible=$initiallyRendered '
      'distinctRendered=${renderedIds.length} backendDMs=$backendDirectMessages '
      'backendChats=${backendChats.length}',
    );
  });
}
