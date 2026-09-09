import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:microslop/app_log.dart';
import 'package:microslop/auth_gateway.dart';
import 'package:microslop/desktop_notifications.dart';
import 'package:microslop/main.dart';
import 'package:microslop/src/rust/frb_generated.dart';
import 'package:microslop/teams_gateway.dart';
import 'package:shared_preferences/shared_preferences.dart';

const _selfChatName = String.fromEnvironment('MICROSLOP_E2E_SELF_CHAT');

class _Finding {
  const _Finding(this.code, this.scope, this.detail);

  final String code;
  final String scope;
  final String detail;

  @override
  String toString() => 'E2E_FINDING|$code|$scope|$detail';
}

class _RecordingNotifications implements DesktopNotifications {
  final notifications = <({String title, String body})>[];

  @override
  Stream<String> get conversationSelections => const Stream.empty();

  @override
  Future<void> show({
    required String title,
    required String body,
    String? conversationId,
  }) async {
    notifications.add((title: title, body: body));
  }
}

class _SnapshotTeamsGateway implements TeamsGateway {
  _SnapshotTeamsGateway({
    required this.delegate,
    required this.chats,
    required this.snapshotTeams,
    required this.messages,
    required this.user,
  });

  final TeamsGateway delegate;
  final List<Conversation> chats;
  final List<TeamSummary> snapshotTeams;
  final Map<String, List<MessageSummary>> messages;
  final UserSummary user;

  @override
  Future<List<TeamSummary>> listTeams() async => snapshotTeams;

  @override
  Future<List<Conversation>> listChats({int limit = 50}) async =>
      chats.take(limit).toList();

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      messages[conversation.id] ?? await delegate.readMessages(conversation);

  @override
  Future<void> sendMessage(Conversation conversation, String content) async {
    await delegate.sendMessage(conversation, content);
    messages[conversation.id] = await delegate.readMessages(conversation);
  }

  @override
  Future<void> sendImageMessage(
    Conversation conversation,
    Uint8List bytes,
    String contentType, {
    String caption = '',
  }) async {
    await delegate.sendImageMessage(
      conversation,
      bytes,
      contentType,
      caption: caption,
    );
    messages[conversation.id] = await delegate.readMessages(conversation);
  }

  @override
  Future<void> setReaction(
    Conversation conversation,
    MessageSummary message,
    String reactionType,
  ) async {
    await delegate.setReaction(conversation, message, reactionType);
    messages[conversation.id] = await delegate.readMessages(conversation);
  }

  @override
  Future<UserSummary> getUser() async => user;

  @override
  Future<Uint8List?> getProfilePhoto(String userId) =>
      delegate.getProfilePhoto(userId);

  @override
  Future<Uint8List?> getChatPhoto(String chatId) =>
      delegate.getChatPhoto(chatId);

  @override
  Future<Uint8List?> getTeamPhoto(String teamId) =>
      delegate.getTeamPhoto(teamId);

  @override
  Stream<MessageEvent> messageEvents() => const Stream<MessageEvent>.empty();

  @override
  Future<void> stopMessageEvents() async {}
}

Finder _searchField() => find.byWidgetPredicate(
  (widget) => widget is TextField && widget.decoration?.hintText == 'Search...',
  description: 'workspace search field',
);

Finder _composerField() => find.byWidgetPredicate(
  (widget) =>
      widget is TextField && widget.decoration?.hintText == 'Write a message',
  description: 'message composer',
);

Future<bool> _pumpUntil(
  WidgetTester tester,
  bool Function() condition, {
  Duration timeout = const Duration(seconds: 45),
  Duration step = const Duration(milliseconds: 250),
}) async {
  final deadline = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(deadline)) {
    await tester.pump(step);
    if (condition()) return true;
  }
  return condition();
}

void _drainFrameworkExceptions(
  WidgetTester tester,
  List<_Finding> findings,
  String scope,
) {
  Object? error;
  while ((error = tester.takeException()) != null) {
    findings.add(_Finding('framework-exception', scope, error.toString()));
  }
}

String _textOf(Widget? widget) => widget is Text ? widget.data ?? '' : '';

bool _looksLikeUuid(String value) => RegExp(
  r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$',
  caseSensitive: false,
).hasMatch(value.trim());

void _printFindings(List<_Finding> findings) {
  for (final finding in findings) {
    // ignore: avoid_print
    print(finding);
  }
  // ignore: avoid_print
  print('E2E_FINDING_TOTAL=${findings.length}');
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() async {
    await RustLib.init();
  });

  testWidgets('programmatically explores the real Linux desktop app', (
    tester,
  ) async {
    expect(
      _selfChatName,
      isNotEmpty,
      reason: 'Set MICROSLOP_E2E_SELF_CHAT with --dart-define.',
    );

    final findings = <_Finding>[];
    AppLog.entries.value = const [];

    final auth = RustAuthGateway(restoreTimeout: const Duration(seconds: 45));
    final teams = RustTeamsGateway(chatTimeout: const Duration(seconds: 90));
    final notifications = _RecordingNotifications();

    final session = await auth.restoreWorkSession();
    expect(
      session.signedIn,
      isTrue,
      reason: 'A cached work session is required.',
    );

    final user = await teams.getUser();
    final chatListWatch = Stopwatch()..start();
    final chats = await teams.listChats(limit: 100);
    chatListWatch.stop();
    if (chatListWatch.elapsed > const Duration(seconds: 5)) {
      findings.add(
        _Finding(
          'slow-chat-list',
          'startup',
          'Listing ${chats.length} chats took ${chatListWatch.elapsed}.',
        ),
      );
    }
    final teamListWatch = Stopwatch()..start();
    final teamsList = await teams.listTeams();
    teamListWatch.stop();
    if (teamListWatch.elapsed > const Duration(seconds: 5)) {
      findings.add(
        _Finding(
          'slow-team-list',
          'startup',
          'Listing teams/channels took ${teamListWatch.elapsed}.',
        ),
      );
    }
    final channels = [for (final team in teamsList) ...team.channels];
    final conversations = [...chats, ...channels];

    expect(chats, isNotEmpty);
    expect(teamsList, isNotEmpty);

    final selfMatches = chats.where(
      (chat) => !chat.isGroup && chat.name.trim() == _selfChatName.trim(),
    );
    expect(
      selfMatches,
      hasLength(1),
      reason: 'CUD may only run in the configured self-chat.',
    );
    final self = selfMatches.single;

    final channelIds = channels.map((channel) => channel.id).toSet();
    final directChatsByParticipant = <String, List<Conversation>>{};
    for (final chat in chats) {
      final participantId = chat.profilePhotoUserId?.trim().toLowerCase();
      if (!chat.isGroup && participantId?.isNotEmpty == true) {
        directChatsByParticipant
            .putIfAbsent(participantId!, () => <Conversation>[])
            .add(chat);
      }
      if (chat.teamId != null || channelIds.contains(chat.id)) {
        findings.add(
          _Finding(
            'channel-duplicated-as-chat',
            chat.id,
            '${chat.name} is returned in the chat list even though it belongs to a team/channel.',
          ),
        );
      }
      if (_looksLikeUuid(chat.name)) {
        findings.add(
          _Finding(
            'uuid-visible-as-chat-name',
            chat.id,
            'Visible chat name is the raw user id: ${chat.name}',
          ),
        );
      }
      if (chat.name.trim().toLowerCase() == 'undefined') {
        findings.add(
          _Finding(
            'undefined-chat-name',
            chat.id,
            'Chat is named "undefined".',
          ),
        );
      }
      if (chat.name.trim() == 'Group chat') {
        findings.add(
          _Finding(
            'generic-group-name',
            chat.id,
            'Group is exposed with the generic fallback name.',
          ),
        );
      }
      if (!chat.isGroup &&
          chat.id != self.id &&
          chat.profilePhotoUserId == null) {
        findings.add(
          _Finding(
            'dm-missing-profile-id',
            chat.id,
            'Direct message has no participant profile-photo id.',
          ),
        );
      }
      // Named groups can render their chat photo or name initials without
      // requiring member ids. Missing member metadata is only a defect when
      // it also leaves the group without a meaningful identity.
      if (chat.isGroup &&
          chat.avatarUserIds.isEmpty &&
          chat.name.trim() == 'Group chat') {
        findings.add(
          _Finding(
            'group-missing-member-avatars',
            chat.id,
            'Unnamed group has no member ids for an identity fallback.',
          ),
        );
      }
      if (chat.preview != null &&
          RegExp(
            r'&(?:amp|lt|gt|quot|nbsp|#39);',
            caseSensitive: false,
          ).hasMatch(chat.preview!)) {
        findings.add(
          _Finding(
            'html-entity-in-sidebar-preview',
            chat.id,
            'Sidebar preview contains undecoded HTML entities: ${chat.preview}',
          ),
        );
      }
    }
    for (final entry in directChatsByParticipant.entries) {
      if (entry.value.length <= 1) continue;
      for (final chat in entry.value.skip(1)) {
        findings.add(
          _Finding(
            'duplicate-direct-message-participant',
            chat.id,
            'Multiple one-to-one chats resolve to participant ${entry.key}: '
                '${entry.value.map((item) => item.name).join(', ')}',
          ),
        );
      }
    }

    final messagesByConversation = <String, List<MessageSummary>>{};
    var next = 0;
    Future<void> readWorker() async {
      while (next < conversations.length) {
        final conversation = conversations[next++];
        try {
          final readWatch = Stopwatch()..start();
          final messages = await teams
              .readMessages(conversation)
              .timeout(const Duration(seconds: 45));
          readWatch.stop();
          messagesByConversation[conversation.id] = messages;
          if (readWatch.elapsed > const Duration(seconds: 8)) {
            findings.add(
              _Finding(
                'slow-conversation-read',
                conversation.id,
                '${conversation.name} took ${readWatch.elapsed} to load ${messages.length} messages.',
              ),
            );
          }
          if (messages.isNotEmpty &&
              conversation.kind == ConversationKind.chat &&
              conversation.preview?.trim().isNotEmpty != true) {
            findings.add(
              _Finding(
                'missing-sidebar-preview',
                conversation.id,
                '${conversation.name} has ${messages.length} messages but no preview.',
              ),
            );
          }
          for (final message in messages) {
            if (message.id.trim().isEmpty) {
              findings.add(
                _Finding(
                  'empty-message-id',
                  conversation.id,
                  'A visible message has no id, so reactions cannot target it.',
                ),
              );
            }
            if (message.sender.trim() == '?') {
              findings.add(
                _Finding(
                  'unknown-message-sender',
                  '${conversation.id}:${message.id}',
                  'Message sender could not be resolved.',
                ),
              );
            }
            if (DateTime.tryParse(message.timestamp) == null) {
              findings.add(
                _Finding(
                  'invalid-message-timestamp',
                  '${conversation.id}:${message.id}',
                  'Timestamp is not ISO-parseable: ${message.timestamp}',
                ),
              );
            }
            final currentUserId = user.id?.trim();
            final senderId = message.senderId?.trim();
            if (currentUserId?.isNotEmpty == true &&
                senderId == currentUserId &&
                !message.isFromCurrentUser) {
              findings.add(
                _Finding(
                  'current-user-message-misclassified',
                  '${conversation.id}:${message.id}',
                  'Message from the current user is rendered as incoming.',
                ),
              );
            }
            if (currentUserId?.isNotEmpty == true &&
                message.isFromCurrentUser &&
                senderId?.isNotEmpty == true &&
                senderId != currentUserId) {
              findings.add(
                _Finding(
                  'foreign-message-marked-current-user',
                  '${conversation.id}:${message.id}',
                  'Message from another sender is rendered as outgoing.',
                ),
              );
            }
            if (RegExp(
              r'<(?:div|p|span|br|a|img|at|attachment)\b',
              caseSensitive: false,
            ).hasMatch(message.content)) {
              findings.add(
                _Finding(
                  'raw-html-message-content',
                  '${conversation.id}:${message.id}',
                  'Rendered message content still contains HTML markup.',
                ),
              );
            }
            if (RegExp(
              r'&(?:amp|lt|gt|quot|nbsp|#39);',
              caseSensitive: false,
            ).hasMatch(message.content)) {
              findings.add(
                _Finding(
                  'html-entity-in-message-content',
                  '${conversation.id}:${message.id}',
                  'Message text contains undecoded HTML entities.',
                ),
              );
            }
            if (message.images.any((image) => image.bytes.isEmpty)) {
              findings.add(
                _Finding(
                  'empty-image-payload',
                  '${conversation.id}:${message.id}',
                  'Message exposes an empty image payload.',
                ),
              );
            }
          }
        } on Object catch (error) {
          findings.add(
            _Finding(
              'conversation-read-failed',
              conversation.id,
              '${conversation.name}: $error',
            ),
          );
        }
      }
    }

    await Future.wait([readWorker(), readWorker()]);

    final snapshot = _SnapshotTeamsGateway(
      delegate: teams,
      chats: chats,
      snapshotTeams: teamsList,
      messages: messagesByConversation,
      user: user,
    );
    SharedPreferences.setMockInitialValues({});
    await tester.pumpWidget(
      OstApp(
        gateway: auth,
        teamsGateway: snapshot,
        notifications: notifications,
      ),
    );

    final loaded = await _pumpUntil(
      tester,
      () =>
          _searchField().evaluate().isNotEmpty ||
          find.text('Unable to load Teams').evaluate().isNotEmpty,
      timeout: const Duration(seconds: 90),
    );
    if (!loaded) {
      findings.add(
        const _Finding(
          'workspace-load-timeout',
          'startup',
          'Workspace did not become interactive within 90 seconds.',
        ),
      );
      _printFindings(findings);
      fail('Workspace never became interactive.');
    }
    _drainFrameworkExceptions(tester, findings, 'startup');

    if (find.text('Unable to load Teams').evaluate().isNotEmpty) {
      findings.add(
        const _Finding(
          'workspace-load-error-panel',
          'startup',
          'Production UI reached the Unable to load Teams error panel.',
        ),
      );
    }

    Future<void> selectConversation(Conversation conversation) async {
      final search = _searchField();
      if (search.evaluate().isEmpty) {
        findings.add(
          _Finding(
            'search-field-missing',
            conversation.id,
            'Navigation search is unavailable.',
          ),
        );
        return;
      }
      await tester.tap(search);
      await tester.pump();
      await tester.enterText(search, conversation.name);
      await tester.pump(const Duration(milliseconds: 400));

      final kind = conversation.kind == ConversationKind.chat
          ? 'chat'
          : 'channel';
      final tile = find.byKey(
        ValueKey('conversation-tile-$kind-${conversation.id}'),
      );
      if (tile.evaluate().isEmpty) {
        final enteredQuery =
            tester.widget<TextField>(search).controller?.text ?? '';
        final visibleKeys = tester
            .widgetList<ListTile>(find.byType(ListTile))
            .map((tile) => tile.key)
            .whereType<ValueKey<String>>()
            .map((key) => key.value)
            .where((key) => key.startsWith('conversation-tile-'))
            .take(8)
            .join(',');
        // ignore: avoid_print
        print(
          'E2E_HARNESS_MISS|conversation-navigation|${conversation.id}|'
          'query=$enteredQuery|visible=$visibleKeys',
        );
        return;
      }

      if (conversation.kind == ConversationKind.chat) {
        final listTile = tester.widget<ListTile>(tile.first);
        final subtitle = _textOf(listTile.subtitle);
        final backendMessages =
            messagesByConversation[conversation.id] ?? const [];
        if (subtitle == 'No recent messages' && backendMessages.isNotEmpty) {
          findings.add(
            _Finding(
              'sidebar-says-no-recent-messages',
              conversation.id,
              '${conversation.name} has ${backendMessages.length} readable messages.',
            ),
          );
        }
      }

      final beforeErrors = AppLog.entries.value.length;
      await tester.tap(tile.first);
      await tester.pump(const Duration(milliseconds: 250));
      await _pumpUntil(
        tester,
        () => find.byType(CircularProgressIndicator).evaluate().isEmpty,
        timeout: const Duration(seconds: 15),
      );
      await tester.pump(const Duration(milliseconds: 250));
      _drainFrameworkExceptions(tester, findings, conversation.id);

      final newErrors = AppLog.entries.value.skip(beforeErrors);
      for (final error in newErrors) {
        findings.add(
          _Finding('app-log-error-after-selection', conversation.id, error),
        );
      }

      if (_composerField().evaluate().isEmpty) {
        findings.add(
          _Finding(
            'composer-missing-after-selection',
            conversation.id,
            '${conversation.name} selected without a composer.',
          ),
        );
      }
      if (find.text(conversation.name).evaluate().isEmpty) {
        findings.add(
          _Finding(
            'selected-title-missing',
            conversation.id,
            'Selected conversation title is not visible.',
          ),
        );
      }

      final backendMessages =
          messagesByConversation[conversation.id] ?? const [];
      if (backendMessages.isNotEmpty &&
          find.text('No messages yet.').evaluate().isNotEmpty) {
        findings.add(
          _Finding(
            'ui-shows-empty-conversation',
            conversation.id,
            'Backend returned ${backendMessages.length} messages.',
          ),
        );
      }

      final audioCallAction = find.byTooltip('Start audio call');
      final videoCallAction = find.byTooltip('Start video call');
      final shouldHaveCallAction =
          conversation.kind == ConversationKind.chat && !conversation.isGroup;
      if (shouldHaveCallAction &&
          (audioCallAction.evaluate().isEmpty ||
              videoCallAction.evaluate().isEmpty)) {
        findings.add(
          _Finding(
            'dm-call-control-missing',
            conversation.id,
            'One-to-one chat is missing its audio or video call control.',
          ),
        );
      }
      if (!shouldHaveCallAction &&
          (audioCallAction.evaluate().isNotEmpty ||
              videoCallAction.evaluate().isNotEmpty)) {
        findings.add(
          _Finding(
            'call-control-on-non-dm',
            conversation.id,
            'Call control is visible for a group/channel.',
          ),
        );
      }
    }

    // Exercise a broad sample of real conversations through actual Flutter taps.
    final uiSample = <Conversation>[
      self,
      ...chats.where((chat) => chat.id != self.id).take(24),
      ...channels.take(12),
    ];
    for (final conversation in uiSample) {
      await selectConversation(conversation);
    }

    // Search mechanics.
    final search = _searchField();
    if (search.evaluate().isNotEmpty) {
      await tester.tap(search);
      await tester.pump();
      await tester.enterText(search, 'THIS_SEARCH_SHOULD_NEVER_MATCH_9A71');
      await tester.pump(const Duration(milliseconds: 250));
      if (find.text('No chats found.').evaluate().isEmpty) {
        // The widget suite verifies this state deterministically. Keep a live
        // diagnostic without inflating the application bug count if the Linux
        // harness cannot observe the lazily-built result pane.
        // ignore: avoid_print
        print('E2E_HARNESS_MISS|empty-search-state|search');
      }
      if (find.byTooltip('Clear search').evaluate().isEmpty) {
        findings.add(
          const _Finding(
            'clear-search-control-missing',
            'search',
            'Non-empty search has no clear control.',
          ),
        );
      } else {
        await tester.tap(find.byTooltip('Clear search').first);
        await tester.pump(const Duration(milliseconds: 250));
      }
    }

    // Inspect Settings without changing persistent state or sending a notification/call.
    if (find.text('Settings').evaluate().isNotEmpty) {
      final settingsTile = find.ancestor(
        of: find.text('Settings'),
        matching: find.byType(ListTile),
      );
      if (settingsTile.evaluate().isNotEmpty) {
        await tester.tap(settingsTile.first);
        await tester.pump(const Duration(milliseconds: 350));
        _drainFrameworkExceptions(tester, findings, 'settings');
        final settingsSearch = find.byKey(const ValueKey('settings-search'));
        for (final label in const [
          'Enable notifications',
          '@mentions',
          'Make a test call',
        ]) {
          await tester.enterText(settingsSearch, label);
          await tester.pump(const Duration(milliseconds: 150));
          if (find.text(label).evaluate().isEmpty) {
            findings.add(
              _Finding(
                'settings-control-missing',
                'settings',
                '$label is absent.',
              ),
            );
          }
        }
        await tester.enterText(settingsSearch, '');
        await tester.pump(const Duration(milliseconds: 150));
        final iconButtons = tester.widgetList<IconButton>(
          find.byType(IconButton),
        );
        var index = 0;
        for (final button in iconButtons) {
          if ((button.tooltip ?? '').trim().isEmpty) {
            findings.add(
              _Finding(
                'icon-button-missing-tooltip',
                'settings:$index',
                'IconButton has no tooltip.',
              ),
            );
          }
          index++;
        }
        await tester.pageBack();
        await tester.pump(const Duration(milliseconds: 300));
      }
    }

    // Desktop/mobile breakpoints and cramped-but-usable desktop window sizes.
    for (final size in const [
      Size(360, 480),
      Size(480, 640),
      Size(719, 480),
      Size(720, 480),
      Size(800, 480),
      Size(1024, 600),
      Size(1440, 900),
    ]) {
      await tester.binding.setSurfaceSize(size);
      await tester.pump(const Duration(milliseconds: 350));
      _drainFrameworkExceptions(
        tester,
        findings,
        'viewport-${size.width}x${size.height}',
      );
      if (size.width < 720) {
        if (find.byTooltip('Open navigation').evaluate().isEmpty) {
          findings.add(
            _Finding(
              'mobile-navigation-button-missing',
              '${size.width}x${size.height}',
              'Narrow layout has no drawer button.',
            ),
          );
        }
      } else if (find.byTooltip('Open navigation').evaluate().isNotEmpty) {
        findings.add(
          _Finding(
            'desktop-shows-mobile-navigation',
            '${size.width}x${size.height}',
            'Desktop layout still exposes the drawer button.',
          ),
        );
      }
    }
    await tester.binding.setSurfaceSize(null);
    await tester.pump(const Duration(milliseconds: 300));

    // The only mutation: send a unique marker in the explicitly configured self-chat.
    await selectConversation(self);
    final composer = _composerField();
    if (composer.evaluate().isNotEmpty) {
      final marker =
          'Microslop UI exploration ${DateTime.now().toUtc().toIso8601String()}';
      await tester.enterText(composer, marker);
      await tester.pump(const Duration(milliseconds: 100));
      final send = find.byTooltip('Send message');
      if (send.evaluate().isEmpty) {
        findings.add(
          const _Finding(
            'send-control-missing',
            'self-chat',
            'Composer has no send button.',
          ),
        );
      } else {
        await tester.tap(send.first);
        await tester.pump(const Duration(milliseconds: 200));
        if (find.text(marker).evaluate().isEmpty) {
          findings.add(
            const _Finding(
              'optimistic-send-not-visible',
              'self-chat',
              'Sent self-chat message was not rendered immediately.',
            ),
          );
        }
        final confirmed = await _pumpUntil(
          tester,
          () => (messagesByConversation[self.id] ?? const []).any(
            (message) => message.content.contains(marker),
          ),
          timeout: const Duration(seconds: 2),
        );
        // Refresh through the read-only gateway because the UI cache above is a snapshot.
        if (!confirmed) {
          try {
            final refreshed = await teams.readMessages(self);
            if (!refreshed.any((message) => message.content.contains(marker))) {
              findings.add(
                const _Finding(
                  'self-chat-send-not-confirmed',
                  'self-chat',
                  'Sent marker did not round-trip through Teams.',
                ),
              );
            }
          } on Object catch (error) {
            findings.add(
              _Finding(
                'self-chat-read-after-send-failed',
                'self-chat',
                '$error',
              ),
            );
          }
        }
      }
    }

    _drainFrameworkExceptions(tester, findings, 'final');
    _printFindings(findings);

    // This is an exploratory run: findings are the output, not test-framework failures.
    expect(user.displayName, isNotEmpty);
  });
}
