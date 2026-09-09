import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:microslop/auth_gateway.dart';
import 'package:microslop/main.dart';
import 'package:microslop/teams_gateway.dart';
import 'package:shared_preferences/shared_preferences.dart';

class _AuthGateway implements AuthGateway {
  const _AuthGateway();

  @override
  Future<AuthSnapshot> status() async =>
      const AuthSnapshot(signedIn: true, loginInProgress: false);

  @override
  Future<AuthSnapshot> restoreWorkSession() => status();

  @override
  Future<DeviceCodeDetails> beginWorkLogin() => throw UnsupportedError('login');

  @override
  Future<AuthSnapshot> completeWorkLogin() => status();

  @override
  Future<void> cancelWorkLogin() async {}

  @override
  Future<AuthSnapshot> logout() async =>
      const AuthSnapshot(signedIn: false, loginInProgress: false);
}

class _TeamsGateway implements TeamsGateway, ResponsiveTeamsGateway {
  final _events = StreamController<MessageEvent>.broadcast();
  Completer<List<MessageSummary>>? _pendingAda;
  var _adaRefreshes = 0;

  static const chats = [
    Conversation.chat(id: 'chat-ada', name: 'Chat with Ada', isGroup: false),
    Conversation.chat(
      id: 'chat-grace',
      name: 'Chat with Grace',
      isGroup: false,
    ),
  ];

  @override
  Future<List<TeamSummary>> listTeams() async => const [];

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async => chats;

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) =>
      refreshMessages(conversation);

  @override
  Future<List<MessageSummary>> refreshMessages(Conversation conversation) {
    if (conversation.id == 'chat-ada' && _adaRefreshes++ > 0) {
      return (_pendingAda = Completer<List<MessageSummary>>()).future;
    }
    return Future.value([
      MessageSummary(
        id: '${conversation.id}-cached',
        sender: 'Test User',
        timestamp: '2026-09-02T00:00:00Z',
        content: conversation.id == 'chat-ada'
            ? 'Cached Ada history'
            : 'Cached Grace history',
      ),
    ]);
  }

  void completeAdaRefresh() {
    _pendingAda?.complete(const [
      MessageSummary(
        id: 'chat-ada-fresh',
        sender: 'Test User',
        timestamp: '2026-09-02T00:01:00Z',
        content: 'Fresh Ada history',
      ),
    ]);
    _pendingAda = null;
  }

  @override
  Future<List<MessageSummary>> hydrateRecentImages(
    Conversation conversation,
  ) async => const [];

  @override
  Future<void> sendMessage(Conversation conversation, String content) async {}

  @override
  Future<void> sendImageMessage(
    Conversation conversation,
    Uint8List bytes,
    String contentType, {
    String caption = '',
  }) async {}

  @override
  Future<void> setReaction(
    Conversation conversation,
    MessageSummary message,
    String reactionType,
  ) async {}

  @override
  Future<UserSummary> getUser() async =>
      const UserSummary(id: 'test-user', displayName: 'Test User');

  @override
  Future<Uint8List?> getProfilePhoto(String userId) async => null;

  @override
  Future<Uint8List?> getChatPhoto(String chatId) async => null;

  @override
  Future<Uint8List?> getTeamPhoto(String teamId) async => null;

  @override
  Stream<MessageEvent> messageEvents() => _events.stream;

  @override
  Future<void> stopMessageEvents() async {}
}

Future<void> _pumpUntil(
  WidgetTester tester,
  bool Function() condition, {
  Duration timeout = const Duration(seconds: 10),
}) async {
  final deadline = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(deadline)) {
    await tester.pump(const Duration(milliseconds: 50));
    if (condition()) return;
  }
  fail('Timed out waiting for UI state.');
}

Future<void> _openChat(WidgetTester tester, String conversationId) async {
  await tester.tap(find.byTooltip('Open navigation'));
  await tester.pump();
  final tile = find.byKey(ValueKey('conversation-tile-chat-$conversationId'));
  await _pumpUntil(tester, () => tile.evaluate().isNotEmpty);
  await tester.tap(tile);
  await tester.pump();
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('cached chat reopen has no message spinner on Android', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({});
    final gateway = _TeamsGateway();
    addTearDown(gateway._events.close);

    await tester.pumpWidget(
      OstApp(gateway: const _AuthGateway(), teamsGateway: gateway),
    );
    await _pumpUntil(
      tester,
      () => find.byTooltip('Open navigation').evaluate().isNotEmpty,
    );

    await _openChat(tester, 'chat-ada');
    await _pumpUntil(
      tester,
      () => find.text('Cached Ada history').evaluate().isNotEmpty,
    );
    await _openChat(tester, 'chat-ada');

    expect(find.text('Cached Ada history'), findsWidgets);
    expect(
      find.byKey(const ValueKey('message-loading-indicator')),
      findsNothing,
    );

    gateway.completeAdaRefresh();
    await _pumpUntil(
      tester,
      () => find.text('Fresh Ada history').evaluate().isNotEmpty,
    );
  });
}
