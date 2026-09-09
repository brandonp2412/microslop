import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:microslop/call_gateway.dart';
import 'package:microslop/desktop_notifications.dart';
import 'package:microslop/main.dart';
import 'package:microslop/teams_gateway.dart';
import 'package:permission_handler/permission_handler.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../test/widget_test.dart' as fixtures;

class _SelfReactionGateway extends fixtures.GalleryMessagesGateway {
  final reactions = <String>[];
  final selectedReactions = <String>{};

  @override
  Future<List<MessageSummary>> readMessages(Conversation conversation) async =>
      [
        MessageSummary(
          id: 'self-reaction-message',
          sender: 'brandonp2412',
          senderId: 'test-user',
          isFromCurrentUser: true,
          timestamp: '2026-09-05T00:00:00Z',
          content: 'React to my self message',
          reactions: [
            for (final type in selectedReactions)
              MessageReaction(type: type, count: 1, selected: true),
          ],
        ),
      ];

  @override
  Future<void> setReaction(
    Conversation conversation,
    MessageSummary message,
    String reactionType,
  ) async {
    reactions.add(reactionType);
    if (!selectedReactions.remove(reactionType)) {
      selectedReactions.add(reactionType);
    }
  }
}

Future<void> _pumpMobileApp(
  WidgetTester tester,
  TeamsGateway teams, {
  CallGateway? calls,
  DesktopNotifications? notifications,
}) async {
  await tester.binding.setSurfaceSize(const Size(400, 800));
  await tester.pumpWidget(
    OstApp(
      key: UniqueKey(),
      gateway: fixtures.RestoringAuthGateway(),
      teamsGateway: teams,
      callGateway: calls,
      notifications: notifications,
    ),
  );
  await tester.pumpAndSettle();
}

void _mockMediaPermissions(WidgetTester tester) {
  const channel = MethodChannel('flutter.baseflow.com/permissions/methods');
  tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
    channel,
    (call) async => call.method == 'requestPermissions'
        ? <int, int>{Permission.microphone.value: 1, Permission.camera.value: 1}
        : null,
  );
  addTearDown(
    () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      channel,
      null,
    ),
  );
}

void _mockBackgroundNotifications(WidgetTester tester) {
  const channel = MethodChannel('microslop/background_notifications');
  tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
    channel,
    (call) async => call.method == 'isRunning' ? false : null,
  );
  addTearDown(
    () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      channel,
      null,
    ),
  );
}

Future<Finder> _openMobileSettings(
  WidgetTester tester,
  TeamsGateway teams, {
  CallGateway? calls,
  DesktopNotifications? notifications,
}) async {
  await _pumpMobileApp(
    tester,
    teams,
    calls: calls,
    notifications: notifications,
  );
  await tester.tap(find.byTooltip('Open navigation'));
  await tester.pumpAndSettle();
  await tester.tap(find.text('Settings'));
  await tester.pumpAndSettle();
  final search = find.byKey(const ValueKey('settings-search'));
  expect(search, findsOneWidget);
  return search;
}

Future<void> _openConversation(
  WidgetTester tester,
  String name,
  String conversationId,
) async {
  await tester.tap(find.byTooltip('Open navigation'));
  await tester.pumpAndSettle();
  final search = find.byType(TextField).first;
  await tester.enterText(search, name);
  await tester.pumpAndSettle();
  final tile = find.byKey(ValueKey('conversation-tile-chat-$conversationId'));
  expect(tile, findsOneWidget);
  await tester.tap(tile);
  await tester.pumpAndSettle();
  expect(find.text(name), findsWidgets);
  expect(find.byTooltip('Open navigation'), findsOneWidget);
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  setUp(() => SharedPreferences.setMockInitialValues({}));

  testWidgets('mobile navigation and mention completion work on device', (
    tester,
  ) async {
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await _pumpMobileApp(tester, fixtures.MentionSuggestionsGateway());
    await _openConversation(tester, 'Project group', 'group-mentions');

    final composer = find.byType(TextField).last;
    await tester.enterText(composer, '@Gr');
    await tester.pumpAndSettle();

    expect(find.text('People in this conversation'), findsOneWidget);
    final grace = find.text('Grace Hopper');
    expect(grace, findsWidgets);
    await tester.tap(grace.last);
    await tester.pumpAndSettle();
    expect(
      tester.widget<TextField>(composer).controller!.text,
      '@Grace Hopper ',
    );
  });

  testWidgets('mobile long press reaction works on device', (tester) async {
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = fixtures.ReactionsGateway();
    await _pumpMobileApp(tester, gateway);
    await _openConversation(tester, 'Chat with Ada', 'chat-1');

    await tester.longPress(find.text('React to this message'));
    await tester.pumpAndSettle();
    expect(find.byTooltip('👍'), findsOneWidget);
    await tester.tap(find.byTooltip('👍'));
    await tester.pumpAndSettle();

    expect(gateway.reactionType, 'like');
  });

  testWidgets('optimistic message send works on device', (tester) async {
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = fixtures.DelayedSendGateway();
    await _pumpMobileApp(tester, gateway);
    await _openConversation(tester, 'Chat with Ada', 'chat-1');

    final composer = find.byType(TextField).last;
    await tester.enterText(composer, 'Waydroid optimistic send');
    await tester.tap(find.byTooltip('Send message'));
    await tester.pump();

    expect(gateway.sentContent, 'Waydroid optimistic send');
    expect(find.text('Waydroid optimistic send'), findsOneWidget);
    expect(tester.widget<TextField>(composer).controller!.text, isEmpty);

    gateway.sendCompleter.complete();
    await tester.pumpAndSettle();
  });

  testWidgets('image and channel rendering work on device', (tester) async {
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await _pumpMobileApp(tester, fixtures.ImageMessagesGateway());
    await _openConversation(tester, 'Chat with Ada', 'chat-1');
    expect(find.byType(Image), findsOneWidget);

    SharedPreferences.setMockInitialValues({});
    await _pumpMobileApp(tester, fixtures.ChannelsGateway());
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Channels'));
    await tester.pumpAndSettle();
    expect(find.text('Engineering'), findsOneWidget);
    await tester.tap(find.text('General'));
    await tester.pumpAndSettle();
    expect(find.text('Channel message'), findsOneWidget);
  });

  testWidgets('call controls and Android audio routing work on device', (
    tester,
  ) async {
    addTearDown(() => tester.binding.setSurfaceSize(null));
    _mockMediaPermissions(tester);
    final calls = fixtures.FakeCallGateway();
    await _pumpMobileApp(tester, fixtures.FakeTeamsGateway(), calls: calls);
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    final testCall = find.text('Make a test call');
    await tester.scrollUntilVisible(
      testCall,
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(testCall);
    await tester.pumpAndSettle();
    expect(calls.started, [microsoftTestCallConversationId]);
    expect(calls.startedVideo, [false]);

    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.connected,
        callId: 'waydroid-call',
        conversationId: microsoftTestCallConversationId,
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Microsoft Test Call Bot'), findsOneWidget);
    expect(find.text('Speaker'), findsOneWidget);
    expect(find.text('Mic on'), findsOneWidget);

    await tester.tap(find.text('Speaker'));
    await tester.pumpAndSettle();
    expect(
      find.text('Earpiece').evaluate().isNotEmpty ||
          find
              .textContaining('Could not change audio output')
              .evaluate()
              .isNotEmpty,
      isTrue,
    );
    await tester.tap(find.text('Mic on'));
    await tester.pumpAndSettle();
    expect(find.text('Muted'), findsOneWidget);

    await tester.tap(find.text('Hang up'));
    await tester.pumpAndSettle();
    expect(calls.hangUps, 1);
  });

  testWidgets('mobile self-chat conversation actions work on device', (
    tester,
  ) async {
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await _pumpMobileApp(tester, fixtures.GalleryMessagesGateway());
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    var self = find.byKey(const ValueKey('conversation-tile-chat-self-chat'));
    await tester.longPress(self);
    await tester.pumpAndSettle();
    expect(find.text('Add to favourites'), findsOneWidget);
    expect(find.text('Mute notifications'), findsOneWidget);
    expect(find.text('Hide brandonp2412'), findsOneWidget);
    await tester.tap(find.text('Add to favourites'));
    await tester.pumpAndSettle();
    expect(find.byIcon(Icons.star_rounded), findsOneWidget);

    SharedPreferences.setMockInitialValues({
      'navigation.favoriteConversations': ['self-chat'],
    });
    await _pumpMobileApp(tester, fixtures.GalleryMessagesGateway());
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    self = find.byKey(const ValueKey('conversation-tile-chat-self-chat'));
    await tester.longPress(self);
    await tester.pumpAndSettle();
    expect(find.text('Remove from favourites'), findsOneWidget);
    await tester.tap(find.text('Remove from favourites'));
    await tester.pumpAndSettle();
    expect(find.byIcon(Icons.star_rounded), findsNothing);

    SharedPreferences.setMockInitialValues({});
    await _pumpMobileApp(tester, fixtures.GalleryMessagesGateway());
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    self = find.byKey(const ValueKey('conversation-tile-chat-self-chat'));
    await tester.longPress(self);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Mute notifications'));
    await tester.pumpAndSettle();
    expect(find.byIcon(Icons.notifications_off_outlined), findsOneWidget);

    SharedPreferences.setMockInitialValues({
      'notifications.mutedConversations': ['self-chat'],
    });
    await _pumpMobileApp(tester, fixtures.GalleryMessagesGateway());
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    self = find.byKey(const ValueKey('conversation-tile-chat-self-chat'));
    await tester.longPress(self);
    await tester.pumpAndSettle();
    expect(find.text('Unmute notifications'), findsOneWidget);
    await tester.tap(find.text('Unmute notifications'));
    await tester.pumpAndSettle();
    expect(find.byIcon(Icons.notifications_off_outlined), findsNothing);

    SharedPreferences.setMockInitialValues({});
    await _pumpMobileApp(tester, fixtures.GalleryMessagesGateway());
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    self = find.byKey(const ValueKey('conversation-tile-chat-self-chat'));
    await tester.longPress(self);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Hide brandonp2412'));
    await tester.pumpAndSettle();
    expect(self, findsNothing);

    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Hidden items'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.text('Hidden items'));
    await tester.pumpAndSettle();
    expect(find.text('brandonp2412'), findsOneWidget);
    await tester.tap(find.text('Restore'));
    await tester.pumpAndSettle();
    await tester.pageBack();
    await tester.pumpAndSettle();
    await tester.pageBack();
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('conversation-tile-chat-self-chat')),
      findsOneWidget,
    );
  });

  testWidgets(
    'mobile self-chat reactions cover every base reaction on device',
    (tester) async {
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final gateway = _SelfReactionGateway();
      await _pumpMobileApp(tester, gateway);
      await _openConversation(tester, 'brandonp2412', 'self-chat');

      const reactions = {
        '👍': 'like',
        '❤️': 'heart',
        '😂': 'laugh',
        '😮': 'surprised',
        '😢': 'sad',
        '😠': 'angry',
      };
      for (final entry in reactions.entries) {
        for (var toggle = 0; toggle < 2; toggle++) {
          await tester.longPress(find.text('React to my self message'));
          await tester.pumpAndSettle();
          final reaction = find.byTooltip(entry.key);
          expect(reaction, findsOneWidget);
          await tester.tap(reaction);
          await tester.pumpAndSettle();
        }
      }
      expect(gateway.reactions, [
        for (final type in reactions.values) ...[type, type],
      ]);
      expect(gateway.selectedReactions, isEmpty);
    },
  );

  testWidgets(
    'mobile self-chat attachment and gallery surfaces work on device',
    (tester) async {
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final gateway = fixtures.GalleryMessagesGateway();
      await _pumpMobileApp(tester, gateway);
      await _openConversation(tester, 'brandonp2412', 'self-chat');

      await tester.tap(find.byTooltip('Add attachment'));
      await tester.pumpAndSettle();
      for (final label in const [
        'Upload file',
        'Image',
        'Open camera',
        'Record audio',
      ]) {
        expect(find.text(label), findsOneWidget);
      }
      await tester.tapAt(const Offset(8, 8));
      await tester.pumpAndSettle();

      final image = find.byType(Image).last;
      await tester.tap(image);
      await tester.pumpAndSettle();
      expect(find.byType(InteractiveViewer), findsOneWidget);
      if (find.byTooltip('Previous image').evaluate().isNotEmpty) {
        await tester.tap(find.byTooltip('Previous image'));
        await tester.pumpAndSettle();
      }
      if (find.byTooltip('Next image').evaluate().isNotEmpty) {
        await tester.tap(find.byTooltip('Next image'));
        await tester.pumpAndSettle();
      }
      await tester.tap(find.byTooltip('Copy image'));
      await tester.pump();
      await tester.tap(find.byTooltip('Close'));
      await tester.pumpAndSettle();
    },
  );

  testWidgets('mobile settings toggles and hidden items work on device', (
    tester,
  ) async {
    addTearDown(() => tester.binding.setSurfaceSize(null));
    _mockBackgroundNotifications(tester);
    final notifications = fixtures.RecordingDesktopNotifications();
    final calls = fixtures.FakeMediaCallGateway();
    await tester.binding.setSurfaceSize(const Size(400, 800));
    await tester.pumpWidget(
      OstApp(
        key: UniqueKey(),
        gateway: fixtures.RestoringAuthGateway(),
        teamsGateway: fixtures.SyncingGateway(),
        callGateway: calls,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();

    final settingsSearch = find.byKey(const ValueKey('settings-search'));
    await tester.enterText(settingsSearch, 'camera');
    await tester.pumpAndSettle();
    expect(find.text('Camera'), findsOneWidget);
    expect(find.text('Enable notifications'), findsNothing);
    await tester.enterText(settingsSearch, '');
    await tester.pumpAndSettle();

    for (final label in const [
      'Direct messages',
      '@mentions',
      'Group chats',
      'Channels',
      'Reactions',
      'Link previews',
    ]) {
      final control = find.text(label);
      await tester.scrollUntilVisible(
        control,
        300,
        scrollable: find.byType(Scrollable).first,
      );
      await tester.tap(control);
      await tester.pump();
      await tester.tap(control);
      await tester.pump();
    }

    final master = find.text('Enable notifications');
    await tester.scrollUntilVisible(
      master,
      -300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(master);
    await tester.pump();
    await tester.tap(master);
    await tester.pump();

    await tester.scrollUntilVisible(
      find.text('Hidden items'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.text('Hidden items'));
    await tester.pumpAndSettle();
    expect(find.text('Nothing is hidden.'), findsOneWidget);
  });

  testWidgets('mobile settings message-limit selector works on device', (
    tester,
  ) async {
    addTearDown(() => tester.binding.setSurfaceSize(null));
    _mockBackgroundNotifications(tester);
    final notifications = fixtures.RecordingDesktopNotifications();
    final calls = fixtures.FakeMediaCallGateway();
    await tester.binding.setSurfaceSize(const Size(400, 800));
    await tester.pumpWidget(
      OstApp(
        key: UniqueKey(),
        gateway: fixtures.RestoringAuthGateway(),
        teamsGateway: fixtures.SyncingGateway(),
        callGateway: calls,
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();

    await tester.scrollUntilVisible(
      find.text('Messages per conversation'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    final limitTile = find.ancestor(
      of: find.text('Messages per conversation'),
      matching: find.byType(ListTile),
    );
    final limitDropdown = find.descendant(
      of: limitTile,
      matching: find.byWidgetPredicate(
        (widget) => widget is DropdownButton<int>,
      ),
    );
    await tester.tap(limitDropdown);
    await tester.pumpAndSettle();
    await tester.tap(find.text('250').last);
    await tester.pumpAndSettle();
    await tester.tap(limitDropdown);
    await tester.pumpAndSettle();
    await tester.tap(find.text('100').last);
    await tester.pumpAndSettle();
  });

  testWidgets('mobile settings sync works on device', (tester) async {
    addTearDown(() => tester.binding.setSurfaceSize(null));
    _mockBackgroundNotifications(tester);
    final settingsSearch = await _openMobileSettings(
      tester,
      fixtures.SyncingGateway(),
    );

    await tester.enterText(settingsSearch, 'sync');
    await tester.pumpAndSettle();
    final sync = find.byTooltip('Sync messages now');
    expect(sync, findsOneWidget);
    await tester.tap(sync);
    await tester.pumpAndSettle();
    expect(find.text('Cached 2 of 2 conversations'), findsOneWidget);
  });

  testWidgets('mobile microphone selector and preview work on device', (
    tester,
  ) async {
    addTearDown(() => tester.binding.setSurfaceSize(null));
    _mockBackgroundNotifications(tester);
    _mockMediaPermissions(tester);
    final calls = fixtures.FakeMediaCallGateway();
    final settingsSearch = await _openMobileSettings(
      tester,
      fixtures.FakeTeamsGateway(),
      calls: calls,
    );

    await tester.enterText(settingsSearch, 'microphone');
    await tester.pumpAndSettle();
    final dropdown = find.byWidgetPredicate(
      (widget) => widget is DropdownButton<String>,
    );
    expect(dropdown, findsOneWidget);
    await tester.ensureVisible(dropdown);
    await tester.pumpAndSettle();
    await tester.tap(dropdown);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Desk microphone').last);
    await tester.pumpAndSettle();
    expect(calls.selectedDevices[MediaDeviceKind.microphone], 'mic-1');
    await tester.tap(find.byKey(const ValueKey('microphone-preview')));
    await tester.pump(const Duration(milliseconds: 350));
    expect(calls.microphonePreviewCalls, greaterThan(0));
    await tester.tap(find.byKey(const ValueKey('microphone-preview')));
    await tester.pumpAndSettle();
  });

  testWidgets('mobile speaker selector and preview work on device', (
    tester,
  ) async {
    addTearDown(() => tester.binding.setSurfaceSize(null));
    _mockBackgroundNotifications(tester);
    final calls = fixtures.FakeMediaCallGateway();
    final settingsSearch = await _openMobileSettings(
      tester,
      fixtures.FakeTeamsGateway(),
      calls: calls,
    );

    await tester.enterText(settingsSearch, 'speaker');
    await tester.pumpAndSettle();
    final dropdown = find.byWidgetPredicate(
      (widget) => widget is DropdownButton<String>,
    );
    expect(dropdown, findsOneWidget);
    await tester.ensureVisible(dropdown);
    await tester.pumpAndSettle();
    await tester.tap(dropdown);
    await tester.pumpAndSettle();
    await tester.tap(find.text('USB headset').last);
    await tester.pumpAndSettle();
    expect(calls.selectedDevices[MediaDeviceKind.speaker], 'speaker-1');
    await tester.tap(find.byKey(const ValueKey('speaker-preview')));
    await tester.pumpAndSettle();
    expect(find.textContaining('Test tone played'), findsOneWidget);
  });

  testWidgets('mobile camera selector and preview failure work on device', (
    tester,
  ) async {
    addTearDown(() => tester.binding.setSurfaceSize(null));
    _mockBackgroundNotifications(tester);
    _mockMediaPermissions(tester);
    final calls = fixtures.FakeMediaCallGateway();
    final settingsSearch = await _openMobileSettings(
      tester,
      fixtures.FakeTeamsGateway(),
      calls: calls,
    );

    await tester.enterText(settingsSearch, 'camera');
    await tester.pumpAndSettle();
    final dropdown = find.byWidgetPredicate(
      (widget) => widget is DropdownButton<String>,
    );
    expect(dropdown, findsOneWidget);
    await tester.ensureVisible(dropdown);
    await tester.pumpAndSettle();
    await tester.tap(dropdown);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Virtual camera').last);
    await tester.pumpAndSettle();
    expect(calls.selectedDevices[MediaDeviceKind.camera], 'camera-1');
    await tester.tap(find.byKey(const ValueKey('camera-preview')));
    await tester.pumpAndSettle();
    expect(
      find.textContaining('Could not open the selected camera'),
      findsOneWidget,
    );
  });

  testWidgets('mobile notification and debug diagnostics work on device', (
    tester,
  ) async {
    addTearDown(() => tester.binding.setSurfaceSize(null));
    _mockBackgroundNotifications(tester);
    final notifications = fixtures.RecordingDesktopNotifications();
    final settingsSearch = await _openMobileSettings(
      tester,
      fixtures.FakeTeamsGateway(),
      notifications: notifications,
    );

    await tester.enterText(settingsSearch, 'send test notification');
    await tester.pumpAndSettle();
    await tester.tap(find.text('Send test notification'));
    await tester.pumpAndSettle();
    expect(notifications.titles, ['Microslop test notification']);

    await tester.enterText(settingsSearch, 'debug log');
    await tester.pumpAndSettle();
    await tester.tap(find.text('Debug log'));
    await tester.pumpAndSettle();
    expect(find.text('Debug log'), findsOneWidget);
  });

  testWidgets('mobile account menu can be inspected on device', (tester) async {
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.binding.setSurfaceSize(const Size(400, 800));
    await tester.pumpWidget(
      OstApp(
        key: UniqueKey(),
        gateway: fixtures.MultiAccountRestoringAuthGateway(),
        teamsGateway: fixtures.FakeTeamsGateway(),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Account: Test User'));
    await tester.pumpAndSettle();
    expect(find.text('Other Account'), findsOneWidget);
    expect(find.text('Add account'), findsOneWidget);
    await tester.tapAt(const Offset(8, 8));
    await tester.pumpAndSettle();
    expect(find.text('Other Account'), findsNothing);
  });

  testWidgets('video call reports a missing camera on Waydroid', (
    tester,
  ) async {
    addTearDown(() => tester.binding.setSurfaceSize(null));
    _mockMediaPermissions(tester);
    final calls = fixtures.FakeCallGateway();
    await _pumpMobileApp(tester, fixtures.FakeTeamsGateway(), calls: calls);
    await _openConversation(tester, 'Chat with Ada', 'chat-1');

    await tester.tap(find.byTooltip('Start call'));
    await tester.pumpAndSettle();
    expect(find.byType(BottomSheet), findsOneWidget);
    expect(find.byIcon(Icons.call_rounded), findsOneWidget);
    expect(find.byIcon(Icons.videocam_rounded), findsOneWidget);
    await tester.tap(find.text('Video call'));
    await tester.pumpAndSettle();

    expect(calls.started, isEmpty);
    expect(
      find.textContaining('No usable camera is available'),
      findsOneWidget,
    );
  });
}
