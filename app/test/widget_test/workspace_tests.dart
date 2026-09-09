part of '../widget_test.dart';

void registerWorkspaceTests() {
  testWidgets('uses the FluffyChat-inspired rounded workspace shell', (
    tester,
  ) async {
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: FakeTeamsGateway()),
    );
    await tester.pumpAndSettle();

    expect(find.byType(Scaffold), findsOneWidget);
    expect(find.byType(ClipRRect), findsWidgets);
    expect(find.text('Direct messages'), findsOneWidget);
  });

  testWidgets('does not render the desktop navigation icon rail', (
    tester,
  ) async {
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: FakeTeamsGateway()),
    );
    await tester.pumpAndSettle();

    expect(find.byIcon(Icons.forum_rounded), findsNothing);
    expect(find.byIcon(Icons.chat_bubble_rounded), findsNothing);
    expect(find.byIcon(Icons.groups_outlined), findsNothing);
    expect(find.byIcon(Icons.folder_outlined), findsNothing);
  });

  test('requests the first direct-message page by default', () async {
    BigInt? requestedLimit;
    final gateway = RustTeamsGateway(
      chatLoader: ({required limit}) async {
        requestedLimit = limit;
        return const [];
      },
    );

    await gateway.listChats();

    expect(requestedLimit, BigInt.from(50));
  });

  test('times out a stalled direct-message request', () async {
    final gateway = RustTeamsGateway(
      chatTimeout: const Duration(milliseconds: 1),
      chatLoader: ({required limit}) => Completer<List<rust.Chat>>().future,
    );

    await expectLater(gateway.listChats(), throwsA(isA<TimeoutException>()));
  });

  test('times out a stalled work-session restore', () async {
    final gateway = RustAuthGateway(
      restoreTimeout: const Duration(milliseconds: 1),
      sessionRestorer: () => Completer<auth_rust.AuthStatus>().future,
    );

    await expectLater(
      gateway.restoreWorkSession(),
      throwsA(isA<TimeoutException>()),
    );
  });

  testWidgets('opens Microsoft sign-in and copies the device code', (
    tester,
  ) async {
    Uri? launchedUri;
    String? copiedText;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          copiedText =
              (call.arguments as Map<Object?, Object?>)['text'] as String;
        }
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );
    await tester.pumpWidget(
      OstApp(
        gateway: PendingLoginAuthGateway(),
        externalUrlLauncher: (uri) async {
          launchedUri = uri;
          return true;
        },
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Start Microsoft sign-in'));
    await tester.pumpAndSettle();

    expect(launchedUri, Uri.parse('https://microsoft.com/devicelogin'));
    expect(copiedText, 'ABC-123');
    expect(find.text('ABC-123'), findsOneWidget);
    expect(find.text('Open Microsoft sign-in'), findsOneWidget);
    expect(find.text('Waiting for Microsoft…'), findsOneWidget);
  });

  testWidgets('completed sign-in survives active account lookup failure', (
    tester,
  ) async {
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (_) async => null,
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );
    final auth = FailingActiveAccountLookupLoginAuthGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: auth,
        teamsGateway: FakeTeamsGateway(),
        externalUrlLauncher: (_) async => true,
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Start Microsoft sign-in'));
    await tester.pumpAndSettle();
    expect(auth.completionReads, 1);
    await tester.tap(find.text('I have signed in'));
    await tester.pumpAndSettle();
    expect(auth.completionReads, 2);

    expect(find.text('Chat with Ada'), findsOneWidget);
    expect(find.text('Start Microsoft sign-in'), findsNothing);
  });

  testWidgets('copies the whole error text to the clipboard', (tester) async {
    String? copiedText;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          copiedText =
              (call.arguments as Map<Object?, Object?>)['text'] as String;
        }
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );

    await tester.pumpWidget(OstApp(gateway: FailingAuthGateway()));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Start Microsoft sign-in'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Copy error text'));
    await tester.pumpAndSettle();

    expect(
      copiedText,
      'Exception: Unable to store the secure sign-in credential.',
    );
  });

  testWidgets('uses a restored cached work session', (tester) async {
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: FakeTeamsGateway()),
    );
    await tester.pumpAndSettle();

    expect(find.text('Chat with Ada'), findsOneWidget);
    expect(find.text('Start Microsoft sign-in'), findsNothing);
  });

  testWidgets('restored session survives active account lookup failure', (
    tester,
  ) async {
    await tester.pumpWidget(
      OstApp(
        gateway: FailingActiveAccountLookupAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Chat with Ada'), findsOneWidget);
    expect(find.text('Retry session restore'), findsNothing);
  });

  testWidgets('does not offer sign-in when session restore fails', (
    tester,
  ) async {
    await tester.pumpWidget(OstApp(gateway: FailingRestoreAuthGateway()));
    await tester.pumpAndSettle();

    expect(find.text('Start Microsoft sign-in'), findsNothing);
    expect(find.text('Retry session restore'), findsOneWidget);
  });

  testWidgets('moves focused conversation identity and calls into app bar', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(400, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: FakeTeamsGateway()),
    );
    await tester.pumpAndSettle();

    expect(find.text('Chats'), findsOneWidget);
    expect(find.byTooltip('Refresh'), findsNothing);
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    expect(find.text('Chat with Ada'), findsOneWidget);
    expect(find.byTooltip('Start call'), findsOneWidget);
    expect(find.byTooltip('Start audio call'), findsNothing);
    expect(find.byTooltip('Start video call'), findsNothing);
    expect(find.byTooltip('Account: Test User'), findsNothing);
  });

  testWidgets('shows separate desktop audio and video call actions', (
    tester,
  ) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.windows;
    addTearDown(() => debugDefaultTargetPlatformOverride = null);
    await tester.binding.setSurfaceSize(const Size(1024, 768));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: FakeTeamsGateway()),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();

    expect(find.byTooltip('Start audio call'), findsOneWidget);
    expect(find.byTooltip('Start video call'), findsOneWidget);
    expect(find.byTooltip('Start call'), findsNothing);
    expect(find.byIcon(Icons.call_outlined), findsOneWidget);
    expect(find.byIcon(Icons.videocam_outlined), findsOneWidget);
    debugDefaultTargetPlatformOverride = null;
  });

  testWidgets('uses an Android call picker bottom sheet', (tester) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.android;
    addTearDown(() => debugDefaultTargetPlatformOverride = null);
    await tester.binding.setSurfaceSize(const Size(400, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: FakeTeamsGateway()),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Start call'));
    await tester.pumpAndSettle();

    expect(find.byType(BottomSheet), findsOneWidget);
    expect(find.text('Audio call'), findsOneWidget);
    expect(find.text('Video call'), findsOneWidget);
    expect(find.byIcon(Icons.call_rounded), findsOneWidget);
    expect(find.byIcon(Icons.videocam_rounded), findsOneWidget);
    debugDefaultTargetPlatformOverride = null;
  });

  testWidgets('shows desktop navigation categories in the mobile drawer', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(400, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: SplitChatsGateway(),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();

    expect(find.text('Groups'), findsOneWidget);
    expect(find.text('Direct messages'), findsOneWidget);
    expect(find.text('Settings'), findsOneWidget);
    expect(find.text('Search...'), findsOneWidget);
  });

  testWidgets('loads more chats when the groups section reaches its end', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(400, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = PaginatedGroupsGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    expect(gateway.requestedLimits, [50]);
    await tester.tap(find.text('Groups'));
    await tester.pumpAndSettle();

    final groupsList = find.descendant(
      of: find.byKey(const PageStorageKey<String>('group-chat-list')),
      matching: find.byType(Scrollable),
    );
    await tester.scrollUntilVisible(
      find.text('Group 50'),
      500,
      scrollable: groupsList,
    );
    await tester.pumpAndSettle();

    expect(gateway.requestedLimits, [50, 100]);
  });

  testWidgets('workspace reload preserves an in-flight larger chat limit', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(400, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = PaginationRefreshRaceGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Groups'));
    await tester.pumpAndSettle();
    final groupsList = find.descendant(
      of: find.byKey(const PageStorageKey<String>('group-chat-list')),
      matching: find.byType(Scrollable),
    );
    await tester.scrollUntilVisible(
      find.text('Group 50'),
      500,
      scrollable: groupsList,
    );
    await gateway.paginationStarted.future;

    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pump();
    await tester.pump();
    gateway.releasePagination.complete();
    await tester.pumpAndSettle();

    expect(gateway.requestedLimits, [50, 100, 100]);
    await tester.scrollUntilVisible(
      find.text('Group 70'),
      500,
      scrollable: groupsList,
    );
    expect(find.text('Group 70'), findsOneWidget);
  });

  testWidgets('long pressing a section hides it until restored in settings', (
    tester,
  ) async {
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: SplitChatsGateway(),
      ),
    );
    await tester.pumpAndSettle();

    await tester.longPress(find.text('Direct messages'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Hide Direct messages'));
    await tester.pumpAndSettle();

    expect(find.text('Direct messages'), findsNothing);
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Hidden items'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.ensureVisible(find.text('Hidden items'));
    await tester.pump();
    await tester.tap(find.text('Hidden items'));
    await tester.pumpAndSettle();
    expect(find.text('Direct messages'), findsOneWidget);
    await tester.tap(find.text('Restore'));
    await tester.pumpAndSettle();
    await tester.pageBack();
    await tester.pumpAndSettle();
    expect(find.text('Hidden items'), findsOneWidget);
    await tester.pageBack();
    await tester.pumpAndSettle();
    expect(find.text('Direct messages'), findsOneWidget);
  });

  testWidgets('restores the last selected chat on launch', (tester) async {
    SharedPreferences.setMockInitialValues({
      'workspace.chats': jsonEncode(
        const CachedWorkspace(
          chats: [
            CachedConversation(
              id: 'chat-1',
              name: 'Chat with Ada',
              isGroup: false,
            ),
          ],
          teams: [],
          selectedChatId: 'chat-1',
          messages: {
            'chat-1': [
              CachedMessage(
                id: 'cached',
                sender: 'Ada',
                isFromCurrentUser: false,
                timestamp: 'Now',
                content: 'Restored message',
              ),
            ],
          },
        ).toJson(),
      ),
    });

    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: FakeTeamsGateway()),
    );
    await tester.pumpAndSettle();

    expect(find.byTooltip('Start audio call'), findsOneWidget);
    expect(find.byTooltip('Start video call'), findsOneWidget);
  });

  testWidgets('startup stops the Android watcher when notifications are off', (
    tester,
  ) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.android;
    addTearDown(() => debugDefaultTargetPlatformOverride = null);
    SharedPreferences.setMockInitialValues({
      'notifications.enabled': false,
      'notifications.backgroundWatcher': true,
    });
    const channel = MethodChannel('microslop/background_notifications');
    final enabledValues = <bool>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, (
      call,
    ) async {
      if (call.method == 'setEnabled') {
        enabledValues.add(
          (call.arguments as Map<Object?, Object?>)['enabled'] as bool,
        );
      }
      if (call.method == 'isRunning') return false;
      return null;
    });
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        channel,
        null,
      ),
    );

    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: FakeTeamsGateway()),
    );
    await tester.pumpAndSettle();

    expect(enabledValues, [false]);
    debugDefaultTargetPlatformOverride = null;
  });

  testWidgets('uses cross-platform notification wording in settings', (
    tester,
  ) async {
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: FakeTeamsGateway()),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();

    expect(find.text('Enable notifications'), findsOneWidget);
    expect(
      find.text('Master switch for all message notifications'),
      findsOneWidget,
    );
    expect(find.text('Mention notifications'), findsNothing);
    expect(find.textContaining('Desktop'), findsNothing);
  });

  testWidgets('searches settings and hides unrelated items', (tester) async {
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: FakeTeamsGateway()),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();

    await tester.enterText(
      find.byKey(const ValueKey('settings-search')),
      'camera',
    );
    await tester.pumpAndSettle();

    expect(find.text('Camera'), findsOneWidget);
    expect(find.text('Audio and video'), findsOneWidget);
    expect(find.text('Enable notifications'), findsNothing);
    expect(find.text('Data sync'), findsNothing);
  });

  testWidgets('settings use compact horizontal gutters on phones', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(400, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: FakeTeamsGateway()),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();

    expect(
      tester.getTopLeft(find.text('Enable notifications')).dx,
      lessThan(32),
    );
    expect(
      tester.getTopLeft(find.text('Notifications')).dx,
      greaterThanOrEqualTo(20),
    );
  });

  testWidgets('groups media devices separately from notifications', (
    tester,
  ) async {
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: FakeTeamsGateway()),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Audio and video'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.pumpAndSettle();

    expect(find.text('Audio and video'), findsOneWidget);
    expect(find.text('Microphone'), findsOneWidget);
    expect(find.text('Speaker or headset'), findsOneWidget);
    expect(find.text('Camera'), findsOneWidget);
    expect(find.text('Speaker preview'), findsOneWidget);
    expect(find.text('Camera preview'), findsOneWidget);
    expect(
      find.text('Select a microphone or tap Test for a live level meter'),
      findsOneWidget,
    );
  });

  testWidgets(
    'microphone settings preview updates continuously until stopped',
    (tester) async {
      final calls = FakeMediaCallGateway();
      await tester.pumpWidget(
        OstApp(
          gateway: RestoringAuthGateway(),
          teamsGateway: FakeTeamsGateway(),
          callGateway: calls,
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Settings'));
      await tester.pumpAndSettle();
      await tester.scrollUntilVisible(
        find.text('Microphone'),
        300,
        scrollable: find.byType(Scrollable).first,
      );
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(const ValueKey('microphone-preview')));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 350));
      await tester.pump();
      expect(calls.microphonePreviewCalls, greaterThanOrEqualTo(2));
      expect(find.textContaining('current level'), findsOneWidget);
      expect(
        tester
            .widget<LinearProgressIndicator>(
              find.byKey(const ValueKey('microphone-level')),
            )
            .value,
        greaterThan(0),
      );
      await tester.tap(find.byKey(const ValueKey('microphone-preview')));
      await tester.pump();
      final stoppedAt = calls.microphonePreviewCalls;
      await tester.pump(const Duration(milliseconds: 350));
      expect(calls.microphonePreviewCalls, stoppedAt);
    },
  );

  testWidgets('camera settings preview updates continuously until stopped', (
    tester,
  ) async {
    final calls = FakeMediaCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Camera preview'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byKey(const ValueKey('camera-preview')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 350));
    await tester.pump();
    expect(calls.cameraPreviewCalls, greaterThanOrEqualTo(2));

    await tester.tap(find.byKey(const ValueKey('camera-preview')));
    await tester.pump();
    final stoppedAt = calls.cameraPreviewCalls;
    await tester.pump(const Duration(milliseconds: 350));
    expect(calls.cameraPreviewCalls, stoppedAt);
  });

  testWidgets(
    'account switching stops the old message listener before rebuilding',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final auth = MultiAccountRestoringAuthGateway();
      final teams = DelayedMessageStopGateway();
      await tester.pumpWidget(OstApp(gateway: auth, teamsGateway: teams));
      await tester.pumpAndSettle();

      expect(teams.messageEventStarts, 1);
      await tester.tap(find.byTooltip('Account: Test User'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Other Account'));
      await tester.pump();
      await teams.stopStarted.future;

      expect(auth.switchedAccounts, ['account-secondary']);
      expect(teams.messageEventStarts, 1);
      expect(teams.stopCount, 1);

      teams.releaseStop.complete();
      await tester.pumpAndSettle();

      expect(auth.switchedAccounts, ['account-secondary']);
      expect(teams.messageEventStarts, 2);
      expect(teams.stopCount, 1);
    },
  );

  testWidgets('account switching does not re-read the known active account', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final auth = FailingPostSwitchAccountLookupAuthGateway();
    final teams = DelayedMessageStopGateway();
    await tester.pumpWidget(OstApp(gateway: auth, teamsGateway: teams));
    await tester.pumpAndSettle();

    expect(auth.activeAccountIdReads, 1);
    await tester.tap(find.byTooltip('Account: Test User'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Other Account'));
    await tester.pump();
    await teams.stopStarted.future;
    teams.releaseStop.complete();
    await tester.pumpAndSettle();

    expect(auth.activeId, 'account-secondary');
    expect(auth.switchedAccounts, ['account-secondary']);
    expect(auth.activeAccountIdReads, 1);
    expect(teams.messageEventStarts, 2);
  });

  testWidgets('account switching completes when realtime shutdown fails', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final auth = MultiAccountRestoringAuthGateway();
    final teams = FailingMessageStopGateway();
    final calls = FakeCallGateway();
    await tester.pumpWidget(
      OstApp(gateway: auth, teamsGateway: teams, callGateway: calls),
    );
    await tester.pumpAndSettle();

    expect(teams.messageEventStarts, 1);
    await tester.tap(find.byTooltip('Account: Test User'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Other Account'));
    await tester.pumpAndSettle();

    expect(auth.activeId, 'account-secondary');
    expect(auth.switchedAccounts, ['account-secondary']);
    expect(teams.messageEventStarts, 2);
    expect(teams.stopCount, 1);
    expect(calls.stopCount, 1);
    expect(find.text('Start Microsoft sign-in'), findsNothing);
  });

  testWidgets(
    'account switching declines an incoming call before auth changes',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final auth = MultiAccountRestoringAuthGateway();
      final calls = AccountAwareCallGateway(() => auth.activeId);
      await tester.pumpWidget(
        OstApp(
          gateway: auth,
          teamsGateway: FakeTeamsGateway(),
          callGateway: calls,
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.byTooltip('Account: Test User'));
      await tester.pumpAndSettle();
      calls.callEvents.add(
        const CallUpdate(
          kind: CallUpdateKind.incoming,
          callId: 'call-before-switch',
          displayName: 'Ada Lovelace',
        ),
      );
      await tester.pump();
      await tester.tap(find.text('Other Account'));
      await tester.pumpAndSettle();

      expect(calls.declined, ['call-before-switch']);
      expect(calls.declineAccountIds, ['account-primary']);
      expect(auth.switchedAccounts, ['account-secondary']);
    },
  );

  testWidgets('account switching aborts when incoming call decline fails', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final auth = MultiAccountRestoringAuthGateway();
    final calls = FailingAccountAwareCallGateway(() => auth.activeId);
    await tester.pumpWidget(
      OstApp(
        gateway: auth,
        teamsGateway: FakeTeamsGateway(),
        callGateway: calls,
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Account: Test User'));
    await tester.pumpAndSettle();
    calls.callEvents.add(
      const CallUpdate(
        kind: CallUpdateKind.incoming,
        callId: 'call-before-failed-switch',
        displayName: 'Ada Lovelace',
      ),
    );
    await tester.pump();
    await tester.tap(find.text('Other Account'));
    await tester.pumpAndSettle();

    expect(calls.declined, ['call-before-failed-switch']);
    expect(calls.declineAccountIds, ['account-primary']);
    expect(auth.switchedAccounts, isEmpty);
    expect(find.text('Incoming call'), findsOneWidget);
  });

  testWidgets(
    'account switching stops the old call listener before rebuilding',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final auth = MultiAccountRestoringAuthGateway();
      final calls = DelayedCallStopGateway();
      await tester.pumpWidget(
        OstApp(
          gateway: auth,
          teamsGateway: FakeTeamsGateway(),
          callGateway: calls,
        ),
      );
      await tester.pumpAndSettle();

      expect(calls.eventStarts, 1);
      await tester.tap(find.byTooltip('Account: Test User'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Other Account'));
      await tester.pump();
      await calls.stopStarted.future;

      expect(auth.switchedAccounts, ['account-secondary']);
      expect(calls.eventStarts, 1);
      expect(calls.stopCount, 1);

      calls.releaseStop.complete();
      await tester.pumpAndSettle();

      expect(auth.switchedAccounts, ['account-secondary']);
      expect(calls.eventStarts, 2);
      expect(calls.stopCount, 1);
    },
  );

  testWidgets('adding an account continues when realtime shutdown fails', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final auth = MultiAccountPendingLoginAuthGateway();
    final teams = FailingMessageStopGateway();
    final calls = FakeCallGateway();
    await tester.pumpWidget(
      OstApp(
        gateway: auth,
        teamsGateway: teams,
        callGateway: calls,
        externalUrlLauncher: (_) async => true,
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Account: Test User'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Add account'));
    await tester.pumpAndSettle();

    expect(find.text('ABC-123'), findsOneWidget);
    expect(teams.messageEventStarts, 1);
    expect(teams.stopCount, 1);
    expect(calls.stopCount, 1);
  });

  testWidgets(
    'adding an account stops old realtime events before leaving workspace',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final auth = MultiAccountPendingLoginAuthGateway();
      final teams = DelayedMessageStopGateway();
      await tester.pumpWidget(OstApp(gateway: auth, teamsGateway: teams));
      await tester.pumpAndSettle();

      await tester.tap(find.byTooltip('Account: Test User'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Add account'));
      await tester.pump();
      await teams.stopStarted.future;

      expect(teams.messageEventStarts, 1);
      expect(teams.stopCount, 1);
      expect(find.text('ABC-123'), findsNothing);

      teams.releaseStop.complete();
      await tester.pumpAndSettle();

      expect(find.text('ABC-123'), findsOneWidget);
      expect(teams.messageEventStarts, 1);
    },
  );

  testWidgets('sign out completes when realtime shutdown fails', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final auth = MultiAccountRestoringAuthGateway();
    final teams = FailingMessageStopGateway();
    final calls = FakeCallGateway();
    await tester.pumpWidget(
      OstApp(gateway: auth, teamsGateway: teams, callGateway: calls),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Account: Test User'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Sign out'));
    await tester.pump();
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 10)),
    );
    await tester.pumpAndSettle();

    expect(auth.logoutCount, 1);
    expect(find.text('Start Microsoft sign-in'), findsOneWidget);
    expect(teams.stopCount, 1);
    expect(calls.stopCount, 1);
  });

  testWidgets('desktop profile menu switches between saved accounts', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final auth = MultiAccountRestoringAuthGateway();
    await tester.pumpWidget(
      OstApp(gateway: auth, teamsGateway: FakeTeamsGateway()),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Account: Test User'));
    await tester.pumpAndSettle();
    expect(find.text('Other Account'), findsOneWidget);
    expect(find.text('Add account'), findsOneWidget);

    await tester.tap(find.text('Other Account'));
    await tester.pumpAndSettle();
    expect(auth.switchedAccounts, ['account-secondary']);
  });

  testWidgets('keeps media settings readable and debugging actions on phones', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(384, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final notifications = RecordingDesktopNotifications();
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: FakeTeamsGateway(),
        notifications: notifications,
      ),
    );
    await tester.pumpAndSettle();

    expect(find.byTooltip('Application log'), findsNothing);
    await tester.tap(find.byTooltip('Open navigation'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();

    await tester.scrollUntilVisible(
      find.text('Speaker or headset'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.pumpAndSettle();
    expect(
      tester.getSize(find.text('Speaker or headset')).width,
      greaterThan(100),
    );
    expect(tester.takeException(), isNull);

    await tester.scrollUntilVisible(
      find.text('Send test notification'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.text('Send test notification'));
    await tester.pumpAndSettle();
    expect(notifications.titles, ['Microslop test notification']);

    await tester.scrollUntilVisible(
      find.text('Debug log'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.ensureVisible(find.text('Debug log'));
    await tester.pump();
    await tester.tap(find.text('Debug log'));
    await tester.pumpAndSettle();
    expect(find.text('Debug log'), findsOneWidget);
  });

  testWidgets('copies the complete debug log', (tester) async {
    AppLog.entries.value = const ['first error', 'second error'];
    addTearDown(() => AppLog.entries.value = []);
    String? copiedText;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          copiedText =
              (call.arguments as Map<Object?, Object?>)['text'] as String;
        }
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );

    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: FakeTeamsGateway()),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Debug log'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.ensureVisible(find.text('Debug log'));
    await tester.pump();
    await tester.tap(find.text('Debug log'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Copy debug log'));
    await tester.pump();

    expect(copiedText, 'first error\n\nsecond error');
  });

  testWidgets('data sync reports deterministic conversation progress', (
    tester,
  ) async {
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: SyncingGateway()),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Data sync'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.pumpAndSettle();

    expect(find.text('Messages per conversation'), findsOneWidget);
    await tester.tap(find.text('Sync messages now'));
    await tester.pumpAndSettle();

    expect(find.text('Cached 2 of 2 conversations'), findsOneWidget);
    expect(
      tester
          .widget<LinearProgressIndicator>(
            find.byWidgetPredicate(
              (widget) =>
                  widget is LinearProgressIndicator && widget.value == 1,
            ),
          )
          .value,
      1,
    );
  });

  testWidgets('data sync keeps running when settings is closed', (
    tester,
  ) async {
    final gateway = PersistentSyncingGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Data sync'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.text('Sync messages now'));
    await gateway.started.future;
    await tester.pump();

    expect(find.text('1 of 2 conversations cached'), findsOneWidget);
    await tester.pageBack();
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Data sync'),
      300,
      scrollable: find.byType(Scrollable).first,
    );

    expect(find.text('1 of 2 conversations cached'), findsOneWidget);
    expect(find.text('Syncing messages…'), findsOneWidget);

    gateway.finish.complete();
    await tester.pumpAndSettle();
    expect(find.text('Cached 2 of 2 conversations'), findsOneWidget);
  });

  testWidgets('disposing workspace while data sync runs is safe', (
    tester,
  ) async {
    final gateway = PersistentSyncingGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Data sync'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.text('Sync messages now'));
    await gateway.started.future;
    await tester.pumpWidget(const SizedBox.shrink());

    gateway.finish.complete();
    await tester.pump();

    expect(tester.takeException(), isNull);
  });

  testWidgets('data sync cannot overwrite newer live history', (tester) async {
    final gateway = DelayedSyncRefreshRaceGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Data sync'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.text('Sync messages now'));
    await gateway.syncStarted.future;
    await tester.pageBack();
    await tester.pumpAndSettle();

    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    expect(selectableMessage('Fresh live history'), findsOneWidget);

    gateway.releaseSync.complete();
    await tester.pumpAndSettle();

    expect(selectableMessage('Fresh live history'), findsOneWidget);
    expect(selectableMessage('Stale synced history'), findsNothing);
  });

  testWidgets('coalesces rapid workspace reloads on resume', (tester) async {
    final gateway = BurstyWorkspaceLoadGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    for (var index = 0; index < 4; index++) {
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
      await tester.pump();
    }
    await gateway.resumeLoadStarted.future;
    await tester.pump();
    gateway.releaseResumeLoads.complete();
    await tester.pumpAndSettle();

    expect(gateway.maxConcurrentResumeLoads, 1);
    expect(gateway.listCalls, lessThanOrEqualTo(3));
  });

  testWidgets('data sync preserves chats discovered while it is running', (
    tester,
  ) async {
    final gateway = SyncChatDiscoveryRaceGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    expect(find.text('Chat with Ada'), findsOneWidget);
    expect(find.text('Chat with Grace'), findsNothing);

    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Data sync'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.text('Sync messages now'));
    await gateway.syncStarted.future;
    await tester.pageBack();
    await tester.pumpAndSettle();

    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.paused);
    final listCallsBeforeResume = gateway.listCalls;
    gateway.revealGrace = true;
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pumpAndSettle();
    expect(gateway.listCalls, greaterThan(listCallsBeforeResume));
    expect(find.text('Chat with Ada'), findsOneWidget);
    expect(find.text('Chat with Grace'), findsOneWidget);

    gateway.releaseSync.complete();
    await tester.pumpAndSettle();

    expect(find.text('Chat with Grace'), findsOneWidget);
  });

  testWidgets('data sync preserves an optimistic send', (tester) async {
    final gateway = PendingSendSyncRaceGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Chat with Ada'));
    await tester.pumpAndSettle();
    expect(selectableMessage('Baseline history'), findsOneWidget);

    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Data sync'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.text('Sync messages now'));
    await gateway.syncStarted.future;
    await tester.pageBack();
    await tester.pumpAndSettle();

    final composer = find.byType(TextField).last;
    await tester.enterText(composer, 'Pending local send');
    await tester.tap(find.byTooltip('Send message'));
    await gateway.sendStarted.future;
    await tester.pump();
    expect(selectableMessage('Pending local send'), findsOneWidget);

    gateway.releaseSync.complete();
    await tester.pumpAndSettle();

    expect(selectableMessage('Pending local send'), findsOneWidget);
    gateway.releaseSend.complete();
    await tester.pumpAndSettle();
  });

  testWidgets('completed data sync opens cached chat without a spinner', (
    tester,
  ) async {
    final gateway = CachedSyncResponsiveGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Data sync'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.text('Sync messages now'));
    await tester.pumpAndSettle();
    await tester.pageBack();
    await tester.pumpAndSettle();

    await tester.tap(find.text('Chat with Ada'));
    await tester.pump();

    expect(selectableMessage('Synced history'), findsOneWidget);
    expect(find.byType(CircularProgressIndicator), findsNothing);

    gateway.completeRefresh();
    await tester.pumpAndSettle();
  });

  testWidgets('completed data sync makes the full DM list locally scrollable', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final gateway = FullyCachedNavigationGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    expect(gateway.requestedLimits, [50]);
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.text('Data sync'),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.text('Sync messages now'));
    await tester.pumpAndSettle();
    await tester.pageBack();
    await tester.pumpAndSettle();

    final directMessages = find.descendant(
      of: find.byKey(const PageStorageKey<String>('direct-chat-list')),
      matching: find.byType(Scrollable),
    );
    await tester.scrollUntilVisible(
      find.text('Chat 80'),
      500,
      scrollable: directMessages,
    );
    await tester.pumpAndSettle();

    expect(find.text('Chat 80'), findsOneWidget);
    expect(gateway.requestedLimits, [50]);
    expect(find.byType(CircularProgressIndicator), findsNothing);
    final prefs = await SharedPreferences.getInstance();
    expect(prefs.getBool('workspace.allChatsCached'), isTrue);
  });

  testWidgets('legacy completed sync preloads the full chat page on launch', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({
      'workspace.chats': jsonEncode(
        const CachedWorkspace(
          chats: [
            CachedConversation(id: 'chat-1', name: 'Chat 1', isGroup: false),
          ],
          teams: [],
          selectedChatId: null,
          messages: {},
        ).toJson(),
      ),
      'messages.syncedConversations': ['chat-1'],
    });
    final gateway = FullyCachedNavigationGateway();
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: gateway),
    );
    await tester.pumpAndSettle();

    expect(gateway.requestedLimits, [500]);
    expect(find.text('Chat 80'), findsNothing);
  });

  testWidgets('opens a centered destination palette with control K', (
    tester,
  ) async {
    await tester.pumpWidget(
      OstApp(gateway: RestoringAuthGateway(), teamsGateway: FakeTeamsGateway()),
    );
    await tester.pumpAndSettle();

    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyK);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pumpAndSettle();

    expect(find.byKey(const ValueKey('command-palette')), findsOneWidget);
    expect(find.text('Go to…'), findsOneWidget);
    expect(
      find.descendant(
        of: find.byKey(const ValueKey('command-palette')),
        matching: find.text('Chat with Ada'),
      ),
      findsOneWidget,
    );
    expect(find.text('Settings'), findsWidgets);
  });

  testWidgets('omits the only visible navigation section heading', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({
      'workspace.chats': jsonEncode(
        const CachedWorkspace(
          chats: [],
          teams: [],
          selectedChatId: null,
          hiddenSectionIds: {'channels', 'groups'},
          messages: {},
        ).toJson(),
      ),
    });
    await tester.pumpWidget(
      OstApp(
        gateway: RestoringAuthGateway(),
        teamsGateway: SplitChatsGateway(),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Direct messages'), findsNothing);
    expect(find.text('Chat with Ada'), findsOneWidget);
  });
}
